use std::fs::{self, File};
use std::io;
use std::path::Path;
use md5::{Digest, Md5};
use md5::digest::FixedOutput;
use walkdir::WalkDir;
use crate::error::Error;

pub fn md5(file: &Path) -> Result<String, Error> {
    let mut md5 = Md5::new();
    let mut file = File::open(file)?;
    io::copy(&mut file, &mut md5)?;

    Ok(format!("{:x}", md5.finalize_fixed()))
}

// Recursively remove empty directories starting at a given path.
pub fn remove_empty_dirs(root: &Path, remove_root: bool) -> Result<(), Error> {
    if root.is_file() || !root.exists() {
        Err(Error::DirectoryNotFound(root.to_path_buf()))?;
    }

    let dirs = WalkDir::new(root)
        .into_iter()
        .filter_map(|x| x.ok())
        .filter(|x| x.path().is_dir())
        .filter(|x| !x.path_is_symlink())
        .filter(|x| {
            !fs::read_dir(x.path())
                .unwrap_or_else(|_| panic!("Failed to read the contents of {:?}.", x.path()))
                .filter_map(|x| x.ok())
                .any(|x| x.path().is_file())
        })
        .map(|x| x.path().to_owned())
        .collect::<Vec<_>>()
        .into_iter()
        .rev();

    for dir in dirs {
        let mut contents = fs::read_dir(&dir)?;

        // Skip removing the root-most directory if remove_root is set.
        if dir == root && !remove_root {
            continue;
        }

        // Skip directories that don't exist or are not empty.
        if !dir.exists() || contents.next().is_some() {
            continue;
        }

        println!("{dir:?}");
        fs::remove_dir(dir)?;
    }

    Ok(())
}