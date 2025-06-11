use serde::{Deserialize, Serialize};

use super::Error;
use crate::package::cache;
use crate::package::index::PackageIndex;
use crate::server::proto::{Id, Response};
use crate::server::{Runtime, ServerError};
use crate::ts::package_reference::PackageReference;
use crate::TCLI_HOME;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum PackageMethod {
    /// Get metadata about this package.
    GetMetadata(GetMetadata),
    /// Determine if the package exists within the cache.
    IsCached(IsCached),
    /// Syncronize the package index.
    SyncIndex,
}

impl PackageMethod {
    pub fn from_value(method: &str, value: serde_json::Value) -> Result<Self, Error> {
        Ok(match method {
            "get_metadata" => Self::GetMetadata(super::parse_value(value)?),
            "is_cached" => Self::IsCached(super::parse_value(value)?),
            "sync_index" => Self::SyncIndex,
            x => Err(ServerError::InvalidMethod(x.into()))?,
        })
    }

    pub async fn route(&self, rt: &mut Runtime) -> Result<(), Error> {
        match self {
            Self::GetMetadata(data) => {
                let index = PackageIndex::open(&TCLI_HOME).await?;
                let package = index.lock().unwrap().get_package(&data.package).unwrap();
                rt.send(Response::data_ok(Id::String("diowadaw".into()), package));
            }
            Self::IsCached(data) => {
                let is_cached = cache::is_cached(&data.package);
                rt.send(Response::data_ok(Id::String("dwdawdwa".into()), is_cached));
            }
            Self::SyncIndex => {
                PackageIndex::sync(&TCLI_HOME).await?;
                rt.send(Response::ok(Id::String("dwada".into())));
            }
        }

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct IsCached {
    package: PackageReference,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct GetMetadata {
    package: PackageReference,
}
