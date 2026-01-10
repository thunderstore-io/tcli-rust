
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;


use error::ProjectError;
use futures::future::try_join_all;
pub use publish::publish;
use tokio::sync::Semaphore;
use zip::write::SimpleFileOptions;

use self::lock::LockFile;
use crate::error::{Error, IoError, IoResultToTcli};
use crate::game::registry::GameData;
use crate::game::{proc, registry};
use crate::package::index::PackageIndex;
use crate::package::install::tracked::{ConcreteFs, TrackedFs};
use crate::package::install::{self, PackageInstaller};
use crate::package::resolver::DependencyGraph;
use crate::package::{resolver, Package};
use crate::project::manifest::ProjectManifest;
use crate::project::overrides::ProjectOverrides;
use crate::project::state::{StateEntry, StateFile};
use crate::ts::package_manifest::PackageManifestV1;
use crate::ts::package_reference::PackageReference;
use crate::ui::progress;
use crate::{util, TCLI_HOME};

pub mod error;
pub mod lock;
pub mod manifest;
pub mod overrides;
pub mod publish;
pub mod state;

pub enum ProjectKind {
    Dev(ProjectOverrides),
    Profile,
}

pub struct Project {
    pub base_dir: PathBuf,
    pub state_dir: PathBuf,
    pub staging_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub lockfile_path: PathBuf,
    pub game_registry_path: PathBuf,
    pub statefile_path: PathBuf,
}

impl Project {
    /// Open the directory as a project but perform no additional tasks.
    pub fn open_unchecked(project_dir: &Path) -> Self {
        Self {
            base_dir: project_dir.to_path_buf(),
            state_dir: project_dir.join(".tcli/project_state"),
            staging_dir: project_dir.join(".tcli/staging"),
            manifest_path: project_dir.join("Thunderstore.toml"),
            lockfile_path: project_dir.join("Thunderstore.lock"),
            game_registry_path: project_dir.join(".tcli/game_registry.json"),
            statefile_path: project_dir.join(".tcli/state.json"),
        }
    }

    /// Open the directory as a project, performing validation and housekeeping tasks.
    pub fn open(project_dir: &Path) -> Result<Self, Error> {
        // TODO: Validate that the following paths exist.
        let project = Project::open_unchecked(project_dir);
        project.validate()?;
        project.prune_pids()?;

        Ok(project)
    }

    /// Validate that the project's state is correct and in working order.
    pub fn validate(&self) -> Result<(), Error> {
        // A directory without a manifest is *not* a project.
        if !self.manifest_path.is_file() {
            Err(ProjectError::NoProjectFile(
                self.manifest_path.to_path_buf(),
            ))?;
        }

        // Everything within .tcli is assumed to be replacable. Therefore we only care
        // about whether or not .tcli itself exists.
        let dotdir = self.base_dir.join(".tcli");
        if !dotdir.is_dir() {
            fs::create_dir(dotdir)?;
        }

        Ok(())
    }

    /// Prune pid files that refer to processes that no longer exist.
    pub fn prune_pids(&self) -> Result<(), Error> {
        let pid_files = proc::get_pid_files(&self.base_dir.join(".tcli"))?;
        let pid_files = pid_files
            .iter()
            .filter_map(|x| fs::read_to_string(x).map(|inner| (x, inner)).ok())
            .filter(|(_, x)| {
                let as_usize = x.parse::<usize>().unwrap();
                !proc::is_running(as_usize)
            })
            .map(|(path, _)| path);

        // Delete each invalid PID.
        for pid_file in pid_files {
            fs::remove_file(pid_file)?;
        }

        Ok(())
    }

    /// Create a new project within the given directory.
    pub fn create_new(
        project_dir: &Path,
        overwrite: bool,
        project_kind: ProjectKind,
    ) -> Result<Project, Error> {
        if project_dir.is_file() {
            Err(IoError::DirectoryIsFile(project_dir.into()))?;
        }

        if !project_dir.is_dir() {
            fs::create_dir(project_dir).map_fs_error(project_dir)?;
        }

        let manifest = match &project_kind {
            ProjectKind::Dev(overrides) => {
                let mut manifest = ProjectManifest::default_dev_project();
                manifest.apply_overrides(overrides.clone())?;
                manifest
            }
            ProjectKind::Profile => ProjectManifest::default_profile_project(),
        };

        let mut options = File::options();
        options.write(true);
        if overwrite {
            options.create(true);
        } else {
            options.create_new(true);
        }

        let manifest_path = project_dir.join("Thunderstore.toml");
        let mut manifest_file = match options.open(&manifest_path) {
            Ok(x) => Ok(x),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                Err(ProjectError::ProjectAlreadyExists(manifest_path.clone()))
            }
            Err(e) => Err(IoError::Native(e, Some(manifest_path.to_path_buf())))?,
        }?;

        write!(
            manifest_file,
            "{}",
            toml::to_string_pretty(&manifest).unwrap()
        )?;

        let project_state = project_dir.join(".tcli/project_state");
        fs::create_dir_all(&project_state)?;

        let staging_dir = project_dir.join(".tcli/staging");
        fs::create_dir_all(&staging_dir)?;

        let statefile_path = project_dir.join(".tcli/state.json");
        fs::write(
            statefile_path,
            serde_json::to_string_pretty(&StateFile::default())?,
        )?;

        let project = Project {
            base_dir: project_dir.to_path_buf(),
            state_dir: project_state,
            staging_dir,
            manifest_path,
            lockfile_path: project_dir.join("Thunderstore.lock"),
            game_registry_path: project_dir.join(".tcli/game_registry.json"),
            statefile_path: project_dir.join(".tcli/state.json"),
        };

        // Stop here if all we need is a profile.
        if matches!(project_kind, ProjectKind::Profile) {
            return Ok(project);
        }

        let package = manifest.package.as_ref().unwrap();

        let icon_path = project_dir.join("icon.png");
        match File::options()
            .write(true)
            .create_new(true)
            .open(&icon_path)
        {
            Ok(mut f) => f
                .write_all(include_bytes!("../../resources/icon.png"))
                .unwrap(),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => {}
            Err(e) => Err(IoError::Native(e, Some(icon_path)))?,
        }

        let readme_path = project_dir.join("README.md");
        match File::options()
            .write(true)
            .create_new(true)
            .open(&readme_path)
        {
            Ok(mut f) => write!(
                f,
                include_str!("../../resources/readme_template.md"),
                package.namespace, package.name, package.description
            )?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => Err(IoError::Native(e, Some(readme_path)))?,
        }

        let dist_dir = project.base_dir.join("dist");
        if !dist_dir.exists() {
            fs::create_dir(dist_dir)?;
        }

        Ok(project)
    }

    pub fn add_game_data(&self, game_data: GameData) -> Result<(), Error> {
        registry::write_data(&self.game_registry_path, game_data)
    }

    /// Add one or more packages to this project.
    ///
    /// Note: This function does not COMMIT the packages, it only adds them to the project manifest.
    pub fn add_packages(&self, packages: &[PackageReference]) -> Result<(), Error> {
        let mut manifest = ProjectManifest::read_from_file(&self.manifest_path)?;
        let mut manifest_deps = manifest.dependencies.dependencies.clone();

        // Merge the manifest's dependencies with the given packages.
        // The rule here is:
        // 1. Add if the package does not exist within the manifest.
        // 2. Replace with given version if manifest.version < given.version.
        let manifest_index = manifest_deps
            .iter()
            .enumerate()
            .map(|(index, x)| (x.to_loose_ident_string(), index))
            .collect::<HashMap<_, _>>();

        for package in packages.iter() {
            match manifest_index.get(&package.to_loose_ident_string()) {
                Some(x) if manifest_deps[*x].version < package.version => {
                    manifest_deps[*x] = package.clone();
                }

                None => {
                    manifest_deps.push(package.clone());
                }

                _ => (),
            }
        }

        manifest.dependencies.dependencies = manifest_deps;
        manifest.write_to_file(&self.manifest_path)?;

        Ok(())
    }

    /// Remove one or more packages from this project.
    ///
    /// Similar to add_packages, this function does not commit changes to the project.
    pub fn remove_packages(&self, packages: &[PackageReference]) -> Result<(), Error> {
        let mut manifest = ProjectManifest::read_from_file(&self.manifest_path)?;
        let manifest_deps = &mut manifest.dependencies.dependencies;

        for package in packages {
            let remove_index = manifest_deps.iter().position(|x| x == package);

            if let Some(x) = remove_index {
                manifest_deps.remove(x);
            } else {
                println!("Project manifest does not include package '{package}', skipping.");
            }
        }

        manifest.write_to_file(&self.manifest_path)
    }

    async fn install_packages(
        &self,
        statefile: &mut StateFile,
        packages: Vec<&PackageReference>,
        all_resolved: &[PackageReference],
    ) -> Result<(), Error> {
        // Determine the modloader using the full resolved package list.
        let modloader = install::guess_modloader(all_resolved)
            .await
            .expect("Could not determine modloader. Ensure a modloader package is in your dependencies.");
        let modloader_packages = install::get_modloader_packages(all_resolved).await;

        let packages = packages
            .into_iter()
            .map(|x| async move { Package::from_any(x).await });

        let sem = Arc::new(Semaphore::new(5));

        let jobs = packages.into_iter().map(|package| {
            let modloader = modloader.clone();
            let modloader_packages = modloader_packages.clone();
            let sem = sem.clone();
            
            async move {
                let _permit = sem.acquire().await.unwrap();
                let package = package.await?;
                let pkg_id = package.identifier.to_string();
                let is_modloader = modloader_packages.contains(&package.identifier.to_loose_ident_string());

                progress::scope_start_child(&pkg_id, "install", &package.identifier.name);

                // Resolve the package, either downloading it or returning its cached path.
                progress::scope_progress(&pkg_id, 0, Some("resolving"));
                let package_dir = match package.get_path().await {
                    Some(x) => x,
                    None => package.download().await?,
                };

                let mut installer = install::get_installer(&modloader, ConcreteFs::new(StateEntry::default()));

                progress::scope_progress(&pkg_id, 0, Some("installing"));
                let install_result = installer
                    .install_package(
                        &package,
                        &package_dir,
                        &self.state_dir,
                        &self.staging_dir,
                        is_modloader,
                    )
                    .await;

                // On success, extract the tracked state and return it with the package id.
                match install_result {
                    Ok(_) => {
                        progress::scope_complete(&pkg_id);
                        let state = installer.extract_state();
                        Ok((package.identifier, state))
                    }
                    Err(e) => {
                        progress::scope_fail(&pkg_id, e.to_string());
                        Err(e)
                    }
                }
            }
        });

        let results = try_join_all(jobs).await?;

        // Merge tracked files into the statefile.
        for (package_id, state_entry) in results {
            // Merge the new state with any existing state for this package.
            let existing = statefile.entry(package_id);
            existing.staged.extend(state_entry.staged);
            existing.linked.extend(state_entry.linked);
        }

        Ok(())
    }

    async fn uninstall_packages(
        &self,
        statefile: &mut StateFile,
        packages: Vec<&PackageReference>,
    ) -> Result<(), Error> {
        let packages = try_join_all(
            packages
                .into_iter()
                .map(|x| async move { Package::from_any(x).await }),
        )
        .await?;

        // For each package to uninstall:
        // 1. Remove all staged files that were copied to game directories
        // 2. Remove all linked files from the state directory
        // 3. Remove the package's entry from the statefile
        for package in packages {
            let pkg_id = package.identifier.to_string();
            progress::scope_start_child(&pkg_id, "uninstall", &package.identifier.name);

            let Some(entry) = statefile.get(&package.identifier) else {
                progress::scope_complete(&pkg_id);
                continue;
            };

            // Remove staged file destinations (files copied to game dir at launch)
            for staged in &entry.staged {
                for dest in &staged.dest {
                    // Only remove if the file still matches what we installed
                    if let Ok(true) = staged.is_same_as(dest) {
                        let _ = fs::remove_file(dest);
                    }
                }
                // Remove the source file in staging dir
                let _ = fs::remove_file(&staged.file.path);
            }

            // Remove linked files from state dir
            for linked in &entry.linked {
                let _ = fs::remove_file(&linked.path);
            }

            // Remove package from statefile
            statefile.remove(&package.identifier);
            progress::scope_complete(&pkg_id);
        }

        // Cleanup empty directories in the state and staging dirs.
        util::file::remove_empty_dirs(&self.state_dir, false)?;
        util::file::remove_empty_dirs(&self.staging_dir, false)?;

        Ok(())
    }

    /// Commit changes made to the project manifest to the project.
    pub async fn commit(&self, sync: bool) -> Result<(), Error> {
        if sync {
            progress::scope_start("sync", "Syncing package index");
            PackageIndex::sync(&TCLI_HOME).await?;
            progress::scope_complete("sync");
        }

        let lockfile = LockFile::open_or_new(&self.lockfile_path)?;
        let lockfile_graph = DependencyGraph::from_graph(lockfile.package_graph);

        let manifest = ProjectManifest::read_from_file(&self.manifest_path)?;
        let package_graph = resolver::resolve_packages(manifest.dependencies.dependencies).await?;

        // Get the full list of resolved packages for modloader detection.
        let all_resolved: Vec<_> = package_graph.digest().into_iter().cloned().collect();
        
        let delta = lockfile_graph.graph_delta(&package_graph);

        progress::info(format!(
            "{} packages to install, {} to remove",
            delta.add.len(),
            delta.del.len()
        ));

        let mut statefile = StateFile::open_or_new(&self.statefile_path)?;

        let packages_to_remove = delta.del.iter().rev().collect::<Vec<_>>();
        let packages_to_add = delta.add.iter().rev().collect::<Vec<_>>();

        if !packages_to_remove.is_empty() {
            progress::scope_start("uninstall", "Removing packages");
            self.uninstall_packages(&mut statefile, packages_to_remove).await?;
            progress::scope_complete("uninstall");
        }

        if !packages_to_add.is_empty() {
            progress::scope_start("install", "Installing packages");
            self.install_packages(&mut statefile, packages_to_add, &all_resolved).await?;
            progress::scope_complete("install");
        }

        // Write the statefile with changes made during unins
        statefile.write(&self.statefile_path)?;

        LockFile::open_or_new(&self.lockfile_path)?
            .with_graph(package_graph)
            .commit()?;

        Ok(())
    }

    pub async fn start_game(
        &self,
        game_id: &str,
        _mods_enabled: bool,
        _args: Vec<String>,
    ) -> Result<(), Error> {
        let game_data = registry::get_game_data(&self.game_registry_path, game_id)
            .ok_or_else(|| ProjectError::InvalidGameId(game_id.to_string()))?;
        let game_dist = game_data.active_distribution;
        let game_dir = &game_dist.game_dir;

        // Copy the contents of staging into the game directory.
        let mut statefile = StateFile::open_or_new(&self.statefile_path)?;
        let staged_files = statefile.state.values_mut().flat_map(|x| &mut x.staged);

        for file in staged_files {
            let rel = file.file.path.strip_prefix(&self.staging_dir).unwrap();
            let dest = game_dir.join(rel);

            if file.is_same_as(&dest)? {
                continue;
            }

            let dest_parent = dest.parent().unwrap();
            if !dest_parent.is_dir() {
                fs::create_dir_all(dest_parent)?;
            }

            fs::copy(&file.file.path, &dest)?;
            file.dest.push(dest);
        }

        statefile.write(&self.statefile_path)?;

        // let installer = Installer::override_new();
        // let pid = installer
        //     .start_game(
        //         mods_enabled,
        //         &self.state_dir,
        //         &game_dist.game_dir,
        //         &game_dist.exe_path,
        //         args,
        //     )
        //     .await?;

        // // The PID file is contained within the state dir and is of name `game.exe.pid`.
        // let pid_path = self
        //     .base_dir
        //     .join(".tcli")
        //     .join(format!("{}.pid", game_data.identifier));

        // let mut pid_file = File::create(pid_path)?;
        // pid_file.write_all(format!("{}", pid).as_bytes())?;

        // println!(
        //     "{} has been started with PID {}.",
        //     game_data.display_name.green(),
        //     pid
        // );

        Ok(())
    }

    pub fn stop_game(&self, game_id: &str) -> Result<(), Error> {
        let game_data = registry::get_game_data(&self.game_registry_path, game_id)
            .ok_or_else(|| ProjectError::InvalidGameId(game_id.to_string()))?;

        let mut pid_file = self.base_dir.join(".tcli").join(game_data.identifier);
        pid_file.set_extension("pid");

        if !pid_file.is_file() {
            Err(IoError::FileNotFound(pid_file.clone()))?;
        }

        let pid = fs::read_to_string(&pid_file)?.parse::<usize>().unwrap();

        proc::kill(pid);
        fs::remove_file(pid_file)?;

        Ok(())
    }

    pub fn build(&self, overrides: ProjectOverrides) -> Result<PathBuf, Error> {
        let mut manifest = self.get_manifest()?;
        manifest.apply_overrides(overrides)?;

        let project_dir = manifest
            .project_dir
            .as_deref()
            .expect("Project should be loaded from a file to build");

        let package = manifest
            .package
            .as_ref()
            .ok_or(ProjectError::MissingTable("package"))?;

        let build = manifest
            .build
            .as_ref()
            .ok_or(ProjectError::MissingTable("build"))?;

        let output_dir = project_dir.join(&build.outdir);
        match fs::create_dir_all(&output_dir) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(IoError::Native(e, Some(output_dir.clone()))),
        }?;

        let output_path = output_dir.join(format!(
            "{}-{}-{}.zip",
            package.namespace, package.name, package.version
        ));

        let mut zip = zip::ZipWriter::new(
            File::options()
                .create(true)
                .write(true)
                .open(&output_path)
                .map_fs_error(&output_path)?,
        );

        for copy in &build.copy {
            let source_path = project_dir.join(&copy.source);

            // first elem is always the root, even when the path given is to a file
            for file in walkdir::WalkDir::new(&source_path).follow_links(true) {
                let file = file.map_err(IoError::DirWalker)?;

                let inner_path = file
                    .path()
                    .strip_prefix(&source_path)
                    .expect("Path was made by walking source, but was not rooted in source?");

                if file.file_type().is_dir() {
                    zip.add_directory(
                        copy.target.join(inner_path).to_string_lossy(),
                        SimpleFileOptions::default(),
                    )?;
                } else if file.file_type().is_file() {
                    zip.start_file(
                        copy.target.join(inner_path).to_string_lossy(),
                        SimpleFileOptions::default(),
                    )?;
                    std::io::copy(
                        &mut File::open(file.path()).map_fs_error(file.path())?,
                        &mut zip,
                    )?;
                } else {
                    unreachable!("paths should always be either a file or a dir")
                }
            }
        }

        zip.start_file("manifest.json", SimpleFileOptions::default())?;
        write!(
            zip,
            "{}",
            serde_json::to_string_pretty(&PackageManifestV1::from_manifest(
                package.clone(),
                manifest.dependencies.dependencies.clone()
            ))
            .unwrap()
        )?;

        let icon_path = project_dir.join(&build.icon);
        zip.start_file("icon.png", SimpleFileOptions::default())?;
        std::io::copy(
            &mut File::open(&icon_path).map_fs_error(icon_path)?,
            &mut zip,
        )?;

        let readme_path = project_dir.join(&build.readme);
        zip.start_file("README.md", SimpleFileOptions::default())?;
        write!(
            zip,
            "{}",
            fs::read_to_string(&readme_path).map_fs_error(readme_path)?
        )?;

        zip.finish()?;

        Ok(output_path)
    }

    pub fn get_manifest(&self) -> Result<ProjectManifest, Error> {
        ProjectManifest::read_from_file(&self.manifest_path)
    }

    pub fn get_lockfile(&self) -> Result<LockFile, Error> {
        LockFile::open_or_new(&self.lockfile_path)
    }
}
