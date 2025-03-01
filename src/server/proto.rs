use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::server::method::Method;
use crate::server::Error;

use super::ServerError;

const JRPC_VER: &str = "2.0";

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum Message {
    Request(RequestInner),
    Response(Response),
}

impl Message {
    pub fn from_json(json: &str) -> Result<Self, Error> {
        let msg = serde_json::from_str::<Message>(json).inspect_err(|e| {
            println!("{e:?}");
        }).map_err(ServerError::InvalidJson)?;

        match msg {
            Message::Request(x) if x.jsonrpc != JRPC_VER => Err(ServerError::InvalidMethod(x.jsonrpc))?,
            _ => Ok(msg),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum Id {
    Int(isize),
    String(String),
}

/// This is the raw representation of a JSON-RPC request *before* we convert it
/// into a structured type.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct RequestInner {
    /// The JSON-RPC protocol version. Must be 2.0 as per the spec.
    pub jsonrpc: String,

    /// An identifier which, as per the JSON-RPC spec, can either be an integer
    /// or string. We use an untagged enum to allow serde to transparenly parse these types.
    pub id: Id,

    /// This field is deserialized into a Method enum variant via Method::from_str.
    /// Unfortunately this means that errors returned from Method::from_str are lost.
    // #[serde_as(as = "DisplayFromStr")]
    pub method: String,

    /// This field is null for notifications.
    #[serde(default = "Value::default")]
    #[serde(skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

/// A structured JSON-RPC request.
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Request {
    /// The JSON-RPC identifier.
    pub id: Id,
    /// The method with data, if any.
    pub method: Method,
}

impl TryFrom<RequestInner> for Request {
    type Error = super::Error;

    fn try_from(value: RequestInner) -> Result<Self, Self::Error> {
        // We deserialize the params value depending on the provided method.
        // This is done by passing it to the from_value function of the Method type,
        // which iterates down through nested enums until we have a concrete type for the Value
        // and a valid method variant.
        let method = Method::from_value(&value.method, value.params)?;
        Ok(Self {
            id: value.id,
            method,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Response {
    pub id: Id,

    #[serde(flatten)]
    pub data: ResponseData,
}

impl Response {
    pub fn data_ok(id: Id, data: impl Serialize) -> Response {
        Response {
            id,
            data: ResponseData::Result(serde_json::to_string(&data).unwrap()),
        }
    }

    pub fn ok(id: Id) -> Response {
        Response {
            id,
            data: ResponseData::Result("OK".into()),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum ResponseData {
    #[serde(rename = "result")]
    Result(String),
    #[serde(rename = "error")]
    Error(String),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RpcError {
    pub code: isize,
    pub message: String,
}

impl From<Error> for RpcError {
    fn from(value: Error) -> Self {
        Self {
            code: value.discriminant(),
            message: value.to_string(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::server::method::package::PackageMethod;
    use crate::server::method::project::{ProjectMethod, OpenProject};
    use crate::server::ServerError;

    #[test]
    fn test_jrpc_ver_validate() {
        let data = r#"{ "jsonrpc": "2.0", "id": 1, "method": "oksamies""#;
    }

    #[test]
    fn test_request_deserialize() {
        let data = r#"{ "jsonrpc": "2.0", "id": 1, "method": "project/set_context", "params": { "path": "/some/path" } }"#;
        let rq: RequestInner = serde_json::from_str(&data).unwrap();
        assert_eq!(rq.id, Id::Int(1));
        assert_eq!(rq.method, "project/set_context");
        assert!(matches!(rq.params, Value::Object(..)));

        let rq = Request::try_from(rq).unwrap();
        assert_eq!(rq.id, Id::Int(1));
        assert!(matches!(
            rq.method,
            Method::Project(ProjectMethod::Open(OpenProject { .. }))
        ));

        let data = r#"{ "jsonrpc": "2.0", "id": "oksamies", "method": "package/get_metadata" }"#;
        let rq: RequestInner = serde_json::from_str(&data).unwrap();
        assert_eq!(rq.id, Id::String("oksamies".into()));
        assert_eq!(rq.method, "package/get_metadata");
        assert_eq!(rq.params, Value::Null);

        // let rq = Request::try_from(rq).unwrap();
        // assert_eq!(rq.id, Id::String("oksamies".into()));
        // assert!(matches!(
        //     rq.method,
        //     Method::Package(PackageMethod::GetMetadata)
        // ));

        // Invalid methods should still be deserialized aok as they're checked by typed Request struct.
        let data = r#"{ "jsonrpc": "2.0", "id": "oksamies", "method": "null/null" }"#;
        let rq: RequestInner = serde_json::from_str(&data).unwrap();
        assert_eq!(rq.id, Id::String("oksamies".into()));
        assert_eq!(rq.method, "null/null");
        assert_eq!(rq.params, Value::Null);

        // ...but should then fail to be converted into a typed Request.
        let rq = Request::try_from(rq);
        assert!(matches!(rq, Err(Error::Server(ServerError::InvalidMethod(..))))); // Invalid methods should still be deserialized aok as they're checked by typed Request struct.

        // Likewise, valid methods with garbage data should also fail when converted to typed.
        let data = r#"{ "jsonrpc": "2.0", "id": "oksamies", "method": "project/set_context", "params": { "garbage": 1 } }"#;
        let rq: RequestInner = serde_json::from_str(&data).unwrap();
        assert_eq!(rq.id, Id::String("oksamies".into()));
        assert_eq!(rq.method, "project/set_context");
        assert!(matches!(rq.params, Value::Object(..)));

        let rq = Request::try_from(rq);
        panic!("{rq:?}");
        assert!(matches!(rq, Err(Error::Server(ServerError::InvalidJson(..)))));
    }
}
