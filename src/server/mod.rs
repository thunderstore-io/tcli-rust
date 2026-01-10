use std::io::{self, BufRead, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use futures_util::{SinkExt, StreamExt};
use lock::ProjectLock;
use once_cell::sync::Lazy;
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::WebSocketStream;

use self::proto::{Id, Message, Request, Response, RpcError};
use crate::error::Error;
use crate::project::Project;
use crate::ts;

mod lock;
pub mod method;
pub mod proto;

/// Transport trait for receiving and sending JSON-RPC messages.
/// Implementations can be stdin/stdout, WebSocket, TCP, etc.
pub trait Transport {
    /// Receive the next message. Returns None on EOF/disconnect.
    fn recv(&mut self) -> impl std::future::Future<Output = Option<String>> + Send;
    
    /// Send a message.
    fn send(&mut self, msg: &str) -> impl std::future::Future<Output = Result<(), Error>> + Send;
}

/// Stdin/stdout transport for CLI usage.
pub struct StdioTransport {
    stdin: io::Stdin,
    stdout: io::Stdout,
    buf: String,
}

impl StdioTransport {
    pub fn new() -> Self {
        Self {
            stdin: io::stdin(),
            stdout: io::stdout(),
            buf: String::new(),
        }
    }
}

impl Transport for StdioTransport {
    async fn recv(&mut self) -> Option<String> {
        self.buf.clear();
        match self.stdin.lock().read_line(&mut self.buf) {
            Ok(0) => None, // EOF
            Ok(_) => Some(self.buf.trim().to_string()),
            Err(_) => None,
        }
    }

    async fn send(&mut self, msg: &str) -> Result<(), Error> {
        let mut out = self.stdout.lock();
        writeln!(out, "{}", msg).map_err(|e| Error::Server(ServerError::InvalidRequest(e.to_string())))?;
        out.flush().map_err(|e| Error::Server(ServerError::InvalidRequest(e.to_string())))?;
        Ok(())
    }
}

/// WebSocket transport for GUI/remote usage.
pub struct WebSocketTransport {
    ws: WebSocketStream<TcpStream>,
}

impl WebSocketTransport {
    pub fn new(ws: WebSocketStream<TcpStream>) -> Self {
        Self { ws }
    }
}

impl Transport for WebSocketTransport {
    async fn recv(&mut self) -> Option<String> {
        loop {
            match self.ws.next().await {
                Some(Ok(WsMessage::Text(text))) => return Some(text.to_string()),
                Some(Ok(WsMessage::Close(_))) => return None,
                Some(Ok(WsMessage::Ping(data))) => {
                    // Respond to ping with pong
                    let _ = self.ws.send(WsMessage::Pong(data)).await;
                    continue;
                }
                Some(Ok(_)) => continue, // Ignore binary, pong, etc.
                Some(Err(_)) => return None,
                None => return None,
            }
        }
    }

    async fn send(&mut self, msg: &str) -> Result<(), Error> {
        self.ws
            .send(WsMessage::Text(msg.into()))
            .await
            .map_err(|e| Error::Server(ServerError::InvalidRequest(e.to_string())))
    }
}

/// This is our project dir singleton. It will likely be refactored, but also likely not.
static PROJECT_DIR: Lazy<RwLock<PathBuf>> = Lazy::new(Default::default);

/// Server-specific errors. These map to JSON-RPC error codes.
/// TODO: Replace with macro-based system for inline error metadata.
#[derive(thiserror::Error, Debug)]
pub enum ServerError {
    #[error("Failed to parse JSON: {0}")]
    InvalidJson(#[from] serde_json::Error),

    #[error("Invalid request: {0}")]
    InvalidRequest(String),

    #[error("Method not found: {0}")]
    InvalidMethod(String),

    #[error("Invalid params for {0}: {1}")]
    InvalidParams(String, String),

    #[error("No project context available")]
    InvalidContext,

    #[error("Project is locked by another process")]
    ProjectLocked,

    #[error("WebSocket error: {0}")]
    WebSocket(String),
}

/// Runtime context for the server.
pub struct Runtime {
    pub proj: Arc<RwLock<Project>>,
    _lock: ProjectLock,
}

impl Runtime {
    /// Create a new runtime, acquiring an exclusive lock on the project.
    pub fn new(project_dir: &Path) -> Result<Self, Error> {
        let lock = ProjectLock::lock(project_dir)
            .ok_or(ServerError::ProjectLocked)?;
        
        let project = Project::open(project_dir)?;
        
        Ok(Self {
            proj: Arc::new(RwLock::new(project)),
            _lock: lock,
        })
    }

    /// Send a response through the provided transport.
    pub async fn send_response<T: Transport>(&self, transport: &mut T, response: Response) {
        let json = serde_json::to_string(&response).unwrap_or_else(|e| {
            serde_json::to_string(&Response::err(
                Id::Null,
                RpcError::internal_error(format!("Serialization failed: {e}")),
            ))
            .unwrap()
        });
        let _ = transport.send(&json).await;
    }

    /// Execute a read-only operation on the project.
    pub fn with_project<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(&Project) -> Result<R, Error>,
    {
        let proj = self.proj.read().map_err(|_| ServerError::InvalidContext)?;
        f(&proj)
    }

    /// Execute a mutable operation on the project.
    pub fn with_project_mut<F, R>(&self, f: F) -> Result<R, Error>
    where
        F: FnOnce(&mut Project) -> Result<R, Error>,
    {
        let mut proj = self.proj.write().map_err(|_| ServerError::InvalidContext)?;
        f(&mut proj)
    }
}

/// Create and run the server with the given transport.
pub async fn run<T: Transport>(mut transport: T, project_dir: &Path) -> Result<(), Error> {
    let rt = Runtime::new(project_dir)?;
    
    ts::init_repository("https://thunderstore.io", None);

    while let Some(line) = transport.recv().await {
        if line.is_empty() {
            continue;
        }

        // Parse the message
        let msg = match Message::from_json(&line) {
            Ok(msg) => msg,
            Err(e) => {
                let rpc_err = match &e {
                    Error::Server(se) => RpcError::from(se),
                    _ => RpcError::parse_error(e.to_string()),
                };
                rt.send_response(&mut transport, Response::err(Id::Null, rpc_err)).await;
                continue;
            }
        };

        // Route the message
        if let Err(e) = route(msg, &rt, &mut transport).await {
            let rpc_err = match &e {
                Error::Server(se) => RpcError::from(se),
                _ => RpcError::internal_error(e.to_string()),
            };
            rt.send_response(&mut transport, Response::err(Id::Null, rpc_err)).await;
        }
    }

    Ok(())
}

/// Convenience function to spawn with stdio transport.
pub async fn spawn_stdio(project_dir: &Path) -> Result<(), Error> {
    run(StdioTransport::new(), project_dir).await
}

/// Start a WebSocket server on the given address.
/// Each connection is handled sequentially to avoid Send requirements
/// from the non-Send Reporter/Progress traits used in the project code.
pub async fn spawn_websocket(addr: SocketAddr, project_dir: &Path) -> Result<(), Error> {
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| ServerError::WebSocket(e.to_string()))?;

    println!("WebSocket server listening on ws://{}", addr);

    let rt = Runtime::new(project_dir)?;
    ts::init_repository("https://thunderstore.io", None);

    loop {
        let (stream, peer) = listener
            .accept()
            .await
            .map_err(|e| ServerError::WebSocket(e.to_string()))?;

        match tokio_tungstenite::accept_async(stream).await {
            Ok(ws) => {
                println!("New WebSocket connection from {}", peer);
                let mut transport = WebSocketTransport::new(ws);
                if let Err(e) = run_with_runtime(&rt, &mut transport).await {
                    eprintln!("Connection error from {}: {}", peer, e);
                }
                println!("Connection closed: {}", peer);
            }
            Err(e) => {
                eprintln!("WebSocket handshake failed from {}: {}", peer, e);
            }
        }
    }
}

/// Run the server loop with an existing runtime (for shared WebSocket connections).
async fn run_with_runtime<T: Transport>(rt: &Runtime, transport: &mut T) -> Result<(), Error> {
    while let Some(line) = transport.recv().await {
        if line.is_empty() {
            continue;
        }

        let msg = match Message::from_json(&line) {
            Ok(msg) => msg,
            Err(e) => {
                let rpc_err = match &e {
                    Error::Server(se) => RpcError::from(se),
                    _ => RpcError::parse_error(e.to_string()),
                };
                rt.send_response(transport, Response::err(Id::Null, rpc_err)).await;
                continue;
            }
        };

        if let Err(e) = route(msg, rt, transport).await {
            let rpc_err = match &e {
                Error::Server(se) => RpcError::from(se),
                _ => RpcError::internal_error(e.to_string()),
            };
            rt.send_response(transport, Response::err(Id::Null, rpc_err)).await;
        }
    }

    Ok(())
}

/// Route a message to its handler.
async fn route<T: Transport>(msg: Message, rt: &Runtime, transport: &mut T) -> Result<(), Error> {
    match msg {
        Message::Request(rq) => {
            let id = rq.id.clone();
            match Request::try_from(rq) {
                Ok(request) => route_rq(request, rt, transport).await,
                Err(e) => {
                    let rpc_err = match &e {
                        Error::Server(se) => RpcError::from(se),
                        Error::Parse(pe) => RpcError::invalid_params(pe.to_string()),
                        _ => RpcError::internal_error(e.to_string()),
                    };
                    rt.send_response(transport, Response::err(id, rpc_err)).await;
                    Ok(())
                }
            }
        }
        Message::Response(_) => Ok(()),
    }
}

/// Route a validated request to its method handler.
async fn route_rq<T: Transport>(rq: Request, rt: &Runtime, transport: &mut T) -> Result<(), Error> {
    let id = rq.id.clone();

    let result = match rq.method {
        method::Method::Exit => {
            rt.send_response(transport, Response::ok(id, "exiting")).await;
            return Ok(());
        }
        method::Method::Project(proj) => proj.route(id.clone(), rt, transport).await,
        method::Method::Package(pack) => pack.route(id.clone(), rt, transport).await,
    };

    if let Err(e) = result {
        let rpc_err = match &e {
            Error::Server(se) => RpcError::from(se),
            _ => RpcError::internal_error(e.to_string()),
        };
        rt.send_response(transport, Response::err(id, rpc_err)).await;
    }

    Ok(())
}
