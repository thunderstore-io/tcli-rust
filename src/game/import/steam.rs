use std::path::Path;

use steamlocate::SteamDir;

use super::ImportOverrides;
use crate::error::Error;
use crate::game::error::GameError;
use crate::game::registry::ActiveDistribution;
use crate::ts::v1::models::ecosystem::{GameDef, GameDefPlatform};

pub fn get_gamedist(app_id: u32, steam_dir: Option<&Path>, game_def: &GameDef, overrides: &ImportOverrides) -> Result<Option<ActiveDistribution>, Error> {
    // If an app_dir is provided then we can skip automatic path resolution. If not,
    // attempt to resolve the app's directory from the steam dir, whether provided or otherwise.
    let app_dir = match overrides.game_dir {
        Some(ref game_dir) => game_dir.clone(),
        None => {
            let steam = steam_dir
                .as_ref()
                .map_or_else(SteamDir::locate, |x| SteamDir::from_dir(x))
                .map_err(|e: steamlocate::Error| match e {
                    steamlocate::Error::InvalidSteamDir(_) => GameError::SteamDirBadPath(
                        steam_dir.as_ref().unwrap().to_path_buf(),
                    ),
                    steamlocate::Error::FailedLocate(_) => GameError::SteamDirNotFound,
                    _ => unreachable!(),
                })?;

            let (app, lib) = steam
                .find_app(app_id)
                .unwrap_or_else(|e| {
                    panic!(
                        "An error occured while searching for app with id '{}': {e:?}.",
                        app_id
                    )
                })
                .ok_or_else(|| {
                    GameError::SteamAppNotFound(app_id, steam.path().to_path_buf())
                })?;
            lib.resolve_app_dir(&app)
        }
    };

    if !app_dir.is_dir() {
        Err(GameError::SteamDirNotFound)?;
    }

    let r2mm = game_def.r2modman.as_ref().expect(
        "Expected a valid r2mm field in the ecosystem schema, got nothing. This is a bug.",
    ).first().unwrap();

    let exe_path = r2mm
        .exe_names
        .iter()
        .map(|x| app_dir.join(x))
        .find(|x| x.is_file())
        .ok_or_else(|| GameError::ExeNotFound {
            possible_names: r2mm.exe_names.clone(),
            base_path: app_dir.clone(),
        })?;

    Ok(Some(ActiveDistribution {
        dist: GameDefPlatform::Steam {
            identifier: app_id.to_string(),
        },
        data_dir: app_dir.join(&r2mm.data_folder_name),
        game_dir: app_dir,
        exe_path,
    }))
}
