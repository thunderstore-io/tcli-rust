use std::io::Write;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::RwLock;
use std::{io, thread};

use self::proto::{Message, Request, Response};

mod lock;
mod method;
mod proto;
mod route;

trait ToJson {
    fn to_json(&self) -> Result<String, serde_json::Error>;
}

/// This error type exists to wrap library errors into a single easy-to-use package.
#[derive(thiserror::Error, Debug)]
#[repr(isize)]
pub enum Error {
    /// A partial implementation of the error variants described by the JRPC spec.
    #[error("Failed to serialize JSON: {0:?}")]
    InvalidJson(#[from] serde_json::Error) = -32700,

    #[error("The method {0} is not valid.")]
    InvalidMethod(String) = -32601,

    #[error("Recieved invalid params for method {0}: {1}")]
    InvalidParams(String, String) = -32602,

    /// Wrapper error types and codes.
    #[error("${0:?}")]
    ProjectError(String) = 1000,

    #[error("{0:?}")]
    PackageError(String) = 2000,
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

/// The daemon's entrypoint. This is a psuedo event loop which does the following in step:
/// 1. Read JSON-RPC input(s) from stdin.
/// 2. Route each input.
/// 3. Serialize the output and write to stdout.
async fn start() {
    let stdin = io::stdin();
    let mut line = String::new();
    let (tx, rx) = mpsc::channel::<Result<Response, Error>>();

    let cancel = RwLock::new(false);

    // Responses are published through the tx send channel.
    thread::spawn(move || respond_msg(rx, cancel));

    loop {
        // Block the main thread until we have an input line available to be read.
        // This is ok because, in theory, tasks will be processed on background threads.
        if let Err(e) = stdin.read_line(&mut line) {
            panic!("")
        }
        let res = route(&line, tx.clone()).await;
        res.to_json().unwrap();
    }
}

fn respond_msg(rx: Receiver<Result<Response, Error>>, cancel: RwLock<bool>) {
    let mut stdout = io::stdout();
    while let Ok(res) = rx.recv() {
        let msg = res.map(|x| serde_json::to_string(&x).unwrap());
        stdout.write_all(msg.unwrap().as_bytes());
        stdout.write_all("\n".as_bytes());
    }
}

/// Route and execute the request, returning the result.
async fn route(line: &str, tx: Sender<Result<Response, Error>>) -> Result<Response, Error> {
    let req = Message::from_json(line);
    todo!()
}
