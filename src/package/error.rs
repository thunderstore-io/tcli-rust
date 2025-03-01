use crate::ts::version::Version;

#[derive(Debug, thiserror::Error)]
#[repr(u32)]
pub enum PackageError {
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
        "The installer '{package_id}' did not respond correctly:
        \t{message}"
    )]
    InstallerBadResponse { package_id: String, message: String },

    #[error("The installer returned an error:\n\t{message}")]
    InstallerError { message: String },
}
