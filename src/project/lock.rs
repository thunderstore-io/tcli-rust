use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use md5::digest::FixedOutput;
use md5::Md5;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::package::Package;
use crate::Error;
use crate::package::resolver::{DependencyGraph, InnerDepGraph};

#[derive(Serialize, Deserialize, Debug)]
pub struct LockFile {
    #[serde(skip)]
    path: PathBuf,

    version: u32,
    graph_hash: String,
    pub package_graph: InnerDepGraph,
}

impl LockFile {
    /// Opens and reads or creates a new lockfile instance.
    pub fn open_or_new(path: &Path) -> Result<Self, Error> {
        if path.exists() {
            let contents = fs::read_to_string(path)?;
            let lockfile = serde_json::from_str(&contents).unwrap();

            Ok(LockFile {
                path: path.to_path_buf(),
                ..lockfile
            })
        } else {
            Ok(LockFile {
                path: path.to_path_buf(),
                version: 1,
                graph_hash: String::default(),
                package_graph: InnerDepGraph::default(),
            })
        }
    }

    pub fn with_graph(self, package_graph: DependencyGraph) -> Self {
        let inner_graph = package_graph.into_inner();
        let graph_hash = {
            // Note, this hash is not guaranteed to be stable. This is simply a way for us to determine
            // if the lockfile has been manually modified.
            let graph_str = serde_json::to_string(&inner_graph).unwrap();
            let mut md5 = Md5::default();

            std::io::copy(&mut graph_str.as_bytes(), &mut md5).unwrap();
            format!("{:x}", md5.finalize_fixed())
        };

        LockFile {
            graph_hash,
            package_graph: inner_graph,
            ..self
        }
    }

    /// Writes the lockfile to disk.
    pub fn commit(self) -> Result<(), Error> {
        let mut lockfile = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&self.path)?;

        let new_contents = serde_json::to_string_pretty(&self).unwrap();
        lockfile.write_all(new_contents.as_bytes())?;

        Ok(())
    }
}

pub fn serialize<S: Serializer>(
    packages: &HashMap<String, Package>,
    ser: S,
) -> Result<S::Ok, S::Error> {
    packages.values().collect::<Vec<_>>().serialize(ser)
}

pub fn deserialize<'de, D: Deserializer<'de>>(de: D) -> Result<HashMap<String, Package>, D::Error> {
    Ok(Vec::<Package>::deserialize(de)?
        .into_iter()
        .map(|package| (package.identifier.to_loose_ident_string(), package))
        .collect::<HashMap<_, _>>())
}
