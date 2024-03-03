use std::path::PathBuf;

use super::{Error, GameImporter};
use crate::game::import::ImportBase;
use crate::game::registry::{ActiveDistribution, GameData};
use crate::ts::v1::models::ecosystem::GameDefPlatform;
use crate::util::reg::{self, HKey};

pub struct EaImporter {
    ident: String,
}

impl EaImporter {
    pub fn new(ident: &str) -> EaImporter {
        EaImporter {
            ident: ident.into(),
        }
    }
}

impl GameImporter for EaImporter {
    fn construct(self: Box<Self>, base: ImportBase) -> Result<GameData, Error> {
        let subkey = format!("Software\\{}\\", self.ident.replace('.', "\\"));
        let value = reg::get_value_at(HKey::LocalMachine, &subkey, "Install Dir")?;

        let game_dir = PathBuf::from(value);
        let r2mm = base.game_def.r2modman.as_ref().expect(
            "Expected a valid r2mm field in the ecosystem schema, got nothing. This is a bug.",
        );

        let exe_path = base
            .overrides
            .custom_exe
            .clone()
            .or_else(|| super::find_game_exe(&r2mm.exe_names, &game_dir))
            .ok_or_else(|| {
                super::Error::ExeNotFound(base.game_def.label.clone(), game_dir.clone())
            })?;
        let dist = ActiveDistribution {
            dist: GameDefPlatform::Origin {
                identifier: self.ident.to_string(),
            },
            game_dir: game_dir.to_path_buf(),
            data_dir: game_dir.join(&r2mm.data_folder_name),
            exe_path,
        };

        Ok(super::construct_data(base, dist))
    }
}
