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
        &mut self,
        package: &PackageReference,
        _package_deps: &[PackageReference],
        package_dir: &Path,
        state_dir: &Path,
        _staging_dir: &Path,
        game_dir: &Path,
        is_modloader: bool,
    ) -> Result<(), Error> {
        if is_modloader {
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

            self.fs.dir_copy(&bep_dir, &bep_dst).await.unwrap();

            // Install top-level doorstop files.
            let files = fs::read_dir(bepinex_root)
                .unwrap()
                .filter_map(|x| x.ok())
                .filter(|x| x.path().is_file());

            for file in files {
                let dest = game_dir.join(file.path().file_name().unwrap());
                self.fs.file_copy(&file.path(), &dest, None).await?;
            }

            return Ok(());
        }

        let state_dir = state_dir.canonicalize()?;
        let full_name= format!("{}-{}", package.namespace, package.name);

        let targets = vec![
            ("plugins", true),
            ("patchers", true),
            ("monomod", true),
            ("config", false),
        ].into_iter()
        .map(|(x, y)| (Path::new(x), y));

        let default = state_dir.join("BepInEx/plugins");
        for (target, relocate) in targets {
            // Packages may either have the target at their tld or BepInEx/target.
            let src = match package_dir.join("BepInEx").exists() {
                true => package_dir.join("BepInEx").join(target),
                false => package_dir.join(target),
            };
            
            // let src = package_dir.join(target);
            let dest = state_dir.join("BepInEx").join(target);

            if !src.exists() {
                continue;
            }

            if !dest.exists() {
                fs::create_dir_all(&dest)?;
            }

            // Copy the directory contents of the target into the destination.
            let entries = fs::read_dir(&src)?
                .filter_map(|x| x.ok());

            for entry in entries {
                let entry = entry.path();

                let entry_dest = match relocate {
                    true => dest.join(&full_name).join(entry.file_name().unwrap()),
                    false => dest.join(entry.file_name().unwrap()),
                };

                let entry_parent = entry_dest.parent().unwrap();

                if !entry_parent.is_dir() {
                    fs::create_dir_all(entry_parent)?;
                }

                if entry.is_dir(){
                    self.fs.dir_copy(&entry, &entry_dest).await?;
                }

                if entry.is_file() {
                    self.fs.file_copy(&entry, &entry_dest, None).await?;
                }
            }
        }

        // Copy top-level files into the plugin directory.
        let tl_files = fs::read_dir(package_dir)?
            .filter_map(|x| x.ok())
            .filter(|x| x.path().is_file());

        for file in tl_files {
            let parent = default.join(&full_name);
            let dest = parent.join(file.file_name());

            if !parent.exists() {
                fs::create_dir_all(&parent)?;
            }

            self.fs.file_copy(&file.path(), &dest, None).await?;
        }

        Ok(())
    }

    async fn uninstall_package(
        &mut self,
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
