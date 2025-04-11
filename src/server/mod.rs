use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::{io, thread};

use lock::ProjectLock;
use once_cell::sync::Lazy;
use proto::ResponseData;

use self::proto::{Message, Request, Response};
use crate::error::Error;
use crate::project::Project;
use crate::ts;

mod lock;
mod method;
mod proto;

trait ToJson {
    fn to_json(&self) -> Result<String, serde_json::Error>;
}

/// This is our project dir singleton. It will likely be refactored, but also likely not.
/// It's buried within a couple layers of abstraction. The Lazy is because PathBuf does not have
/// a static new(), RwLock is so we can have thread-safe interior mutability.
static PROJECT_DIR: Lazy<RwLock<PathBuf>> = Lazy::new(Default::default);

/// This error type exists to wrap library errors into a single easy-to-use package.
#[derive(thiserror::Error, Debug)]
#[repr(isize)]
pub enum ServerError {
    /// A partial implementation of the error variants described by the JRPC spec.
    #[error("Failed to serialize JSON: {0:?}")]
    InvalidJson(#[from] serde_json::Error) = -32700,

    #[error("The method {0} is not valid.")]
    InvalidMethod(String) = -32601,

    #[error("Recieved invalid params for method {0}: {1}")]
    InvalidParams(String, String) = -32602,

    #[error("")]
    InvalidContext = 0,
}

impl Error {
    pub fn discriminant(&self) -> isize {
        // SAFETY: `Self` is `repr(isize)` with layout `repr(C)`, with each variant having an isize
        // as its first field, so we can access this value without a pointer offset.
        unsafe { *<*const _>::from(self).cast::<isize>() }
    }
}

impl ToJson for Result<Response, Error> {
    fn to_json(&self) -> Result<String, serde_json::Error> {
        todo!()
    }
}

/// Runtime context for the server. This is mutable state, protected through a RwLock.
/// Mutations require a lock first, attainable through ::lock().
struct Runtime {
    tx: Sender<Message>,
    proj: Arc<Project>,
}

impl Runtime {
    pub fn send(&self, response: Response) {
        self.tx
            .send(Message::Response(response))
            .expect("Failed to write to mpsc tx channel.");
    }
}

/// Runtime context, mutable or otherwise. This contains the project, by which most
/// project-specific ops go through.
struct RtContext {
    pub project: Project,
    pub lock: ProjectLock,
}

/// Create the server runtime from the provided read and write channels.
/// This lives for the lifespan of the process.
pub async fn spawn(_read: impl Read, _write: impl Write, project_dir: &Path) -> Result<(), Error> {
    let (tx, rx) = mpsc::channel::<Message>();
    let cancel = RwLock::new(false);

    // This thread recieves internal mpsc messages, serializes, and writes them to stdout.
    thread::spawn(move || respond_msg(rx, cancel));

    // Begin looping over stdin messages.
    let stdin = io::stdin();
    let mut line = String::new();

    let mut rt = Runtime {
        tx,
        proj: Arc::new(Project::open(project_dir)?),
    };

    ts::init_repository("https://thunderstore.io", None);

    loop {
        if let Err(_) = stdin.read_line(&mut line) {
            panic!("");
        };

        println!("LINE: {line}");

        match Message::from_json(&line) {
            Ok(msg) => route(msg, &mut rt).await?,
            Err(e) => {
                rt.tx
                    .send(Message::Response(Response {
                        id: proto::Id::String("FUCK".into()),
                        data: ResponseData::Error(e.to_string()),
                    }))
                    .unwrap();
            }
        };

        // if let Ok(msg) = Message::from_json(&line) {
        // } else {
        // }

        // let msg = Message::from_json(&line);
        // route(msg, &rt).await?;
    }
}

/// Route
async fn route(msg: Message, rt: &mut Runtime) -> Result<(), Error> {
    match msg {
        Message::Request(rq) => route_rq(Request::try_from(rq)?, rt).await?,
        Message::Response(_) => panic!(),
    }

    Ok(())
}

// Request routing
async fn route_rq(rq: Request, rt: &mut Runtime) -> Result<(), Error> {
    match rq.method {
        method::Method::Exit => todo!(),
        method::Method::Project(proj) => proj.route(rt).await?,
        method::Method::Package(pack) => pack.route(rt).await?,
    }

    Ok(())
}

// /// The daemon's entrypoint. This is a psuedo event loop which does the following in step:
// /// 1. Read JSON-RPC input(s) from stdin.
// /// 2. Route each input.
// /// 3. Serialize the output and write to stdout.
// async fn start() {
//     let stdin = io::stdin();
//     let mut line = String::new();
//     let (send, recv) = mpsc::channel::<Result<Response, Error>>();

//     let cancel = RwLock::new(false);

//     // Responses are published through the tx send channel.
//     // thread::spawn(move || respond_msg(recv, cancel));

//     loop {
//         // Block the main thread until we have an input line available to be read.
//         // This is ok because, in theory, tasks will be processed on background threads.
//         if let Err(e) = stdin.read_line(&mut line) {
//             panic!("")
//         }
//         let res = route(&line, self.ctx, send.clone()).await;
//         res.to_json().unwrap();
//     }
// }

fn respond_msg(recv: Receiver<Message>, _cancel: RwLock<bool>) {
    let mut stdout = io::stdout();
    while let Ok(res) = recv.recv() {
        let msg = serde_json::to_string(&res);
        stdout.write_all(msg.unwrap().as_bytes()).unwrap();
        stdout.write_all("\n".as_bytes()).unwrap();
    }
}

// Route and execute the request, returning the result.
// Messages, including the result of subsequent computation, are sent over the sender channel.
// async fn route(line: &str, ctx: RwLock<Project>, send: Sender<Result<Response, Error>>) -> Result<Response, Error> {
//     let req = Message::from_json(line)?;
//     match req {
//         Message::Request(rq) => route_rq(Request::try_from(rq)?, ctx, send).await,
//         Message::Response(_) => panic!(),
//     }
// }

// /// Do the actual Request routing here.
// /// One more level of abstraction. This routes calls to their actual implementation within
// /// the method module.
// async fn route_rq(
//     request: Request,
//     ctx: RwLock<Project>,
//     send: Sender<Result<Response, Error>>,
// ) -> Result<Response, Error> {
//     match request.method {
//         method::Method::Exit => todo!(),
//         method::Method::Project(x) => x.route(ctx, send).await,
//         method::Method::Package(_) => todo!(),
//     }
//
