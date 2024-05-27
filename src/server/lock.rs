use std::fs::{self, File};
use std::path::{Path, PathBuf};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("The lock at {0:?} is already acquired by a process with PID {1}.")]
    InUse(PathBuf, u32),
}

/// Only one server process can "own" a project at a single time.
/// We enforce this exclusive access through the creation and deletion of this lockfile.
const LOCKFILE: &'static str = ".server-lock";

pub struct ProjectLock<'a> {
    file: File,
    path: &'a Path,
}

impl<'a> ProjectLock<'_> {
    /// Attempt to acquire a lock on the provided directory.
    pub fn lock(project_path: &'a Path) -> Result<Self, Error> {
        todo!()
    }
}

impl Drop for ProjectLock<'_> {
    fn drop(&mut self) {
        // The file handle is dropped before this, so we can safely delete it.
        fs::remove_file(self.path);
    }
}
