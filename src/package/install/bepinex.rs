use std::{fs, path::Path};

use walkdir::WalkDir;

use crate::error::Error;
use crate::package::install::tracked::TrackedFs;
use crate::package::install::PackageInstaller;
use crate::ts::package_reference::PackageReference;

pub struct BpxInstaller<T: TrackedFs> {
    fs: T,
}

impl<T: TrackedFs> BpxInstaller<T> {
    pub fn new(fs: T) -> Self {
        BpxInstaller { fs }
    }
}

impl<T: TrackedFs> PackageInstaller<T> for BpxInstaller<T> {
    async fn install_package(
        &self,
        _package: &PackageReference,
        _package_deps: &[PackageReference],
        package_dir: &Path,
        state_dir: &Path,
        _staging_dir: &Path,
        game_dir: &Path,
        _is_modloader: bool,
    ) -> Result<(), Error> {
        // Figure out the root bepinex directory. This should, in theory, always be the folder
        // that contains the winhttp.dll binary.
        let bepinex_root = WalkDir::new(package_dir)
            .into_iter()
            .filter_map(|x| x.ok())
            .filter(|x| x.path().is_file())
            .find(|x| x.path().file_name().unwrap() == "winhttp.dll")
            .expect("Failed to find winhttp.dll within BepInEx directory.");
        let bepinex_root = bepinex_root.path().parent().unwrap();

        let bep_dir = bepinex_root.join("BepInEx");
        let bep_dst = state_dir.join("BepInEx");

        // self.fs.dir_copy(&bep_dir, &bep_dst).await.unwrap();

        // Install top-level doorstop files.
        let files = fs::read_dir(bepinex_root)
            .unwrap()
            .filter_map(|x| x.ok())
            .filter(|x| x.path().is_file());

        for file in files {
            let dest = game_dir.join(file.path().file_name().unwrap());
            // self.fs.file_copy(&file.path(), &dest, None).await?;
        }

        Ok(())
    }

    async fn uninstall_package(
        &self,
        _package: &PackageReference,
        _package_deps: &[PackageReference],
        _package_dir: &Path,
        _state_dir: &Path,
        _staging_dir: &Path,
        _game_dir: &Path,
        _is_modloader: bool,
    ) -> Result<(), Error> {
        todo!()
    }
}
