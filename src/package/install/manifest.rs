use serde::{Deserialize, Serialize};

use crate::util::os::{Arch, OS};

/// This manifest exists alongside the typical Thunderstore package manifest.
/// It contains additional information about the type of installer, the runner that will be used,
/// dependencies, etc.
#[derive(Serialize, Deserialize)]
pub struct InstallerManifest {
    pub installer_version: u32,
    pub matrix: Vec<InstallerMatrix>,
}

#[derive(Serialize, Deserialize)]
pub struct InstallerMatrix {
    pub target_os: OS,
    pub architecture: Arch,
    pub executable: String,
}
