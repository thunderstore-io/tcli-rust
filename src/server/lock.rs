use std::fs::{self, File};
use std::path::{Path, PathBuf};

/// Only one server process can "own" a project at a single time.
/// We enforce this exclusive access through the creation and deletion of this lockfile.
const LOCKFILE: &str = ".server-lock";

pub struct ProjectLock {
    file: File,
    path: PathBuf,
}

impl ProjectLock {
    /// Attempt to acquire a lock on the provided directory.
    pub fn lock(project_path: &Path) -> Option<Self> {
        let lock = project_path.join(LOCKFILE);
        match lock.is_file() {
            true => None,
            false => {
                let file = File::create(&lock).ok()?;
                Some(Self { file, path: lock })
            }
        }
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        // The file handle is dropped before this, so we can safely delete it.
        fs::remove_file(&self.path).expect("Failed to remove lockfile.");
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use tempfile::TempDir;

    /// Test that project locks behave in the following way:
    /// - Attempting to lock an already locked project MUST return None.
    /// - Attempting to lock a project that is not locked will return a ProjectLock.
    /// - Dropping a ProjectLock will remove the lockfile from the project.
    #[test]
    fn test_project_lock() {
        let temp = TempDir::new().unwrap();
        let root = temp.path();

        // Acquire a lock on the directory.
        {
            let _lock =
                ProjectLock::lock(root).expect("Failed to acquire lock on empty directory.");

            // Attempting to acquire it again will return None.
            let relock = ProjectLock::lock(root);
            assert!(relock.is_none());
        }

        // Now that the previous lock is dropped we *should* be able to re-acquire it.
        let _lock = ProjectLock::lock(root).expect("Failed to acquire lock on empty directory.");
    }
}
