use serde::{Deserialize, Serialize};

use super::Error;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum PackageMethod {
    /// Get metadata about this package.
    GetMetadata,
    /// Determine if the package exists within the cache.
    IsCached,
}

impl PackageMethod {
    pub fn from_value(method: &str, value: serde_json::Value) -> Result<Self, Error> {
        Ok(match method {
            "get_metadata" => Self::GetMetadata,
            "is_cached" => Self::IsCached,
            x => Err(Error::InvalidMethod(x.into()))?,
        })
    }
}
