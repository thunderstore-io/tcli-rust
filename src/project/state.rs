use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::package::install::api::LinkedFile;
use crate::ts::package_reference::PackageReference;
use crate::util;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct StagedFile {
    pub file: LinkedFile,
    pub dest: Vec<PathBuf>,
    pub md5: String,
}

impl StagedFile {
    pub fn new(file: LinkedFile) -> Result<Self, Error> {
        let md5 = util::file::md5(&file.path)?;
        Ok(StagedFile {
            file,
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

#[derive(Serialize, Deserialize, Default, Debug, Clone)]
pub struct StateEntry {
    pub staged: Vec<StagedFile>,
    pub linked: Vec<LinkedFile>,
}

impl StateEntry {
    /// Add a new staged file. If overwrite is set then already existing
    /// entries with the same source path will be replaced.
    pub fn add_staged(&mut self, file: StagedFile, overwrite: bool) {
        let existing_idx = self.staged.iter().position(|x| x.file.path == file.file.path);

        match existing_idx {
            Some(idx) if overwrite => {
                self.staged[idx] = file;
            }
            Some(_) => {
                // Entry exists and overwrite is false — do nothing
            }
            None => {
                self.staged.push(file);
            }
        }
    }

    /// Add a new linked file. If overwrite is set then already existing
    /// entries with the same path will be replaced.
    pub fn add_linked(&mut self, file: LinkedFile, overwrite: bool) {
        let existing_idx = self.linked.iter().position(|x| x.path == file.path);

        match existing_idx {
            Some(idx) if overwrite => {
                self.linked[idx] = file;
            }
            Some(_) => {
                // Entry exists and overwrite is false — do nothing
            }
            None => {
                self.linked.push(file);
            }
        }
    }

    /// Remove a staged file entry by its source path.
    pub fn remove_staged(&mut self, path: &Path) -> Option<StagedFile> {
        let idx = self.staged.iter().position(|x| x.file.path == path)?;
        Some(self.staged.remove(idx))
    }

    /// Remove a linked file entry by its path.
    pub fn remove_linked(&mut self, path: &Path) -> Option<LinkedFile> {
        let idx = self.linked.iter().position(|x| x.path == path)?;
        Some(self.linked.remove(idx))
    }

    /// Get a staged file by its source path.
    pub fn get_staged(&self, path: &Path) -> Option<&StagedFile> {
        self.staged.iter().find(|x| x.file.path == path)
    }

    /// Get a mutable staged file by its source path.
    pub fn get_staged_mut(&mut self, path: &Path) -> Option<&mut StagedFile> {
        self.staged.iter_mut().find(|x| x.file.path == path)
    }

    /// Get a linked file by its path.
    pub fn get_linked(&self, path: &Path) -> Option<&LinkedFile> {
        self.linked.iter().find(|x| x.path == path)
    }

    /// Check if this entry has any tracked files.
    pub fn is_empty(&self) -> bool {
        self.staged.is_empty() && self.linked.is_empty()
    }

    /// Get the total count of tracked files.
    pub fn file_count(&self) -> usize {
        self.staged.len() + self.linked.len()
    }
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
            return Ok(StateFile::default());
        }

        let contents = fs::read_to_string(path)?;
        let statefile = serde_json::from_str(&contents)?;

        Ok(statefile)
    }

    pub fn write(&self, path: &Path) -> Result<(), Error> {
        let ser = serde_json::to_string_pretty(&self)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        file.write_all(ser.as_bytes())?;

        Ok(())
    }

    /// Get or create a state entry for the given package.
    pub fn entry(&mut self, package: PackageReference) -> &mut StateEntry {
        self.state.entry(package).or_default()
    }

    /// Get the state entry for a package, if it exists.
    pub fn get(&self, package: &PackageReference) -> Option<&StateEntry> {
        self.state.get(package)
    }

    /// Get a mutable state entry for a package, if it exists.
    pub fn get_mut(&mut self, package: &PackageReference) -> Option<&mut StateEntry> {
        self.state.get_mut(package)
    }

    /// Remove a package's state entry entirely, returning it if it existed.
    pub fn remove(&mut self, package: &PackageReference) -> Option<StateEntry> {
        self.state.remove(package)
    }

    /// Check if a package has any tracked state.
    pub fn contains(&self, package: &PackageReference) -> bool {
        self.state.contains_key(package)
    }

    /// Get all packages that have tracked state.
    pub fn packages(&self) -> impl Iterator<Item = &PackageReference> {
        self.state.keys()
    }

    /// Get all staged files across all packages.
    pub fn all_staged(&self) -> impl Iterator<Item = (&PackageReference, &StagedFile)> {
        self.state
            .iter()
            .flat_map(|(pkg, entry)| entry.staged.iter().map(move |f| (pkg, f)))
    }

    /// Get all linked files across all packages.
    pub fn all_linked(&self) -> impl Iterator<Item = (&PackageReference, &LinkedFile)> {
        self.state
            .iter()
            .flat_map(|(pkg, entry)| entry.linked.iter().map(move |f| (pkg, f)))
    }

    /// Find which package owns a staged file by its source path.
    pub fn find_staged_owner(&self, path: &Path) -> Option<&PackageReference> {
        self.state
            .iter()
            .find(|(_, entry)| entry.staged.iter().any(|f| f.file.path == path))
            .map(|(pkg, _)| pkg)
    }

    /// Find which package owns a linked file by its path.
    pub fn find_linked_owner(&self, path: &Path) -> Option<&PackageReference> {
        self.state
            .iter()
            .find(|(_, entry)| entry.linked.iter().any(|f| f.path == path))
            .map(|(pkg, _)| pkg)
    }

    /// Remove empty entries (packages with no tracked files).
    pub fn prune_empty(&mut self) {
        self.state.retain(|_, entry| !entry.is_empty());
    }

    /// Get total count of tracked files across all packages.
    pub fn total_file_count(&self) -> usize {
        self.state.values().map(|e| e.file_count()).sum()
    }
}
