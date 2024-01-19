use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::package::install::api::TrackedFile;
use crate::ts::package_reference::PackageReference;
use crate::util;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StagedFile {
    pub action: TrackedFile,
    pub dest: Vec<PathBuf>,
    pub md5: String,
}

impl StagedFile {
    pub fn new(action: TrackedFile) -> Result<Self, Error> {
        let md5 = util::file::md5(&action.path)?;
        Ok(StagedFile {
            action,
            dest: vec![],
            md5,
        })
    }

    pub fn is_same_as(&self, other: &Path) -> Result<bool, Error> {
        if !other.is_file() {
            return Ok(false);
        }

        let other_md5 = util::file::md5(other)?;
        Ok(self.md5 == other_md5)
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct StateEntry {
    pub staged: Vec<StagedFile>,
    pub linked: Vec<TrackedFile>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct StateFile {
    pub state: HashMap<PackageReference, StateEntry>,
}

impl StateFile {
    pub fn open_or_new(path: &Path) -> Result<Self, Error> {
        if !path.is_file() {
            let empty = StateFile::default();
            empty.write(path)?;
        }

        let contents = fs::read_to_string(path)?;
        let statefile = serde_json::from_str(&contents)?;

        Ok(statefile)
    }

    pub fn write(self, path: &Path) -> Result<(), Error> {
        let ser = serde_json::to_string_pretty(&self)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.write_all(ser.as_bytes())?;

        Ok(())
    }
}
