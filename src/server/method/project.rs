use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::Error;
use crate::project::ProjectKind;
use crate::server::proto::{Id, Response};
use crate::server::{Runtime, ServerError, Transport};
use crate::ts::package_reference::PackageReference;

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
    pub async fn route<T: Transport>(
        &self,
        id: Id,
        rt: &Runtime,
        transport: &mut T,
    ) -> Result<(), Error> {
        match self {
            ProjectMethod::Open(OpenProject { path }) => {
                // Replace the project in the runtime
                let new_project = crate::project::Project::open(path).unwrap_or(
                    crate::project::Project::create_new(path, true, ProjectKind::Profile)?,
                );
                
                let mut proj = rt.proj.write().map_err(|_| ServerError::InvalidContext)?;
                *proj = new_project;
                drop(proj);
                
                rt.send_response(transport, Response::ok(id, serde_json::json!({ "path": path }))).await;
            }
            ProjectMethod::GetMetadata => {
                let proj = rt.proj.read().map_err(|_| ServerError::InvalidContext)?;
                rt.send_response(transport, Response::ok(id, serde_json::json!({
                    "statefile_path": proj.statefile_path,
                    "manifest_path": proj.manifest_path,
                    "lockfile_path": proj.lockfile_path,
                }))).await;
            }
            ProjectMethod::AddPackages(packages) => {
                {
                    let proj = rt.proj.read().map_err(|_| ServerError::InvalidContext)?;
                    proj.add_packages(&packages.packages[..])?;
                    proj.commit(false).await?;
                }
                rt.send_response(
                    transport,
                    Response::ok(id, serde_json::json!({ "added": packages.packages.len() })),
                ).await;
            }
            ProjectMethod::RemovePackages(packages) => {
                {
                    let proj = rt.proj.read().map_err(|_| ServerError::InvalidContext)?;
                    proj.remove_packages(&packages.packages[..])?;
                    proj.commit(false).await?;
                }
                rt.send_response(
                    transport,
                    Response::ok(id, serde_json::json!({ "removed": packages.packages.len() })),
                ).await;
            }
            ProjectMethod::InstalledPackages => {
                let installed = {
                    let proj = rt.proj.read().map_err(|_| ServerError::InvalidContext)?;
                    let lock = proj.get_lockfile()?;
                    lock.installed_packages().await?
                };
                rt.send_response(transport, Response::ok(id, installed)).await;
            }
        }

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct OpenProject {
    pub path: PathBuf,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct AddPackages {
    packages: Vec<PackageReference>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct RemovePackages {
    packages: Vec<PackageReference>,
}
