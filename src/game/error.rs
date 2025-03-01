use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
#[repr(u32)]
pub enum GameError {
    #[error("The game '{0}' is not supported by platform '{1}'.")]
    NotSupported(String, String),

    #[error("Could not find game with id '{0}' within the ecosystem schema.")]
    BadGameId(String),

    #[error("Could not find the game '{0}' installed via platform '{1}'.")]
    NotFound(String, String),

    #[error("Could not find any of '{possible_names:?}' in base directory: '{base_path}'.")]
    ExeNotFound { possible_names: Vec<String>, base_path: PathBuf},

    #[error("The Steam library could not be automatically found.")]
    SteamDirNotFound,

    #[error("The path '{0}' does not refer to a valid Steam directory.")]
    SteamDirBadPath(PathBuf),

    #[error("The app with id '{0}' could not be found in the Steam instance at '{1}'.")]
    SteamAppNotFound(u32, PathBuf),

    // This should probably live elsewhere but it's fine here for now.
    #[error("An error occured while fetching the ecosystem schema.")]
    EcosystemSchema,
}
