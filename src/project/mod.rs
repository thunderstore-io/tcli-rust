use std::borrow::Borrow;
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};

use colored::Colorize;
use futures::future::try_join_all;
pub use publish::publish;
use zip::write::FileOptions;

use self::lock::LockFile;
use crate::error::{Error, IoResultToTcli};
use crate::game::registry::GameData;
use crate::game::{proc, registry};
use crate::package::install::api::TrackedFile;
use crate::package::install::Installer;
use crate::package::resolver::DependencyGraph;
use crate::package::{resolver, Package};
use crate::project::manifest::ProjectManifest;
use crate::project::overrides::ProjectOverrides;
use crate::project::state::{StagedFile, StateFile};
use crate::ts::package_manifest::PackageManifestV1;
use crate::ts::package_reference::PackageReference;
use crate::ui::reporter::{Progress, Reporter};
use crate::util;

pub mod lock;
pub mod manifest;
pub mod overrides;
mod publish;
mod state;

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
    pub fn open(project_dir: &Path) -> Result<Self, Error> {
        // TODO: Validate that the following paths exist.
        let project_dir = project_dir.canonicalize()?;
        let project = Project {
            base_dir: project_dir.to_path_buf(),
            state_dir: project_dir.join(".tcli/project_state"),
            staging_dir: project_dir.join(".tcli/staging"),
            manifest_path: project_dir.join("Thunderstore.toml"),
            lockfile_path: project_dir.join("Thunderstore.lock"),
            game_registry_path: project_dir.join(".tcli/game_registry.json"),
            statefile_path: project_dir.join(".tcli/state.json"),
        }; 

        let pid_files = proc::get_pid_files(&project_dir.join(".tcli"))?;
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

        Ok(project)
    }

    /// Create a new project within the given directory.
    pub fn create_new(
        project_dir: &Path,
        overwrite: bool,
        project_kind: ProjectKind,
    ) -> Result<Project, Error> {
        if project_dir.is_file() {
            return Err(Error::ProjectDirIsFile(project_dir.into()));
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
                Err(Error::ProjectAlreadyExists(manifest_path.clone()))
            }
            Err(e) => Err(Error::FileIoError(manifest_path.to_path_buf(), e)),
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
            Err(e) => Err(Error::FileIoError(icon_path, e))?,
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
            Err(e) => return Err(Error::FileIoError(readme_path, e)),
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
        installer: &Installer,
        statefile: &mut StateFile,
        packages: Vec<&PackageReference>,
        multi: &dyn Progress,
    ) -> Result<(), Error> {
        let packages = try_join_all(
            packages
                .into_iter()
                .map(|x| async move { Package::resolve_new(x).await }),
        )
        .await?;

        let jobs = packages.into_iter().map(|package| async {
            let bar = multi.add_bar();
            let bar = bar.as_ref();

            // Resolve the package, either downloading it or returning its cached path.
            let package_dir = package.resolve(bar).await?;
            let tracked_files = installer
                .install_package(
                    &package,
                    &package_dir,
                    &self.state_dir,
                    &self.staging_dir,
                    bar,
                )
                .await;

            let finished_msg = match tracked_files {
                Ok(_) => format!(
                    "{} Installed {}-{} {}",
                    "[✓]".green(),
                    package.identifier.namespace.bold(),
                    package.identifier.name.bold(),
                    package.identifier.version.to_string().truecolor(90, 90, 90)
                ),
                Err(ref e) => format!(
                    "{} Error {}-{} {}\n\t{}",
                    "[x]".red(),
                    package.identifier.namespace.bold(),
                    package.identifier.name.bold(),
                    package.identifier.version.to_string().truecolor(90, 90, 90),
                    e,
                ),
            };

            bar.println(&finished_msg);
            bar.finish_and_clear();

            tracked_files.map(|x| (package.identifier, x))
        });

        let tracked_files = try_join_all(jobs)
            .await?
            .into_iter()
            .collect::<Vec<(PackageReference, Vec<TrackedFile>)>>();

        // Iterate through each installed mod, separate tracked files into "link" and "stage" variants.
        // TODO: Make this less hacky, we shouldn't be relying on path ops to determine this.
        for (package, tracked_files) in tracked_files {
            let staged_files = tracked_files
                .iter()
                .filter(|x| x.path.starts_with(&self.staging_dir))
                .map(|x| StagedFile::new(x.clone()))
                .collect::<Result<Vec<_>, _>>()?;

            let linked_files = tracked_files
                .into_iter()
                .filter(|x| x.path.starts_with(&self.state_dir));

            let group = statefile.state.entry(package).or_default();
            group.staged.extend(staged_files);
            group.linked.extend(linked_files);
        }

        Ok(())
    }

    async fn uninstall_packages(
        &self,
        installer: &Installer,
        statefile: &mut StateFile,
        packages: Vec<&PackageReference>,
        multi: &dyn Progress,
    ) -> Result<(), Error> {
        let packages = try_join_all(
            packages
                .into_iter()
                .map(|x| async move { Package::resolve_new(x).await }),
        )
        .await?;

        // Uninstall each package in parallel.
        try_join_all(packages.iter().map(|package| async {
            let bar = multi.add_bar();
            let bar = bar.as_ref();

            let package_dir = package.resolve(bar).await?;
            let state_entry = statefile.state.get(&package.identifier);

            let tracked_files = state_entry
                .map_or(vec![], |x| x.staged.clone())
                .into_iter()
                .map(|x| x.action)
                .chain(state_entry.map_or(vec![], |x| x.linked.clone()))
                .collect::<Vec<_>>();

            installer
                .uninstall_package(
                    package,
                    &package_dir,
                    &self.state_dir,
                    &self.staging_dir,
                    tracked_files,
                    bar,
                )
                .await
        }))
        .await?;

        // Run post-uninstall cleanup / validation ops.
        // 1. Invalidate staged files, removing them from the statefile if they no longer exist.
        // 2. Cleanup empty directories within staging and state.
        // 3. Remove uninstalled / invalidated entries from the statefile.
        for package in packages {
            let entry = statefile.state.get(&package.identifier).unwrap();
            let staged = &entry.staged;

            // Determine the list of entries that will be invalidated.
            let invalid_staged_entries = staged
                .iter()
                .filter(|x| !x.action.path.is_file());

            for staged_entry in invalid_staged_entries {
                // Each dest is checked if it (a) exists and (b) is the same as orig.
                let dests_to_remove = staged_entry
                    .dest
                    .iter()
                    .filter_map(|path| match staged_entry.is_same_as(path) {
                        Ok(x) if x => Some(Ok(path)),
                        Ok(_) => None,
                        Err(e) => Some(Err(e)),
                    });

                for dest in dests_to_remove {
                    fs::remove_file(dest?)?;
                }
            }

            statefile.state.remove(&package.identifier);
        }

        // Cleanup empty directories in the state and staging dirs.
        util::file::remove_empty_dirs(&self.state_dir, false)?;
        util::file::remove_empty_dirs(&self.staging_dir, false)?;

        Ok(())
    }

    /// Commit changes made to the project manifest to the project.
    pub async fn commit(&self, reporter: Box<dyn Reporter>) -> Result<(), Error> {
        let lockfile = LockFile::open_or_new(&self.lockfile_path)?;
        let lockfile_graph = DependencyGraph::from_graph(lockfile.package_graph);

        let manifest = ProjectManifest::read_from_file(&self.manifest_path)?;
        let package_graph = resolver::resolve_packages(manifest.dependencies.dependencies).await?;

        // Compare the lockfile and new graphs to determine the
        let delta = lockfile_graph.graph_delta(&package_graph);

        println!(
            "{} packages will be installed, {} will be removed.",
            delta.add.len(),
            delta.del.len()
        );

        let installer = Installer::override_new();
        let mut statefile = StateFile::open_or_new(&self.statefile_path)?;

        let multi = reporter.create_progress();

        let packages_to_remove = delta.del.iter().rev().collect::<Vec<_>>();
        let packages_to_add = delta.add.iter().rev().collect::<Vec<_>>();

        self.uninstall_packages(
            &installer,
            &mut statefile,
            packages_to_remove,
            multi.borrow(),
        )
        .await?;

        self.install_packages(&installer, &mut statefile, packages_to_add, multi.borrow())
            .await?;

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
        mods_enabled: bool,
        args: Vec<String>,
    ) -> Result<(), Error> {
        let game_data = registry::get_game_data(&self.game_registry_path, game_id)
            .ok_or_else(|| Error::InvalidGameId(game_id.to_string()))?;
        let game_dist = game_data.active_distribution;
        let game_dir = &game_dist.game_dir;

        // Copy the contents of staging into the game directory.
        let mut statefile = StateFile::open_or_new(&self.statefile_path)?;
        let staged_files = statefile.state.values_mut().flat_map(|x| &mut x.staged);

        for file in staged_files {
            let rel = file.action.path.strip_prefix(&self.staging_dir).unwrap();
            let dest = game_dir.join(rel);

            if file.is_same_as(&dest)? {
                continue;
            }

            let dest_parent = dest.parent().unwrap();
            if !dest_parent.is_dir() {
                fs::create_dir_all(dest_parent)?;
            }

            fs::copy(&file.action.path, &dest)?;
            file.dest.push(dest);
        }

        statefile.write(&self.statefile_path)?;

        let installer = Installer::override_new();
        let pid = installer
            .start_game(
                mods_enabled,
                &self.state_dir,
                &game_dist.game_dir,
                &game_dist.exe_path,
                args,
            )
            .await?;

        // The PID file is contained within the state dir and is of name `game.exe.pid`.
        let pid_path = self
            .base_dir
            .join(".tcli")
            .join(format!("{}.pid", game_data.identifier));

        let mut pid_file = File::create(pid_path)?;
        pid_file.write_all(format!("{}", pid).as_bytes())?;

        println!(
            "{} has been started with PID {}.",
            game_data.display_name.green(),
            pid
        );

        Ok(())
    }

    pub fn stop_game(&self, game_id: &str) -> Result<(), Error> {
        let game_data = registry::get_game_data(&self.game_registry_path, game_id)
            .ok_or_else(|| Error::BadGameId(game_id.to_string()))?;

        let mut pid_file = self.base_dir.join(".tcli").join(game_data.identifier);
        pid_file.set_extension("pid");

        if !pid_file.is_file() {
            Err(Error::FileNotFound(pid_file.clone()))?;
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
            .ok_or(Error::MissingTable("package"))?;

        let build = manifest
            .build
            .as_ref()
            .ok_or(Error::MissingTable("build"))?;

        let output_dir = project_dir.join(&build.outdir);
        match fs::create_dir_all(&output_dir) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(Error::FileIoError(output_dir.clone(), e)),
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
                let file = file?;

                let inner_path = file
                    .path()
                    .strip_prefix(&source_path)
                    .expect("Path was made by walking source, but was not rooted in source?");

                if file.file_type().is_dir() {
                    zip.add_directory(
                        copy.target.join(inner_path).to_string_lossy(),
                        FileOptions::default(),
                    )?;
                } else if file.file_type().is_file() {
                    zip.start_file(
                        copy.target.join(inner_path).to_string_lossy(),
                        FileOptions::default(),
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

        zip.start_file("manifest.json", FileOptions::default())?;
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
        zip.start_file("icon.png", FileOptions::default())?;
        std::io::copy(
            &mut File::open(&icon_path).map_fs_error(icon_path)?,
            &mut zip,
        )?;

        let readme_path = project_dir.join(&build.readme);
        zip.start_file("README.md", FileOptions::default())?;
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

