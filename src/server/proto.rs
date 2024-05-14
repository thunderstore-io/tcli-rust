use std::str::FromStr;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_with::{serde_as, DisplayFromStr};

use crate::server::method::Method;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("The provided message '{0}' could not be parsed as JSON.")]
    InvalidMessage(String),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum Message {
    Request(Request),
    Response(Response),
}

impl Message {
    pub fn from_json(json: &str) -> Result<Self, Error> {
        serde_json::from_str::<Message>(json).map_err(|e| Error::InvalidMessage(e.to_string()))
    }
}

/// This FromStr wrapper is here specifically for serde_with deserialization.
impl FromStr for Message {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Message::from_json(s)
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum Id {
    Int(isize),
    String(String),
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Request {
    /// An identifier which, as per the JSON-RPC spec, can either be an integer
    /// or string. We use an untagged enum to allow serde to transparenly parse these types.
    pub id: Id,

    /// This field is deserialized into a Method enum variant via Method::from_str.
    /// Unfortunately this means that errors returned from Method::from_str are lost.
    #[serde_as(as = "DisplayFromStr")]
    pub method: Method,

    /// This field is null for notifications.
    #[serde(default = "Value::default")]
    #[serde(skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct Response {
    pub id: Id,
    pub content: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ErrorMessage {
    pub id: Id,
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::server::method::{PackageMethod, ProjectMethod};

    #[test]
    fn test_request_deserialize() {
        let request = "{ \"id\": 1, \"method\": \"project/set_context\" }";
        let de = Message::from_str(request).unwrap();
        let cmp = Request {
            id: Id::Int(1),
            method: Method::Project(ProjectMethod::SetContext),
            params: Value::Null,
        };
        assert_eq!(de, Message::Request(cmp));

        let request = "{ \"id\": \"oksamies\", \"method\": \"package/get_metadata\" }";
        let de = Message::from_str(request).unwrap();
        let cmp = Request {
            id: Id::String(String::from("oksamies")),
            method: Method::Package(PackageMethod::GetMetadata),
            params: Value::Null,
        };
        assert_eq!(de, Message::Request(cmp));

        // At this point the error is pretty obfuscated behind serde_json::Error, so we just
        // check to see if an error was returned.
        let request = "{ \"id\": \"oksamies\", \"method\": \"null/null\" }";
        let de: Result<Request, serde_json::Error> = serde_json::from_str(request);
        assert!(de.is_err());
    }
}
