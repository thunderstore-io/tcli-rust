use std::path::PathBuf;
use std::sync::Arc;

use crate::project::ProjectKind;
use crate::server::proto::{Id, Response, ResponseData};
use crate::{project::Project, ui::reporter::VoidReporter};
use crate::ts::package_reference::PackageReference;
use crate::server::{Runtime, ServerError};
use serde::{Deserialize, Serialize};

use super::Error;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum ProjectMethod {
    /// Opens a project, locking it in the process.
    Open(OpenProject),
    /// Get project metadata.
    GetMetadata,
    /// Add one or more packages to the project.
    AddPackages(AddPackages),
    /// Remove one or more packages from the project.
    RemovePackages(RemovePackages),
    /// Get a list of currently installed packages.
    InstalledPackages,
}

impl From<Option<Project>> for ServerError {
    fn from(_val: Option<Project>) -> Self {
        ServerError::InvalidContext
    }
}

impl ProjectMethod {
    pub fn from_value(method: &str, value: serde_json::Value) -> Result<Self, Error> {
        Ok(match method {
            "open" => Self::Open(super::parse_value(value)?),
            "get_metadata" => Self::GetMetadata,
            "add_packages" => Self::AddPackages(super::parse_value(value)?),
            "remove_packages" => Self::RemovePackages(super::parse_value(value)?),
            "installed_packages" => Self::InstalledPackages,
            x => Err(ServerError::InvalidMethod(x.into()))?,
        })
    }

    /// Route and execute various project methods.
    /// Each of these call and interact directly with global project state.
    pub async fn route(&self, rt: &mut Runtime) -> Result<(), Error> {
        match self {
            ProjectMethod::Open(OpenProject { path }) => {
                // Unlock the previous ctx (if it exists) and relock this one.
                rt.proj = Arc::new(Project::open(path)
                    .unwrap_or(Project::create_new(path, true, ProjectKind::Profile)?))
            },
            ProjectMethod::GetMetadata => {
                rt.send(Response {
                    id: Id::String("OK".into()),
                    data: ResponseData::Result(format!("{:?}", rt.proj.statefile_path))
                });
            },
            ProjectMethod::AddPackages(packages) => {
                rt.proj.add_packages(&packages.packages[..])?;
                rt.proj.commit(Box::new(VoidReporter), false).await?;
            },
            ProjectMethod::RemovePackages(packages) => {
                rt.proj.remove_packages(&packages.packages[..])?;
                rt.proj.commit(Box::new(VoidReporter), false).await?;
            },
            ProjectMethod::InstalledPackages => {
                let lock = rt.proj.get_lockfile()?;
                let installed = lock.installed_packages().await?;

                rt.send(Response {
                    id: Id::Int(installed.len() as _),
                    data: ResponseData::Result(serde_json::to_string(&installed)?),
                });
            },
        }

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct OpenProject {
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
