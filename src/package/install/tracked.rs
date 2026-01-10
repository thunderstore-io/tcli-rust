use std::path::Path;
use tokio::fs;
use walkdir::WalkDir;

use crate::package::install::api::{FileAction, LinkedFile};
use crate::project::state::{StagedFile, StateEntry};

use crate::error::Error;

pub trait TrackedFs {
    /// Create a new instance dedicated to tracking filesystem edits during the 
    /// installation of the provided package.
    /// 
    /// This essentially creates or opens the cooresponding entry within the 
    /// tracked_files.json file and writes any tracked fs modifications to it.
    fn new(state: StateEntry) -> Self;

    /// Extract the new StateEntry from this instance.
    fn extract_state(self) -> StateEntry;

    /// Copy a file from a source to a destination, overwriting it if the file
    /// already exists.
    /// 
    /// This will append (or overwrite) a FileAction::Create entry.
    async fn file_copy(&mut self, src: &Path, dst: &Path, stage_dst: Option<&Path>) -> Result<(), Error>;

    /// Delete some target file.
    /// 
    /// If `tracked` is set this this will append a FileAction::Delete entry,
    /// overwriting one if it already exists for this file.
    async fn file_delete(&mut self, target: &Path, tracked: bool);

    /// Recursively copy a source directory to a destination, overwriting it if 
    /// it already exists.
    /// 
    /// This will append (or overwrite) a FileAction::Create entry for each file
    /// copied while recursing.
    async fn dir_copy(&mut self, src: &Path, dst: &Path) -> Result<(), Error>;

    /// Recursively delete some target directory.
    /// 
    /// If `tracked` if set then this will append a FileAction::Delete entry
    /// for each file deleted while recursing, otherwise matching entries are
    /// deleted.
    async fn dir_delete(&mut self, target: &Path, tracked: bool);
}

#[derive(Debug)]
pub struct ConcreteFs {
    state: StateEntry,
}

impl TrackedFs for ConcreteFs { 
    fn new(state: StateEntry) -> Self {
        ConcreteFs {
            state
        }
    }

    fn extract_state(self) -> StateEntry {
        self.state
    }

    async fn file_copy(&mut self, src: &Path, dst: &Path, stage_dst: Option<&Path>) -> Result<(), Error> {
        fs::copy(src, dst).await?;
        let tracked = LinkedFile { action: FileAction::Create, path: dst.to_path_buf(), context: None };

        if let Some(stage_dst) = stage_dst { 
            let mut staged = StagedFile::new(tracked)?;
            staged.dest.push(stage_dst.to_path_buf());
            self.state.add_staged(staged, false);
        } else {
            self.state.add_linked(tracked, false);
        }

        Ok(())
    }

    async fn file_delete(&mut self, _target: &Path, _tracked: bool) { 
        todo!()
    }

    async fn dir_copy(&mut self, src: &Path, dst: &Path) -> Result<(), Error> {
        let files = WalkDir::new(&src)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|x| x.path().is_file());

        for file in files {
            let dest = dst.join(file.path().strip_prefix(&src).unwrap());
            let dest_parent = dest.parent().unwrap();

            if !dest_parent.is_dir() {
                fs::create_dir_all(dest_parent).await?;
            }

            self.file_copy(file.path(), &dest, None).await?;
        }

        Ok(())
    }

    async fn dir_delete(&mut self, _target: &Path, _tracked: bool) {
        todo!()
    }
}
