use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use md5::digest::FixedOutput;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::ts::package_reference::PackageReference;
use crate::util;

#[derive(Serialize, Deserialize, Debug)]
pub struct StagedFile {
    pub orig: PathBuf,
    pub dest: Vec<PathBuf>,
    pub md5: String,
}

impl StagedFile {
    pub fn new(file: &Path) -> Result<Self, Error> {
        Ok(StagedFile {
            orig: file.to_path_buf(),
            dest: vec![],
            md5: util::file::md5(file)?,
        })
    }

    pub fn is_same_as(&self, other: &Path) -> Result<bool, Error> {
        if !other.is_file() {
            return Ok(false);
        }

        let other_md5 = util::file::md5(other)?;
        Ok(self.md5 == other_md5)
    }

    pub fn copy_to(&mut self, dest: &Path) -> Result<(), Error> {
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Default)]
pub struct StateEntry {
    pub staged: Vec<StagedFile>,
    pub linked: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct StateFile {
    pub state: HashMap<PackageReference, StateEntry>,
}

impl StateFile {
    pub fn open(path: &Path) -> Result<Self, Error> {
        if !path.is_file() {
            Err(Error::FileNotFound(path.to_path_buf()))?;
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
