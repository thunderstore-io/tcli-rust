use std::path::PathBuf;

use crate::ts::v1::models::ecosystem::GameDefPlatform;
use crate::util::reg::{self, HKey};
use crate::game::registry::{ActiveDistribution, GameData};

use super::{GameImporter, ImportBase};
use super::Error;

pub struct GamepassImporter {
    ident: String,
}

impl GamepassImporter {
    pub fn new(ident: &str) -> GamepassImporter {
        GamepassImporter {
            ident: ident.into(),
        }
    }
}

impl GameImporter for GamepassImporter {
    fn construct(self: Box<Self>, base: ImportBase) -> Result<GameData, Error> {
        let root = r#"Software\Microsoft\GamingServices\PackageRepository"#;

        let uuid = reg::get_values_at(HKey::LocalMachine, &format!("{root}\\Package\\"))?
            .into_iter()
            .find(|x| x.key.starts_with(&self.ident))
            .ok_or_else(|| super::Error::NotFound(base.game_def.label.clone(), "Gamepass".to_string()))?
            .val
            .replace('\"', "");

        let game_root = reg::get_keys_at(HKey::LocalMachine, &format!("Root\\{}\\", uuid))?
            .into_iter()
            .next()
            .ok_or_else(|| super::Error::NotFound(base.game_def.label.clone(), "Gamepass".to_string()))?;
        let game_dir = PathBuf::from(reg::get_value_at(HKey::LocalMachine, &game_root, "Root")?);

        let r2mm = base
            .game_def
            .r2modman
            .as_ref()
            .expect("Expected a valid r2mm field in the ecosystem schema, got nothing. This is a bug.");

        let exe_path = base
            .overrides
            .custom_exe
            .clone()
            .or_else(|| super::find_game_exe(&r2mm.exe_names, &game_dir))
            .ok_or_else(|| super::Error::ExeNotFound(base.game_def.label.clone(), game_dir.clone()))?;
        let dist = ActiveDistribution {
            dist: GameDefPlatform::GamePass { identifier: self.ident.to_string() },
            game_dir: game_dir.to_path_buf(),
            data_dir: game_dir.join(&r2mm.data_folder_name),
            exe_path,
        };

        Ok(super::construct_data(base, dist))
    }
}