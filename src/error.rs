use std::path::{Path, PathBuf};

use crate::ts::version::Version;

#[allow(clippy::enum_variant_names)]
#[derive(Debug, thiserror::Error)]
#[repr(u32)]
pub enum Error {
    #[error("An API error occured.")]
    ApiError {
        source: reqwest::Error,
        response_body: Option<String>,
    } = 1,

    #[error("The file at {0} does not exist or is otherwise not accessible.")]
    FileNotFound(PathBuf),

    #[error("The directory at {0} does not exist or is otherwise not accessible.")]
    DirectoryNotFound(PathBuf),

    #[error("A network error occurred while sending an API request.")]
    NetworkError(#[from] reqwest::Error),

    #[error("The path at {0} is actually a file.")]
    ProjectDirIsFile(PathBuf),

    #[error("A project configuration already exists at {0}.")]
    ProjectAlreadyExists(PathBuf),

    #[error("A generic IO error occurred: {0}")]
    GenericIoError(#[from] std::io::Error),

    #[error("A file IO error occurred at path {0}: {1}")]
    FileIoError(PathBuf, std::io::Error),

    #[error("Invalid version.")]
    InvalidVersion(#[from] crate::ts::version::VersionParseError),

    #[error("Failed to read project file. {0}")]
    FailedDeserializeProject(#[from] toml::de::Error),

    #[error("No project exists at the path {0}.")]
    NoProjectFile(PathBuf),

    #[error("Failed modifying zip file: {0}.")]
    ZipError(#[from] zip::result::ZipError),

    #[error("Project is missing required table '{0}'.")]
    MissingTable(&'static str),

    #[error("Missing repository url.")]
    MissingRepository,

    #[error("Missing auth token.")]
    MissingAuthToken,

    #[error("The game identifier '{0}' does not exist within the ecosystem schema.")]
    InvalidGameId(String),

    #[error("An error occurred while parsing JSON: {0}")]
    JsonParserError(#[from] serde_json::Error),

    #[error("An error occured while serializing TOML: {0}")]
    TomlSerializer(#[from] toml::ser::Error),

    #[error("The installer does not contain a valid manifest.")]
    InstallerNoManifest,

    #[error(
        "The installer executable for the current OS and architecture combination does not exist."
    )]
    InstallerNotExecutable,

    #[error(
        "
        The installer '{package_id}' does not support the current tcli installer protocol.
            Expected: {our_version:#?}
            Recieved: {given_version:#?}
    "
    )]
    InstallerBadVersion {
        package_id: String,
        given_version: Version,
        our_version: Version,
    },

    #[error(
        "
        The installer '{package_id}' did not respond correctly:
            {message}
    "
    )]
    InstallerBadResponse { package_id: String, message: String },

    #[error("The installer returned an error:\n\t{message}")]
    InstallerError { message: String },

    #[error("The provided game id '{0}' does not exist or has not been imported into this profile.")]
    BadGameId(String)
}

pub trait IoResultToTcli<R> {
    fn map_fs_error(self, path: impl AsRef<Path>) -> Result<R, Error>;
}

impl<R> IoResultToTcli<R> for Result<R, std::io::Error> {
    fn map_fs_error(self, path: impl AsRef<Path>) -> Result<R, Error> {
        self.map_err(|e| Error::FileIoError(path.as_ref().into(), e))
    }
}

pub trait ReqwestToTcli: Sized {
    async fn error_for_status_tcli(self) -> Result<Self, Error>;
}

impl ReqwestToTcli for reqwest::Response {
    async fn error_for_status_tcli(self) -> Result<Self, Error> {
        match self.error_for_status_ref() {
            Ok(_) => Ok(self),
            Err(err) => Err(Error::ApiError {
                source: err,
                response_body: self.text().await.ok(),
            }),
        }
    }
}

impl From<walkdir::Error> for Error {
    fn from(value: walkdir::Error) -> Self {
        Self::FileIoError(
            value.path().unwrap_or(Path::new("")).into(),
            value.into_io_error().unwrap(),
        )
    }
}
