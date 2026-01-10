use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::server::method::Method;
use crate::server::{Error, ServerError};

const JRPC_VER: &str = "2.0";

/// A JSON-RPC 2.0 message - either a request or response.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum Message {
    Request(RequestInner),
    Response(Response),
}

impl Message {
    pub fn from_json(json: &str) -> Result<Self, Error> {
        let msg = serde_json::from_str::<Message>(json)
            .inspect_err(|e| {
                println!("{e:?}");
            })
            .map_err(ServerError::InvalidJson)?;

        match msg {
            Message::Request(ref x) if x.jsonrpc != JRPC_VER => {
                Err(ServerError::InvalidRequest("jsonrpc must be \"2.0\"".into()))?
            }
            _ => Ok(msg),
        }
    }
}

/// JSON-RPC request/response identifier. Can be integer, string, or null.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(untagged)]
pub enum Id {
    Int(i64),
    String(String),
    Null,
}

/// Raw representation of a JSON-RPC request before method routing.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct RequestInner {
    /// Must be "2.0".
    pub jsonrpc: String,

    /// Request identifier, echoed back in the response.
    pub id: Id,

    /// Method name in "namespace/method" format.
    pub method: String,

    /// Method parameters (optional).
    #[serde(default = "Value::default")]
    #[serde(skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

/// A validated, routable JSON-RPC request.
#[derive(Debug, PartialEq, Eq)]
pub struct Request {
    pub id: Id,
    pub method: Method,
}

impl TryFrom<RequestInner> for Request {
    type Error = Error;

    fn try_from(value: RequestInner) -> Result<Self, Self::Error> {
        let method = Method::from_value(&value.method, value.params)?;
        Ok(Self {
            id: value.id,
            method,
        })
    }
}

/// A JSON-RPC 2.0 response.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Response {
    /// Always "2.0".
    pub jsonrpc: String,

    /// The request ID this response corresponds to.
    pub id: Id,

    /// Success result (mutually exclusive with error).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,

    /// Error object (mutually exclusive with result).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    /// Create a success response with a serializable result.
    pub fn ok<T: Serialize>(id: Id, data: T) -> Response {
        Response {
            jsonrpc: JRPC_VER.into(),
            id,
            result: Some(serde_json::to_value(data).unwrap_or(Value::Null)),
            error: None,
        }
    }

    /// Create a success response with no result data.
    pub fn ok_empty(id: Id) -> Response {
        Response {
            jsonrpc: JRPC_VER.into(),
            id,
            result: Some(Value::Null),
            error: None,
        }
    }

    /// Create an error response.
    pub fn err(id: Id, error: RpcError) -> Response {
        Response {
            jsonrpc: JRPC_VER.into(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

/// JSON-RPC 2.0 error object.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RpcError {
    /// Numeric error code.
    pub code: i32,

    /// Short description of the error.
    pub message: String,

    /// Additional error data. Contains `kind` for i18n error identification.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<RpcErrorData>,
}

/// Additional error data for i18n and debugging.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct RpcErrorData {
    /// Stable string identifier for the error type (e.g., "project.not_found").
    /// Used by clients for i18n lookup.
    pub kind: String,

    /// Additional context, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<Value>,
}

impl RpcError {
    /// Standard JSON-RPC error: Parse error (-32700)
    pub fn parse_error(msg: impl Into<String>) -> Self {
        Self {
            code: -32700,
            message: msg.into(),
            data: Some(RpcErrorData {
                kind: "rpc.parse_error".into(),
                context: None,
            }),
        }
    }

    /// Standard JSON-RPC error: Invalid request (-32600)
    pub fn invalid_request(msg: impl Into<String>) -> Self {
        Self {
            code: -32600,
            message: msg.into(),
            data: Some(RpcErrorData {
                kind: "rpc.invalid_request".into(),
                context: None,
            }),
        }
    }

    /// Standard JSON-RPC error: Method not found (-32601)
    pub fn method_not_found(method: impl Into<String>) -> Self {
        let method = method.into();
        Self {
            code: -32601,
            message: format!("Method not found: {method}"),
            data: Some(RpcErrorData {
                kind: "rpc.method_not_found".into(),
                context: Some(serde_json::json!({ "method": method })),
            }),
        }
    }

    /// Standard JSON-RPC error: Invalid params (-32602)
    pub fn invalid_params(msg: impl Into<String>) -> Self {
        Self {
            code: -32602,
            message: msg.into(),
            data: Some(RpcErrorData {
                kind: "rpc.invalid_params".into(),
                context: None,
            }),
        }
    }

    /// Standard JSON-RPC error: Internal error (-32603)
    pub fn internal_error(msg: impl Into<String>) -> Self {
        Self {
            code: -32603,
            message: msg.into(),
            data: Some(RpcErrorData {
                kind: "rpc.internal_error".into(),
                context: None,
            }),
        }
    }

    /// Application-level error (code >= -32000)
    /// TODO: This will be replaced by macro-generated error conversion
    pub fn app_error(kind: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            code: -32000,
            message: msg.into(),
            data: Some(RpcErrorData {
                kind: kind.into(),
                context: None,
            }),
        }
    }
}

/// Convert server errors to RPC errors.
/// TODO: Replace with macro-based system for inline error metadata.
impl From<&ServerError> for RpcError {
    fn from(err: &ServerError) -> Self {
        match err {
            ServerError::InvalidJson(e) => RpcError::parse_error(e.to_string()),
            ServerError::InvalidRequest(msg) => RpcError::invalid_request(msg),
            ServerError::InvalidMethod(method) => RpcError::method_not_found(method),
            ServerError::InvalidParams(method, msg) => {
                RpcError::invalid_params(format!("{method}: {msg}"))
            }
            ServerError::InvalidContext => {
                RpcError::app_error("server.invalid_context", "No project context available")
            }
            ServerError::ProjectLocked => {
                RpcError::app_error("server.project_locked", "Project is locked by another process")
            }
            ServerError::WebSocket(e) => {
                RpcError::app_error("server.websocket_error", e)
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::server::method::project::{OpenProject, ProjectMethod};

    #[test]
    fn test_jrpc_ver_validate() {
        // Invalid version should fail
        let data = r#"{ "jsonrpc": "1.0", "id": 1, "method": "project/open", "params": {} }"#;
        let result = Message::from_json(data);
        assert!(result.is_err());
    }

    #[test]
    fn test_request_deserialize() {
        // Valid request with params: project/open
        let data = r#"{ "jsonrpc": "2.0", "id": 1, "method": "project/open", "params": { "path": "/some/path" } }"#;
        let rq: RequestInner = serde_json::from_str(data).unwrap();
        assert_eq!(rq.id, Id::Int(1));
        assert_eq!(rq.method, "project/open");
        assert!(matches!(rq.params, Value::Object(..)));

        let rq = Request::try_from(rq).unwrap();
        assert_eq!(rq.id, Id::Int(1));
        assert!(matches!(
            rq.method,
            Method::Project(ProjectMethod::Open(OpenProject { .. }))
        ));

        // Valid request without params: project/get_metadata
        let data = r#"{ "jsonrpc": "2.0", "id": "oksamies", "method": "project/get_metadata" }"#;
        let rq: RequestInner = serde_json::from_str(data).unwrap();
        assert_eq!(rq.id, Id::String("oksamies".into()));
        assert_eq!(rq.method, "project/get_metadata");
        assert_eq!(rq.params, Value::Null);

        let rq = Request::try_from(rq).unwrap();
        assert_eq!(rq.id, Id::String("oksamies".into()));
        assert!(matches!(
            rq.method,
            Method::Project(ProjectMethod::GetMetadata)
        ));

        // Invalid methods should still be deserialized ok as they're checked by typed Request struct.
        let data = r#"{ "jsonrpc": "2.0", "id": "oksamies", "method": "null/null" }"#;
        let rq: RequestInner = serde_json::from_str(data).unwrap();
        assert_eq!(rq.id, Id::String("oksamies".into()));
        assert_eq!(rq.method, "null/null");
        assert_eq!(rq.params, Value::Null);

        // ...but should then fail to be converted into a typed Request.
        let rq = Request::try_from(rq);
        assert!(matches!(
            rq,
            Err(Error::Server(ServerError::InvalidMethod(..)))
        ));

        // Likewise, valid methods with garbage data should also fail when converted to typed.
        let data = r#"{ "jsonrpc": "2.0", "id": "oksamies", "method": "project/open", "params": { "garbage": 1 } }"#;
        let rq: RequestInner = serde_json::from_str(data).unwrap();
        assert_eq!(rq.id, Id::String("oksamies".into()));
        assert_eq!(rq.method, "project/open");
        assert!(matches!(rq.params, Value::Object(..)));

        let rq = Request::try_from(rq);
        assert!(matches!(rq, Err(Error::Parse(..))));
    }

    #[test]
    fn test_response_serialize() {
        // Success response
        let resp = Response::ok(Id::Int(1), "hello");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""jsonrpc":"2.0""#));
        assert!(json.contains(r#""id":1"#));
        assert!(json.contains(r#""result":"hello""#));
        assert!(!json.contains("error"));

        // Error response
        let resp = Response::err(Id::String("req-1".into()), RpcError::method_not_found("foo/bar"));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains(r#""jsonrpc":"2.0""#));
        assert!(json.contains(r#""id":"req-1""#));
        assert!(json.contains(r#""code":-32601"#));
        assert!(json.contains(r#""kind":"rpc.method_not_found""#));
        assert!(!json.contains("result"));
    }
}
