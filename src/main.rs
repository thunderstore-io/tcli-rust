#![allow(dead_code)]

use std::path::PathBuf;

use clap::Parser;
use cli::InitSubcommand;
use colored::Colorize;
use directories::BaseDirs;
use game::import::GameImporter;
use once_cell::sync::Lazy;
use project::ProjectKind;
use wildmatch::WildMatch;

use crate::cli::{Args, Commands, ListSubcommand};
use crate::config::Vars;
use crate::error::Error;
use crate::game::{ecosystem, registry};
use crate::game::import::{self, ImportBase, ImportOverrides};
use crate::package::resolver::DependencyGraph;
use crate::project::lock::LockFile;
use crate::project::overrides::ProjectOverrides;
use crate::project::Project;
use crate::ui::reporter::IndicatifReporter;

mod cli;
mod config;
mod error;
mod game;
mod package;
mod project;
mod ts;
mod ui;
mod util;

pub static TCLI_HOME: Lazy<PathBuf> = Lazy::new(|| {
    let default_home = BaseDirs::new().unwrap().data_dir().join("tcli");

    Vars::HomeDir
        .into_var()
        .map_or_else(|_| default_home, PathBuf::from)
});

#[tokio::main]
async fn main() -> Result<(), Error> {
    match Args::parse().commands {
        Commands::Init {
            command,
            overwrite,
            project_path,
        } => {
            match command {
                Some(InitSubcommand::Project {
                    package_name,
                    package_namespace,
                    package_version,
                }) => {
                    let overrides = ProjectOverrides::new()
                        .namespace_override(package_namespace)
                        .name_override(package_name)
                        .version_override(package_version);

                    Project::create_new(&project_path, overwrite, ProjectKind::Dev(overrides))?;
                }

                Some(InitSubcommand::Profile) | None => {
                    Project::create_new(&project_path, overwrite, ProjectKind::Profile)?;
                }
            }

            Ok(())
        },
        Commands::Build {
            package_name,
            package_namespace,
            package_version,
            output_dir,
            project_path,
        } => {
            let project = Project::open(&project_path)?;
            let overrides = ProjectOverrides::new()
                .namespace_override(package_namespace)
                .name_override(package_name)
                .version_override(package_version)
                .output_dir_override(output_dir);
            
            project.build(overrides)?;
            Ok(())
        }
        Commands::Publish {
            package_archive,
            mut token,
            package_name,
            package_namespace,
            package_version,
            repository,
            project_path,
        } => {
            token = token.or_else(|| Vars::AuthKey.into_var().ok());
            if token.is_none() {
                return Err(Error::MissingAuthToken);
            }
            
            let project = Project::open(&project_path)?;
            let manifest = project.get_manifest()?;
            
            ts::init_repository(
                manifest
                    .config
                    .repository
                    .as_deref()
                    .ok_or(Error::MissingRepository)?,
                token.as_deref(),
            );

            let archive_path = match package_archive {
                Some(x) if x.is_file() => Ok(x),
                Some(x) => Err(Error::FileNotFound(x)),
                None => {
                    let overrides = ProjectOverrides::new()
                        .namespace_override(package_namespace)
                        .name_override(package_name)
                        .version_override(package_version)
                        .repository_override(repository);
            
                    project.build(overrides)
                }
            }?;

            project::publish(&manifest, archive_path).await
        }
        Commands::Add {
            packages,
            project_path,
        } => {
            ts::init_repository("https://thunderstore.io", None);

            let reporter = Box::new(IndicatifReporter);

            let project = Project::open(&project_path)?;
            project.add_packages(&packages[..])?;
            project.commit(reporter).await?;

            Ok(())
        }
        Commands::Remove {
            packages,
            project_path,
        } => {
            ts::init_repository("https://thunderstore.io", None);
            let reporter = Box::new(IndicatifReporter);

            let project = Project::open(&project_path)?;
            project.remove_packages(&packages[..])?;
            project.commit(reporter).await?;

            Ok(())
        }
        Commands::Import {
            game_id,
            custom_id,
            custom_name,
            platform,
            game_dir,
            steam_dir,
            tcli_directory: _,
            repository: _,
            project_path,
        } => {
            ts::init_repository("https://thunderstore.io", None);

            let project = Project::open(&project_path)?;
            let overrides = ImportOverrides {
                custom_name,
                custom_id,
                custom_exe: None,
                game_dir: game_dir.clone(),
            };
            let import_base = ImportBase::new(&game_id)
                .await?
                .with_overrides(overrides);

            if platform.is_none() {
                let importer = import::select_importer(&import_base)?;
                let game_data = importer.construct(import_base)?;
                return project.add_game_data(game_data);
            }

            // Hacky fix for now
            let platform = platform.unwrap();
            let dists = &import_base.game_def.distributions;
            let ident = dists.iter().find_map(|x| x.ident_from_name(&platform));

            let importer: Box<dyn GameImporter> = match (ident, platform.as_str()) {
                (Some(ident), "steam") => {
                    Box::new(import::steam::SteamImporter::new(ident).with_steam_dir(steam_dir)) as _
                }
                (None, "nodrm") => {
                    Box::new(import::nodrm::NoDrmImporter::new(game_dir.as_ref().unwrap())) as _
                }
                _ => panic!("Manually importing games from '{platform}' is not implemented")
            };
            let game_data = importer.construct(import_base)?;
            let res = project.add_game_data(game_data);
            println!("{} has been imported into the current project", game_id.green());

            res
        }
        
        Commands::Run { 
            game_id, 
            vanilla, 
            args, 
            tcli_directory: _, 
            repository: _, 
            project_path, 
            trailing_args
        } => {
            let project = Project::open(&project_path)?;
            let args = args.unwrap_or(vec![])
                .into_iter()
                .chain(trailing_args.into_iter())
                .collect::<Vec<_>>();
            
            project.start_game(
                &game_id,
                !vanilla,
                args,
            ).await?;

            Ok(())
        }

        Commands::Stop {
            id,
            project_path,
        } => {
            match id.parse::<usize>() {
                Ok(x) => {
                    game::proc::kill(x);
                },
                Err(_) => {
                    let project = Project::open(&project_path)?;
                    project.stop_game(&id)?;
                }
            };
            
            Ok(())
        }
        
        Commands::UpdateSchema {} => {
            ts::init_repository("https://thunderstore.io", None);

            if !ecosystem::schema_exists() {
                let new = ecosystem::get_schema().await?;
                println!(
                    "Downloaded the latest ecosystem schema, version {}",
                    new.schema_version
                );

                return Ok(());
            }

            let current = ecosystem::get_schema().await?;
            ecosystem::remove_schema()?;
            let new = ecosystem::get_schema().await?;

            if current.schema_version == new.schema_version {
                println!(
                    "The local ecosystem schema is the latest, version {}",
                    new.schema_version
                );
            } else {
                println!(
                    "Updated ecosystem schema from version {} to {}",
                    current.schema_version, new.schema_version
                );
            }

            Ok(())
        }
        Commands::List { command } => match command {
            ListSubcommand::Platforms { target, detected: _ } => {
                let platforms = registry::get_supported_platforms(&target);

                println!("TCLI supports the following platforms on {target}");
                for plat in platforms {
                    println!("- {plat}");
                }

                Ok(())
            }
            ListSubcommand::ImportedGames { project_path } => {
                let project = Project::open(&project_path)?;
                let games = registry::get_registry(&project.game_registry_path)?;

                for game in games {
                    println!("{game:#?}");
                }

                Ok(())
            }
            ListSubcommand::SupportedGames { search } => {
                let schema = ecosystem::get_schema().await?;
                let pattern = WildMatch::new(&search);

                let filtered = schema
                    .games
                    .iter()
                    .filter(|(_, game_def)| {
                        pattern.matches(&game_def.meta.display_name)
                            || pattern.matches(&game_def.label)
                    })
                    .collect::<Vec<_>>();

                for (_, game_def) in filtered.iter() {
                    println!("{}", game_def.meta.display_name);
                    println!("- label: {}", game_def.label);
                    println!("- uuid : {}", game_def.uuid);
                }

                let count = filtered.len();
                println!("\n{} games have been listed.", count);

                Ok(())
            }
            ListSubcommand::InstalledMods { project_path } => {
                let project = Project::open(&project_path)?;
                let lock = LockFile::open_or_new(&project.lockfile_path)?;
                let graph = DependencyGraph::from_graph(lock.package_graph);

                println!("Installed packages:");



                for package in graph.digest() {
                    println!(
                        "- {}-{} ({})",
                        package.namespace.bold(),
                        package.name.bold(),
                        package.version.to_string().truecolor(90, 90, 90)
                    );
                }

                Ok(())
            }
        },
    }
}
