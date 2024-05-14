use std::fmt::Display;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum Error {
    #[error("The method '{0}' is malformed or otherwise invalid.")]
    BadMethod(String),
    #[error("The method '{0}' is invalid.")]
    InvalidMethodName(String),
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum Method {
    Project(ProjectMethod),
    Package(PackageMethod),
}

impl Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        panic!("Invalid use, Method cannot be serialized (despite it being possible to do so).");
    }
}

/// Method namespace registration.
impl FromStr for Method {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let mut split = value.split('/');
        let (namespace, name) = (
            split.next().ok_or_else(|| Error::BadMethod(value.into()))?,
            split.next().ok_or_else(|| Error::BadMethod(value.into()))?,
        );

        // Route namespaces to the appropriate enum variants for construction.
        Ok(match namespace {
            "project" => Self::Project(ProjectMethod::from_str(&name)?),
            "package" => Self::Package(PackageMethod::from_str(&name)?),
            x => Err(Error::InvalidMethodName(x.into()))?,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum ProjectMethod {
    /// Set the project directory context. This locks the project, if it exists.
    SetContext,
    /// Release the project directory context. This unlocks the project, if a lock exists.
    ReleaseContext,
    /// Get project metadata.
    GetMetadata,
    /// Add one or more packages to the project.
    AddPackages,
    /// Remove one or more packages from the project.
    RemovePackages,
    /// Get a list of currently installed packages.
    GetPackages,
    /// Determine if the current context is a valid project.
    IsValid,
}

impl FromStr for ProjectMethod {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "set_context" => Self::SetContext,
            "release_context" => Self::ReleaseContext,
            "get_metadata" => Self::GetMetadata,
            "add_packages" => Self::AddPackages,
            "remove_packages" => Self::RemovePackages,
            "get_packages" => Self::GetPackages,
            "is_valid" => Self::IsValid,
            x => Err(Error::InvalidMethodName(x.into()))?,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum PackageMethod {
    /// Get metadata about this package.
    GetMetadata,
    /// Determine if the package exists within the cache.
    IsCached,
}

impl FromStr for PackageMethod {
    type Err = Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Ok(match value {
            "get_metadata" => Self::GetMetadata,
            "is_cached" => Self::IsCached,
            x => Err(Error::InvalidMethodName(x.into()))?,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_namespace_resolve() {
        let method = "project/set_context";
        let resolved = Method::from_str(method).unwrap();
        assert_eq!(resolved, Method::Project(ProjectMethod::SetContext));

        let method = "project/release_context";
        let resolved = Method::from_str(method).unwrap();
        assert_eq!(resolved, Method::Project(ProjectMethod::ReleaseContext));

        // Assert that methods with invalid structure are caught.
        let method = "null";
        let resolved = Method::from_str(method);
        assert!(resolved.is_err());
        assert!(matches!(resolved.err().unwrap(), Error::BadMethod(..)));

        // Assert that invalid methods with correct structure are caught.
        let method = "null/null";
        let resolved = Method::from_str(method);
        assert!(resolved.is_err());
        assert!(matches!(
            resolved.err().unwrap(),
            Error::InvalidMethodName(..),
        ));

        // Assert that name resolution can handle bad names.
        let name = "null";
        let resolved = ProjectMethod::from_str(name);
        assert!(resolved.is_err());
        assert!(matches!(
            resolved.err().unwrap(),
            Error::InvalidMethodName(..),
        ));

        let name = "null";
        let resolved = PackageMethod::from_str(name);
        assert!(resolved.is_err());
        assert!(matches!(
            resolved.err().unwrap(),
            Error::InvalidMethodName(..),
        ));
    }
}
