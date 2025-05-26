use std::path::{Path, PathBuf};

use super::ImportOverrides;
use crate::error::{Error, IoError};
use crate::game::error::GameError;
use crate::game::registry::ActiveDistribution;
use crate::ts::v1::models::ecosystem::{GameDef, GamePlatform};

pub struct NoDrmImporter {
    game_dir: PathBuf,
}

impl NoDrmImporter {
    pub fn new(game_dir: &Path) -> NoDrmImporter {
        NoDrmImporter {
            game_dir: game_dir.to_path_buf(),
        }
    }
}

pub fn get_gamedist(game_dir: &Path, game_def: &GameDef, overrides: &ImportOverrides) -> Result<ActiveDistribution, Error> {
    if !game_dir.exists() {
        Err(IoError::DirNotFound(game_dir.to_path_buf()))?;
    }

    let r2mm = game_def.r2modman.as_ref().expect(
        "Expected a valid r2mm field in the ecosystem schema, got nothing. This is a bug.",
    ).first().unwrap();

    let exe_path = overrides
        .custom_exe
        .clone()
        .or_else(|| super::find_game_exe(&r2mm.exe_names, game_dir))
        .ok_or_else(|| GameError::ExeNotFound {
            possible_names: r2mm.exe_names.clone(),
            base_path: game_dir.to_path_buf(),
        })?;
    Ok(ActiveDistribution {
        dist: GamePlatform::Other,
        game_dir: game_dir.to_path_buf(),
        data_dir: game_dir.join(&r2mm.data_folder_name),
        exe_path,
    })
}
