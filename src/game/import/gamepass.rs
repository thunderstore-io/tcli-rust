use std::path::PathBuf;

use super::ImportOverrides;
use crate::error::Error;
use crate::game::error::GameError;
use crate::game::registry::ActiveDistribution;
use crate::ts::v1::models::ecosystem::{GameDef, GameDefPlatform};
use crate::util::reg::{self, HKey};

pub fn get_gamedist(ident: &str, game_def: &GameDef, overrides: &ImportOverrides) -> Result<Option<ActiveDistribution>, Error> {
    let root = r#"Software\Microsoft\GamingServices\PackageRepository"#;
    let r2mm = game_def.r2modman.as_ref().expect(
        "Expected a valid r2mm field in the ecosystem schema, got nothing. This is a bug."
    ).first().unwrap();

    let uuid = reg::get_values_at(HKey::LocalMachine, &format!("{root}\\Package\\"))?
        .into_iter()
        .find(|x| x.key.starts_with(ident))
        .ok_or_else(|| {
            GameError::NotFound(ident.into(), "Gamepass".to_string())
        })?
        .val
        .replace('\"', "");

    let game_root = reg::get_keys_at(HKey::LocalMachine, &format!("Root\\{}\\", uuid))?
        .into_iter()
        .next()
        .ok_or_else(|| {
            GameError::NotFound(ident.into(), "Gamepass".to_string())
        })?;
    let game_dir = PathBuf::from(reg::get_value_at(HKey::LocalMachine, &game_root, "Root")?);

    let exe_path = overrides
        .custom_exe
        .clone()
        .or_else(|| super::find_game_exe(&r2mm.exe_names, &game_dir))
        .ok_or_else(|| GameError::ExeNotFound {
            possible_names: r2mm.exe_names.clone(),
            base_path: game_dir.clone(),
        })?;

    Ok(Some(ActiveDistribution {
        dist: GameDefPlatform::XboxGamePass {
            identifier: ident.to_string(),
        },
        game_dir: game_dir.to_path_buf(),
        data_dir: game_dir.join(&r2mm.data_folder_name),
        exe_path,
    }))
}
