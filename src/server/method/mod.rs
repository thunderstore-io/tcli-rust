pub mod package;
pub mod project;

use std::sync::RwLock;

use futures::channel::mpsc::Sender;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use self::package::PackageMethod;
use self::project::ProjectMethod;
use super::proto::Response;
use super::{Error, ServerError};
use crate::project::Project;

pub trait Routeable {
    async fn route(&self, ctx: RwLock<Project>, send: Sender<Result<Response, Error>>);
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Method {
    Exit,
    Project(ProjectMethod),
    Package(PackageMethod),
}

/// Method namespace registration.
impl Method {
    pub fn from_value(method: &str, value: serde_json::Value) -> Result<Self, Error> {
        let mut split = method.split('/');
        let (namespace, name) = (
            split
                .next()
                .ok_or_else(|| ServerError::InvalidMethod(method.into()))?,
            split
                .next()
                .ok_or_else(|| ServerError::InvalidMethod(method.into()))?,
        );

        // Route namespaces to the appropriate enum variants for construction.
        Ok(match namespace {
            "exit" => Self::Exit,
            "project" => Self::Project(ProjectMethod::from_value(name, value)?),
            "package" => Self::Package(PackageMethod::from_value(name, value)?),
            x => Err(ServerError::InvalidMethod(x.into()))?,
        })
    }
}

pub fn parse_value<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, serde_json::Error> {
    serde_json::from_value(value)
}
