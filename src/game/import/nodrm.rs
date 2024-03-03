use std::path::{Path, PathBuf};

use crate::game::registry::{ActiveDistribution, GameData};
use crate::ts::v1::models::ecosystem::GameDefPlatform;

use super::{Error, GameImporter, ImportBase};

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

impl GameImporter for NoDrmImporter {
    fn construct(self: Box<Self>, base: ImportBase) -> Result<GameData, Error> {
        if !self.game_dir.exists() {
            Err(Error::DirNotFound(self.game_dir.to_path_buf()))?;
        }

        let r2mm = base
            .game_def
            .r2modman
            .as_ref()
            .expect("Expected a valid r2mm field in the ecosystem schema, got nothing. This is a bug.");

        let exe_path = base
            .overrides
            .custom_exe
            .clone()
            .or_else(|| super::find_game_exe(&r2mm.exe_names, &self.game_dir))
            .ok_or_else(|| super::Error::ExeNotFound(base.game_def.label.clone(), self.game_dir.clone()))?;
        let dist = ActiveDistribution {
            dist: GameDefPlatform::Other,
            game_dir: self.game_dir.to_path_buf(),
            data_dir: self.game_dir.join(&r2mm.data_folder_name),
            exe_path,
        };

        Ok(super::construct_data(base, dist))
    }
}
