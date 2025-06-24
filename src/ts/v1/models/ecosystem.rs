use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::ts::version::Version;
use crate::game::ecosystem;
use crate::game::error::GameError;


#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct EcosystemSchema {
    pub schema_version: Version,
    pub games: HashMap<String, GameDef>,
    pub communities: HashMap<String, SchemaCommunity>,
    pub modloader_packages: Vec<R2MMModLoaderPackage>,
    pub package_installers: HashMap<String, PackageInstaller>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GameDef {
    pub uuid: String,
    pub label: String,
    pub meta: GameDefMeta,
    pub distributions: Vec<GamePlatform>,
    pub r2modman: Option<Vec<GameDefR2MM>>,
    pub thunderstore: Option<GameDefThunderstore>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GameDefMeta {
    #[serde(default)]
    pub display_name: String,
    pub icon_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, clap::Subcommand)]
#[serde(tag = "platform")]
#[serde(rename_all = "kebab-case")]
pub enum GamePlatform {
    EpicGamesStore {
        identifier: String,
    },
    XboxGamePass {
        identifier: String,
    },
    Origin {
        identifier: String,
    },
    Steam {
        identifier: String,
    },
    SteamDirect {
        identifier: String,
    },
    OculusStore,
    Other,
}

impl GamePlatform {
    /// Hardcoding these for now until we integrate this sorta thing into
    /// the ecosystem schema, preferably as a compile time check.
    pub fn ident_from_name<'a>(&'a self, name: &str) -> Option<&'a str> {
        match (name, self) {
            ("epic-games-store", GamePlatform::EpicGamesStore { identifier }) => Some(identifier),
            ("gamepass", GamePlatform::XboxGamePass { identifier }) => Some(identifier),
            ("origin" | "ea", GamePlatform::Origin { identifier }) => Some(identifier),
            ("steam", GamePlatform::Steam { identifier }) => Some(identifier),
            ("steam-direct", GamePlatform::SteamDirect { identifier }) => Some(identifier),
            _ => None,
        }
    }

    pub fn get_platform_name(&self) -> &'static str {
        match self {
            GamePlatform::EpicGamesStore { identifier: _ } => "epic-games-store",
            GamePlatform::XboxGamePass { identifier: _ } => "gamepass",
            GamePlatform::Origin { identifier: _ } => "origin",
            GamePlatform::Steam { identifier: _ } => "steam",
            GamePlatform::SteamDirect { identifier: _ } => "steam-direct",
            GamePlatform::OculusStore => "oculus-store",
            GamePlatform::Other => "other",
        }
    }

    pub fn get_platform_names(&self) -> Vec<&'static str> {
        vec![
            "origin",
            "epic-games-store",
            "gamepass",
            "steam",
            "steam-direct",
            "oculus-store",
            "other",
        ]
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GameDefR2MM {
    pub meta: GameDefMeta,
    pub internal_folder_name: String,
    pub data_folder_name: String,
    pub distributions: Vec<GamePlatform>,
    pub settings_identifier: String,
    pub package_index: String,
    pub steam_folder_name: String,
    pub exe_names: Vec<String>,
    pub game_instance_type: String,
    pub game_selection_display_mode: String,
    pub additional_search_strings: Vec<String>,
    pub package_loader: Option<String>,
    pub install_rules: Vec<R2MMInstallRule>,
    pub relative_file_exclusions: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct R2MMModLoaderPackage {
    pub package_id: String,
    pub root_folder: String,
    pub loader: R2MLLoader,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "lowercase")]
pub enum R2MLLoader {
    BepInEx,
    GDWeave,
    GodotML,
    Lovely,
    MelonLoader,
    Northstar,
    #[serde(rename = "recursive-melonloader")]
    RecursiveMelonLoader,
    #[serde(rename = "return-of-modding")]
    ReturnOfModding,
    Shimloader,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PackageInstaller {
    pub name: String,
    pub description: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct R2MMInstallRule {
    pub route: String,
    pub tracking_method: Option<String>,
    pub sub_routes: Option<Vec<R2MMInstallRule>>,
    pub default_file_extensions: Option<Vec<String>>,
    pub is_default_location: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GameDefThunderstore {
    pub display_name: String,
    pub categories: HashMap<String, ThunderstoreCategory>,
    pub sections: HashMap<String, ThunderstoreSection>,
    pub discord_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ThunderstoreCategory {
    pub label: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ThunderstoreSection {
    pub name: String,
    #[serde(default)]
    pub exclude_categories: Vec<String>,
    #[serde(default)]
    pub require_categories: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SchemaCommunity {
    pub display_name: String,
    pub categories: HashMap<String, CommunityCategory>,
    pub sections: HashMap<String, CommunitySection>,
    pub discord_url: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CommunityCategory {
    pub label: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CommunitySection {
    pub name: String,
    #[serde(default)]
    pub excluded_categories: Vec<String>,
    #[serde(default)]
    pub required_categories: Vec<String>,
}
