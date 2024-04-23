mod cache;
pub mod index;
pub mod install;
pub mod resolver;

use std::borrow::Borrow;
use std::fs::File;
use std::io::{ErrorKind, Read, Seek};
use std::path::{Path, PathBuf};

use colored::Colorize;
use futures::prelude::*;
use serde::{Deserialize, Serialize};
use serde_with::{self, serde_as, DisplayFromStr};
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::error::{Error, IoResultToTcli};
use crate::ts::package_manifest::PackageManifestV1;
use crate::ts::package_reference::PackageReference;
use crate::ts::{self, CLIENT};
use crate::ui::reporter::ProgressBarTrait;
use crate::TCLI_HOME;

use self::index::PackageIndex;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum PackageSource {
    Remote(String),
    Local(PathBuf),
    Cache(PathBuf),
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Package {
    pub source: PackageSource,
    #[serde_as(as = "DisplayFromStr")]
    pub identifier: PackageReference,
    #[serde(with = "crate::ts::package_reference::ser::string_array")]
    pub dependencies: Vec<PackageReference>,
}

impl Package {
    /// Attempt to resolve the package from the local cache or remote. 
    /// This does not download the package, it just finds its "source".
    pub async fn from_any(ident: impl Borrow<PackageReference>) -> Result<Self, Error> {
        if cache::get_cache_location(ident.borrow()).exists() {
            return Package::from_cache(ident).await;
        }

        Package::from_repo(ident).await
    }

    /// Load package metadata by parsing its cached manifest.
    ///
    /// This function will fall back to Package::from_repo if the manifest is invalid.
    pub async fn from_cache(ident: impl Borrow<PackageReference>) -> Result<Self, Error> {
        let ident = ident.borrow();

        let path = cache::get_cache_location(ident);
        let manifest_path = path.join("manifest.json");

        let mut manifest_str = String::new();
        fs::File::open(&manifest_path)
            .await
            .map_fs_error(&manifest_path)
            .unwrap()
            .read_to_string(&mut manifest_str)
            .await
            .unwrap();

        // Remove UTF-8 BOM: https://github.com/serde-rs/serde/issues/1753
        let manifest_str = manifest_str.trim_start_matches('\u{feff}');

        match serde_json::from_str::<PackageManifestV1>(manifest_str) {
            Ok(manifest) => Ok(Package {
                identifier: ident.clone(),
                source: PackageSource::Cache(path.to_path_buf()),
                dependencies: manifest.dependencies,
            }),
            Err(_) => {
                println!(
                    "{} package \"{}\" has a malformed manifest, grabbing info from repo instead",
                    "[!]".bright_yellow(),
                    ident,
                );

                let mut package = Package::from_repo(ident).await?;
                package.source = PackageSource::Cache(path);

                Ok(package)
            }
        }
    }

    /// Load package metadata by querying the repository.
    pub async fn from_repo(ident: impl Borrow<PackageReference>) -> Result<Self, Error> {
        let ident = ident.borrow();

        let index = PackageIndex::open(&TCLI_HOME).await?;
        let package = index.get_package(ident).unwrap();

        Ok(Package {
            identifier: ident.clone(),
            source: PackageSource::Remote(ts::v1::package::download_for_package(ident)),
            dependencies: package.dependencies,
        })
    }

    /// Load package metadata from an arbitrary path, extracting it into the cache if it passes
    // manifest validation.
    pub async fn from_path(ident: PackageReference, path: &Path) -> Result<Self, Error> {
        // let package = Package::from_repo(ident).await?;
        add_to_cache(&ident, File::open(path).map_fs_error(path)?)?;
        let package = Package::from_cache(ident).await?;

        Ok(Package {
            identifier: package.identifier,
            source: PackageSource::Local(path.to_path_buf()),
            dependencies: package.dependencies,
        })
    }

    /// Resolve the package into a discrete path, returning None if it does not exist locally.
    pub async fn get_path(&self) -> Option<PathBuf> {
        match &self.source {
            PackageSource::Local(path) => add_to_cache(
                &self.identifier,
                File::open(path).map_fs_error(path).ok()?,
            ).ok(),
            PackageSource::Cache(path) => Some(path.clone()),
            PackageSource::Remote(_) => None,
        }
    }

    }

    pub async fn download(&self, reporter: &dyn ProgressBarTrait) -> Result<PathBuf, Error> {
        let PackageSource::Remote(package_source) = &self.source else {
            panic!("Invalid use, this is a local package.")
        };

        let output_path = cache::get_cache_location(&self.identifier);

        if output_path.is_dir() {
            reporter.finish();
            return Ok(output_path);
        }

        let download_result = CLIENT.get(package_source).send().await.unwrap();
        let download_size = download_result.content_length().unwrap();

        let progress_message = format!(
            "{}-{} ({})",
            self.identifier.namespace.bold(),
            self.identifier.name.bold(),
            self.identifier.version.to_string().truecolor(90, 90, 90)
        );

        reporter.set_length(download_size);
        reporter.set_message(format!("Downloading {progress_message}..."));

        let mut download_stream = download_result.bytes_stream();

        let mut temp_file = cache::get_temp_zip_file(&self.identifier).await?;
        let zip_file = temp_file.file_mut();

        while let Some(chunk) = download_stream.next().await {
            let chunk = chunk.unwrap();
            zip_file.write_all(&chunk).await.unwrap();

            reporter.inc(chunk.len() as u64);
        }

        reporter.set_message(format!("Extracting {progress_message}..."));

        let cache_path = add_to_cache(&self.identifier, temp_file.into_std().await.file())?;

        // reporter.finish();

        Ok(cache_path)
    }
}

fn add_to_cache(package: &PackageReference, zipfile: impl Read + Seek) -> Result<PathBuf, Error> {
    let output_path = cache::get_cache_location(package);

    match std::fs::remove_dir_all(&output_path) {
        Ok(_) => (),
        Err(e) if e.kind() == ErrorKind::NotFound => (),
        Err(e) => return Err(e).map_fs_error(&output_path),
    };

    std::fs::create_dir_all(&output_path).map_fs_error(&output_path)?;
    zip::read::ZipArchive::new(zipfile)?.extract(&output_path)?;

    Ok(output_path)
}
