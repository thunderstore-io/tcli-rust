use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
#[repr(u32)]
pub enum ProjectError {
    #[error("A project configuration already exists at {0}.")]
    ProjectAlreadyExists(PathBuf),

    #[error("No project exists at the path {0}.")]
    NoProjectFile(PathBuf),

    #[error("Project is missing required table '{0}'.")]
    MissingTable(&'static str),

    #[error("Missing repository url.")]
    MissingRepository,

    #[error("The game identifier '{0}' does not exist within the ecosystem schema.")]
    InvalidGameId(String),
}
