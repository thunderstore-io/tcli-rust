use std::path::PathBuf;

use steamlocate::SteamDir;

use super::{Error, GameImporter, ImportBase};
use crate::game::registry::{ActiveDistribution, GameData};
use crate::ts::v1::models::ecosystem::GameDefPlatform;

pub struct SteamImporter {
    appid: u32,
    steam_dir: Option<PathBuf>,
}

impl SteamImporter {
    pub fn new(appid: &str) -> Self {
        SteamImporter {
            steam_dir: None,
            appid: appid
                .parse::<u32>()
                .expect("Got a bad appid from the ecosystem schema. This is a bug"),
        }
    }

    pub fn with_steam_dir(self, steam_dir: Option<PathBuf>) -> Self {
        SteamImporter { steam_dir, ..self }
    }
}

impl GameImporter for SteamImporter {
    fn construct(self: Box<Self>, base: ImportBase) -> Result<GameData, Error> {
        // If an app_dir is provided then we can skip automatic path resolution. If not,
        // attempt to resolve the app's directory from the steam dir, whether provided or otherwise.
        let app_dir = match base.overrides.game_dir {
            Some(ref game_dir) => game_dir.clone(),
            None => {
                let steam = self
                    .steam_dir
                    .as_ref()
                    .map_or_else(SteamDir::locate, |x| SteamDir::from_dir(x))
                    .map_err(|e: steamlocate::Error| match e {
                        steamlocate::Error::InvalidSteamDir(_) => {
                            Error::SteamDirBadPath(self.steam_dir.as_ref().unwrap().to_path_buf())
                        }
                        steamlocate::Error::FailedLocate(_) => Error::SteamDirNotFound,
                        _ => unreachable!(),
                    })?;

                let (app, lib) = steam
                    .find_app(self.appid)
                    .unwrap_or_else(|e| {
                        panic!(
                            "An error occured while searching for app with id '{}': {e:?}.",
                            self.appid
                        )
                    })
                    .ok_or_else(|| {
                        Error::SteamAppNotFound(self.appid, steam.path().to_path_buf())
                    })?;
                lib.resolve_app_dir(&app)
            }
        };

        if !app_dir.is_dir() {
            Err(Error::SteamDirNotFound)?;
        }

        let r2mm = base.game_def.r2modman.as_ref().expect(
            "Expected a valid r2mm field in the ecosystem schema, got nothing. This is a bug.",
        );

        let exe_path = r2mm
            .exe_names
            .iter()
            .map(|x| app_dir.join(x))
            .find(|x| x.is_file())
            .ok_or_else(|| {
                super::Error::ExeNotFound(base.game_def.label.clone(), app_dir.clone())
            })?;

        let dist = ActiveDistribution {
            dist: GameDefPlatform::Steam {
                identifier: self.appid.to_string(),
            },
            data_dir: app_dir.join(&r2mm.data_folder_name),
            game_dir: app_dir,
            exe_path,
        };

        Ok(super::construct_data(base, dist))
    }
}
