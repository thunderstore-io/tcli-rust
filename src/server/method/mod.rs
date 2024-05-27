pub mod package;
pub mod project;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use self::package::PackageMethod;
use self::project::ProjectMethod;
use super::Error;

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
                .ok_or_else(|| Error::InvalidMethod(method.into()))?,
            split
                .next()
                .ok_or_else(|| Error::InvalidMethod(method.into()))?,
        );

        // Route namespaces to the appropriate enum variants for construction.
        Ok(match namespace {
            "exit" => Self::Exit,
            "project" => Self::Project(ProjectMethod::from_value(name, value)?),
            "package" => Self::Package(PackageMethod::from_value(name, value)?),
            x => Err(Error::InvalidMethod(x.into()))?,
        })
    }
}

pub fn parse_value<T: DeserializeOwned>(value: serde_json::Value) -> Result<T, serde_json::Error> {
    serde_json::from_value(value)
}
