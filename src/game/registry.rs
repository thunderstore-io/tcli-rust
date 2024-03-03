use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::ts::v1::models::ecosystem::GameDefPlatform;
use crate::util::os::OS;

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct GameData {
    pub ecosystem_label: String,
    pub identifier: String,
    pub display_name: String,
    pub active_distribution: ActiveDistribution,
    pub possible_distributions: Vec<GameDefPlatform>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct ActiveDistribution {
    pub dist: GameDefPlatform,
    pub game_dir: PathBuf,
    pub data_dir: PathBuf,
    pub exe_path: PathBuf,
}

pub fn get_supported_platforms(target_os: &OS) -> Vec<&'static str> {
    let mut platforms = vec!["Steam", "DRM Free"];

    if matches!(target_os, OS::Windows) {
        platforms.extend(vec!["Epic Games Store (EGS)", "PC Game Pass", "EA Desktop"]);
    };

    platforms
}

pub fn get_registry(game_registry: &Path) -> Result<Vec<GameData>, Error> {
    let contents = fs::read_to_string(game_registry)?;

    Ok(serde_json::from_str(&contents)?)
}

pub fn get_game_data(game_registry: &Path, game_id: &str) -> Option<GameData> {
    let game_registry: Vec<GameData> = {
        let contents = fs::read_to_string(game_registry).ok()?;

        serde_json::from_str(&contents).ok()?
    };

    game_registry.into_iter().find(|x| x.identifier == game_id)
}

pub fn write_data(game_registry: &Path, data: GameData) -> Result<(), Error> {
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(game_registry)?;

    let mut game_registry: Vec<GameData> = {
        let contents = fs::read_to_string(game_registry)?;

        if contents.is_empty() {
            Vec::new()
        } else {
            serde_json::from_str(&contents).unwrap()
        }
    };

    if game_registry.contains(&data) {
        return Ok(());
    }

    game_registry.push(data);

    let data_json = serde_json::to_string_pretty(&game_registry).unwrap();
    file.write_all(data_json.as_bytes())?;

    Ok(())
}

