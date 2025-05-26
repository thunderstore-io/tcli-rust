use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::ImportOverrides;
use crate::error::{Error, IoError};
use crate::game::error::GameError;
use crate::game::registry::ActiveDistribution;
use crate::ts::v1::models::ecosystem::{GameDef, GameDefPlatform};
use crate::util::reg::{self, HKey};

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct PartialInstallManifest {
    install_location: PathBuf,
    app_name: String,
}

pub fn get_gamedist(ident: &str, game_def: &GameDef, overrides: &ImportOverrides) -> Result<Option<ActiveDistribution>, Error> {
    let game_label = game_def.label.clone();

    // There's a couple ways that we can retrieve the path of a game installed via EGS.
    // 1. Parse LauncherInstalled.dat in C:/ProgramData/Epic/UnrealEngineLauncher/
    // 2. Parse game manifest files in C:/ProgramData/Epic/EpicGamesLauncher/Data/Manifests
    // I'm going to go for the second option.

    // Attempt to get the path of the EGS /Data directory from the registry.
    let subkey = r#"Software\WOW64Node\Epic Games\EpicGamesLauncher"#;
    let value = reg::get_value_at(HKey::LocalMachine, subkey, "AppDataPath")?;
    let manifests_dir = PathBuf::from(value).join("Manifests");

    if !manifests_dir.exists() {
        Err(IoError::DirNotFound(manifests_dir.clone()))?;
    }

    // Manifest files are JSON files with .item extensions.
    let manifest_files = fs::read_dir(manifests_dir)
        .unwrap()
        .filter_map(|x| x.ok())
        .map(|x| x.path())
        .filter(|x| x.is_file() && x.extension().is_some())
        .filter(|x| x.extension().unwrap() == "item")
        .collect::<Vec<_>>();

    // Search for the manifest which contains the correct game AppName.
    let game_dir = manifest_files
        .into_iter()
        .find_map(|x| {
            let file_contents = fs::read_to_string(x).unwrap();
            let manifest: PartialInstallManifest =
                serde_json::from_str(&file_contents).unwrap();

            if manifest.app_name == ident {
                Some(manifest.install_location)
            } else {
                None
            }
        })
        .ok_or_else(|| GameError::NotFound(game_label.clone(), "EGS".to_string()))?;

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
        dist: GameDefPlatform::Other,
        game_dir: game_dir.to_path_buf(),
        data_dir: game_dir.join(&r2mm.data_folder_name),
        exe_path,
    }))
}
