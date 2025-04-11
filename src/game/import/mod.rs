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
use crate::game::import::ea::EaImporter;
use crate::game::import::egs::EgsImporter;
use crate::game::import::gamepass::GamepassImporter;
use crate::game::import::steam::SteamImporter;
use crate::ts::v1::models::ecosystem::{GameDef, GameDefPlatform};

pub trait GameImporter {
    fn construct(self: Box<Self>, base: ImportBase) -> Result<GameData, Error>;
}

#[derive(Default)]
pub struct ImportOverrides {
    pub custom_name: Option<String>,
    pub custom_id: Option<String>,
    pub custom_exe: Option<PathBuf>,
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
            .await
            .unwrap()
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
}

pub fn select_importer(base: &ImportBase) -> Result<Box<dyn GameImporter>, Error> {
    base.game_def
        .distributions
        .iter()
        .find_map(|dist| match dist {
            GameDefPlatform::Origin { identifier } => {
                Some(Box::new(EaImporter::new(identifier)) as _)
            }
            GameDefPlatform::EpicGames { identifier } => {
                Some(Box::new(EgsImporter::new(identifier)) as _)
            }
            GameDefPlatform::GamePass { identifier } => {
                Some(Box::new(GamepassImporter::new(identifier)) as _)
            }
            GameDefPlatform::Steam { identifier } => {
                Some(Box::new(SteamImporter::new(identifier)) as _)
            }
            GameDefPlatform::SteamDirect { identifier } => {
                Some(Box::new(SteamImporter::new(identifier)) as _)
            }
            _ => None,
        })
        .ok_or_else(|| GameError::NotSupported(base.game_id.clone(), "".into()).into())
}

pub fn find_game_exe(possible: &[String], base_path: &Path) -> Option<PathBuf> {
    possible
        .iter()
        .map(|x| base_path.join(x))
        .find(|x| x.is_file())
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
