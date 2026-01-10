use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::error::Error;
use crate::game::ecosystem;
use crate::package::install::bepinex::BpxInstaller;
use crate::package::install::tracked::TrackedFs;
use crate::package::Package;
use crate::project::state::StateEntry;
use crate::ts::package_reference::PackageReference;
use crate::ts::v1::models::ecosystem::R2MLLoader;

pub mod api;
mod legacy_compat;
pub mod manifest;
pub mod bepinex;
pub mod tracked;

pub trait PackageInstaller {
    /// Install a package into this profile.
    /// 
    /// `state_dir` is the directory that is "linked" to at runtime by the modloader.
    /// `staging_dir` is the directory that contains files that are directly installed into the game directory.
    async fn install_package(
        &mut self,
        package: &Package,
        package_dir: &Path,
        state_dir: &Path,
        staging_dir: &Path,
        is_modloader: bool,
    ) -> Result<(), Error>;

    /// Uninstall a package from this profile.
    async fn uninstall_package(
        &mut self,
        package: &Package,
        package_dir: &Path,
        state_dir: &Path,
        staging_dir: &Path,
        is_modloader: bool,
    ) -> Result<(), Error>;

    /// Start the game.
    async fn start_game(
        mods_enabled: bool,
        state_dir: &Path,
        game_dir: &Path,
        game_exe: &Path,
        args: Vec<String>,
    ) -> Result<u32, Error>;

    /// Extract the tracked state from this installer, consuming it.
    fn extract_state(self) -> StateEntry;
}

/// Get the proper installer for the provided modloader variant.
pub fn get_installer<T: TrackedFs>(ml_variant: &R2MLLoader, fs: T) -> impl PackageInstaller {
    match ml_variant {
        R2MLLoader::BepInEx => BpxInstaller::new(fs),
        _ => panic!("Support for modloader {ml_variant:?} has not been implemented."),
    }
}

/// Determine the modloader to use for the given packages.
pub async fn guess_modloader(packages: &[PackageReference]) -> Option<R2MLLoader> {
    let schema = ecosystem::get_schema().await.ok()?;
    let ml: HashMap<String, R2MLLoader> = schema
        .modloader_packages
        .into_iter()
        .map(|x| (x.package_id, x.loader))
        .collect();

    packages
        .iter()
        .find_map(|x| ml.get(&x.to_loose_ident_string()).cloned())
}

/// Determine which packages are modloaders.
pub async fn get_modloader_packages(packages: &[PackageReference]) -> HashSet<String> {
    let Ok(schema) = ecosystem::get_schema().await else {
        return HashSet::new();
    };
    
    let ml_ids: HashSet<String> = schema
        .modloader_packages
        .into_iter()
        .map(|x| x.package_id)
        .collect();

    packages
        .iter()
        .filter(|x| ml_ids.contains(&x.to_loose_ident_string()))
        .map(|x| x.to_loose_ident_string())
        .collect()
}
