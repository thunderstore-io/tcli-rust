use std::path::PathBuf;

use super::ImportOverrides;
use crate::error::Error;
use crate::game::error::GameError;
use crate::game::registry::ActiveDistribution;
use crate::ts::v1::models::ecosystem::{GameDef, GameDefPlatform};
use crate::util::reg::{self, HKey};

pub fn get_gamedist(ident: &str, game_def: &GameDef, overrides: &ImportOverrides) -> Result<Option<ActiveDistribution>, Error> {
    let subkey = format!("Software\\{}\\", ident.replace('.', "\\"));
    let value = reg::get_value_at(HKey::LocalMachine, &subkey, "Install Dir")?;

    let game_dir = PathBuf::from(value);
    let r2mm = game_def.r2modman.as_ref().expect(
        "Expected a valid r2mm field in the ecosystem schema, got nothing. This is a bug.",
    ).first().unwrap();

    let exe_path = overrides
        .custom_exe
        .clone()
        .or_else(|| super::find_game_exe(&r2mm.exe_names, &game_dir))
        .ok_or_else(|| GameError::ExeNotFound {
            possible_names: r2mm.exe_names.clone(),
            base_path: game_dir.clone(),
        })?;

    Ok(Some(ActiveDistribution {
        dist: GameDefPlatform::Origin {
            identifier: ident.to_string(),
        },
        game_dir: game_dir.to_path_buf(),
        data_dir: game_dir.join(&r2mm.data_folder_name),
        exe_path,
    }))
}
