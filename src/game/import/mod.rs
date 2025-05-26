pub mod ea;
pub mod egs;
pub mod gamepass;
pub mod nodrm;
pub mod steam;

use std::path::{Path, PathBuf};

use super::ecosystem;
use super::error::GameError;
use super::registry::{ActiveDistribution, GameData};
use crate::error::Error;
use crate::ts::v1::models::ecosystem::{GameDef, GamePlatform};

pub trait GameImporter {
    fn construct(self: Box<Self>, base: ImportBase) -> Result<GameData, Error>;
}

#[derive(Default)]
pub struct ImportOverrides {
    pub custom_name: Option<String>,
    pub custom_id: Option<String>,
    pub custom_exe: Option<PathBuf>,
    pub steam_dir: Option<PathBuf>,
    pub game_dir: Option<PathBuf>,
}

pub struct ImportBase {
    pub game_id: String,
    pub game_def: GameDef,
    pub overrides: ImportOverrides,
    pub wine_prefix: Option<String>,
}

impl ImportBase {
    pub async fn new(game_id: &str) -> Result<Self, Error> {
        let game_def = ecosystem::get_schema()
            .await?
            .games
            .get(game_id)
            .ok_or_else(|| GameError::BadGameId(game_id.into()))?
            .clone();

        Ok(ImportBase {
            game_id: game_id.into(),
            game_def,
            overrides: Default::default(),
            wine_prefix: None,
        })
    }

    pub fn with_overrides(self, overrides: ImportOverrides) -> Self {
        ImportBase { overrides, ..self }
    }

    pub fn with_wine_prefix(self, wine_prefix: Option<String>) -> Self {
        ImportBase {
            wine_prefix,
            ..self
        }
    }

    /// Search the system for valid installs of the specified game id.
    pub fn get_active_dists(&self) -> Result<Vec<ActiveDistribution>, Error> {
        Ok(self.game_def
            .distributions
            .iter()
            .filter_map(|x| self.get_active_dist(x).ok())
            .filter_map(|x| x)
            .collect::<Vec<_>>())
    }

    pub fn get_active_dist(&self, platform: &GamePlatform) -> Result<Option<ActiveDistribution>, Error> {
        match platform {
            GamePlatform::EpicGamesStore { identifier } => {
                egs::get_gamedist(identifier, &self.game_def, &self.overrides)
            },
            GamePlatform::XboxGamePass { identifier } => {
                gamepass::get_gamedist(identifier, &self.game_def, &self.overrides)
            },
            GamePlatform::Origin { identifier } => {
                ea::get_gamedist(identifier, &self.game_def, &self.overrides)
            },
            GamePlatform::Steam { identifier } => {
                steam::get_gamedist(
                    identifier.parse().unwrap(), 
                    self.overrides.steam_dir.as_deref(), 
                    &self.game_def, 
                    &self.overrides,
                )
            },
            GamePlatform::SteamDirect { identifier } => {
                steam::get_gamedist(
                    identifier.parse().unwrap(), 
                    self.overrides.steam_dir.as_deref(), 
                    &self.game_def, 
                    &self.overrides,
                )
            },
            _ => panic!()
        }
    }
    
    pub fn make_gamedata(self, dist: ActiveDistribution) -> GameData {
        GameData {
            identifier: self
                .overrides
                .custom_id
                .unwrap_or(self.game_def.label.clone()),
            ecosystem_label: self.game_def.label,
            display_name: self
                .overrides
                .custom_name
                .unwrap_or(self.game_def.meta.display_name),
            active_distribution: dist,
            possible_distributions: self.game_def.distributions,
        }
    }
}

pub fn find_game_exe(possible: &[String], base_path: &Path) -> Option<PathBuf> {
    possible
        .iter()
        .map(|x| base_path.join(x))
        .find(|x| x.is_file())
}

/// Convert the provided game id and platform name to a full GamePlatform instance, if
/// one can be found in the ecosystem schema.
pub async fn plat_from_name(game_id: &str, plat_name: &str) -> Result<GamePlatform, Error> {
    let ecosystem = ecosystem::get_schema().await?;
    let game = ecosystem.games.get(game_id)
        .ok_or(GameError::BadGameId(game_id.into()))?;

    game
        .distributions
        .iter()
        .find(|x| x.get_platform_name() == plat_name)
        .ok_or(GameError::NotSupported(game_id.into(), plat_name.into()).into())
        .cloned()
}

pub fn construct_data(base: ImportBase, dist: ActiveDistribution) -> GameData {
    GameData {
        identifier: base
            .overrides
            .custom_id
            .unwrap_or(base.game_def.label.clone()),
        ecosystem_label: base.game_def.label,
        display_name: base
            .overrides
            .custom_name
            .unwrap_or(base.game_def.meta.display_name),
        active_distribution: dist,
        possible_distributions: base.game_def.distributions,
    }
}
