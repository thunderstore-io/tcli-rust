use std::path::{Path, PathBuf};

use crate::ts::error::ApiError;

use crate::game::error::GameError;
use crate::package::error::PackageError;
use crate::project::error::ProjectError;

#[derive(Debug, thiserror::Error)]
#[repr(u32)]
pub enum Error {
    #[error("{0}")]
    Game(#[from] GameError),

    #[error("{0}")]
    Package(#[from] PackageError),

    #[error("{0}")]
    Project(#[from] ProjectError),

    #[error("{0}")]
    Api(#[from] ApiError),

    #[error("{0}")]
    Io(#[from] IoError),

    #[error("{0}")]
    JsonParse(#[from] serde_json::Error),

    #[error("{0}")]
    TomlDeserialize(#[from] toml::de::Error),

    #[error("{0}")]
    TomlSerialize(#[from] toml::ser::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum IoError {
    #[error("A file IO error occured: {0}.")]
    Native(std::io::Error, Option<PathBuf>),

    #[error("File not found: {0}.")]
    FileNotFound(PathBuf),

    #[error("Expected directory at '{0}', got file.")]
    DirectoryIsFile(PathBuf),

    #[error("Directory not found: {0}.")]
    DirNotFound(PathBuf),

    #[error("{0}")]
    DirWalker(walkdir::Error),

    #[error("Failed to find file '{0}' within the directory '{1}.")]
    FailedFileSearch(String, PathBuf),

    #[error("Failed to read subkey at '{0}'.")]
    RegistrySubkeyRead(String),

    #[error("Failed to read value with name '{0}' at key '{1}'.")]
    RegistryValueRead(String, String),

    #[error("Failed modifying zip file: {0}.")]
    ZipError(#[from] zip::result::ZipError),
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(IoError::Native(value, None))
    }
}

impl From<reqwest::Error> for Error {
    fn from(value: reqwest::Error) -> Self {
        Self::Api(ApiError::BadRequest { source: value, response_body: None })
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Io(IoError::ZipError(value))
    }
}

pub trait IoResultToTcli<R> {
    fn map_fs_error(self, path: impl AsRef<Path>) -> Result<R, IoError>;
}

impl<R> IoResultToTcli<R> for Result<R, std::io::Error> {
    fn map_fs_error(self, path: impl AsRef<Path>) -> Result<R, IoError> {
        self.map_err(|e| IoError::Native(e, Some(path.as_ref().into())))
    }
}

pub trait ReqwestToTcli: Sized {
    async fn error_for_status_tcli(self) -> Result<Self, ApiError>;
}

impl ReqwestToTcli for reqwest::Response {
    async fn error_for_status_tcli(self) -> Result<Self, ApiError> {
        match self.error_for_status_ref() {
            Ok(_) => Ok(self),
            Err(err) => Err(ApiError::BadRequest {
                source: err,
                response_body: self.text().await.ok(),
            }),
        }
    }
}
