use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ts::package_reference::PackageReference;

use super::Error;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum ProjectMethod {
    /// Set the project directory context. This locks the project, if it exists.
    SetContext(SetContext),
    /// Release the project directory context. This unlocks the project, if a lock exists.
    ReleaseContext,
    /// Get project metadata.
    GetMetadata,
    /// Add one or more packages to the project.
    AddPackages(AddPackages),
    /// Remove one or more packages from the project.
    RemovePackages(RemovePackages),
    /// Get a list of currently installed packages.
    GetPackages,
    /// Determine if the current context is a valid project.
    IsValid,
}

impl ProjectMethod {
    pub fn from_value(method: &str, value: serde_json::Value) -> Result<Self, Error> {
        Ok(match method {
            "set_context" => Self::SetContext(super::parse_value(value)?),
            "release_context" => Self::ReleaseContext,
            "get_metadata" => Self::GetMetadata,
            "add_packages" => Self::AddPackages(super::parse_value(value)?),
            "remove_packages" => Self::RemovePackages(super::parse_value(value)?),
            "get_packages" => Self::GetPackages,
            "is_valid" => Self::IsValid,
            x => Err(Error::InvalidMethod(x.into()))?,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct SetContext {
    path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct AddPackages {
    packages: Vec<PackageReference>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct RemovePackages {
    packages: Vec<PackageReference>,
}
