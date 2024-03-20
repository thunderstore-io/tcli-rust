use std::borrow::Borrow;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Seek};
use std::os::windows::fs::FileExt;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use chrono::NaiveDateTime;
use futures_util::StreamExt;
use log::{warn, debug};
use once_cell::sync::{Lazy, OnceCell};
use serde::{Deserialize, Serialize};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::error::Error;
use crate::ts::experimental;
use crate::ts::experimental::index::PackageIndexEntry;
use crate::ts::package_reference::PackageReference;
use crate::ts::version::Version;
use crate::TCLI_HOME;

static INDEX_PATH: Lazy<PathBuf> = Lazy::new(|| TCLI_HOME.join("package_index.json"));

#[derive(Serialize, Deserialize)]
struct IndexHeader {
    update_time: NaiveDateTime,
}

/// An index which contains packages and optimized methods to query them.
/// 
/// Structurally this refers to three separate files, all contained within TCLI_HOME/index by default.
/// 1. The package header `IndexHeader`. This contains index metadata like last update time, etc.
/// 2. The package lookup table, `IndexLookup`. This is a fast-lookup datastructure which binds
///    package references to start-end byte offsets within the index.
/// 3. The index. This contains a series of newline-delimited json strings, unparsed and unserialized.
#[derive(Debug)]
pub struct PackageIndex {
    lookup: Vec<LookupTableEntry>,

    // Yes, we're continuing this naming scheme. Why? I can't come up with anything better.
    tight_lookup: HashMap<String, usize>,
    loose_lookup: HashMap<String, Vec<usize>>, 

    index_file: File,
}

impl PackageIndex {
    /// Determine if the package index requires an update.
    /// 
    /// An update is requires if any of the following conditions are true:
    /// - Index version is less than the remote version
    /// - Index does not exist
    pub async fn requires_update(tcli_home: &Path) -> Result<bool, Error> {
        let header = tcli_home.join("index").join("header.json");
        if !header.is_file() {
            return Ok(false);
        }

        let header: IndexHeader = {
            let contents = fs::read_to_string(&header)?;
            serde_json::from_str(&contents)?
        };

        let remote_ver = experimental::index::get_index_update_time().await?;

        Ok(header.update_time < remote_ver)
    }

    /// Syncronize the local and remote package index.
    /// 
    /// This will syncronize regardless of local and remote update timestamps.
    /// Use `PackageIndex::requires_update` to determine if an index update is actually required.
    pub async fn sync(tcli_home: &Path) -> Result<(), Error> {
        // Assert internal file structure.
        if !tcli_home.is_dir() {
            Err(Error::DirectoryNotFound(tcli_home.into()))?;
        }

        let index_dir = tcli_home.join("index");
        if !index_dir.is_dir() {
            fs::create_dir(&index_dir)?;
        }

        let index_path = index_dir.join("index.json");
        let mut index_out = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(index_path)
            .await?;

        // The start byte index, of which is tracked in the lookup table.
        let mut lookup: HashMap<PackageReference, LookupTableEntry> = HashMap::new();
        let mut start = 0_usize;

        let mut index_stream = experimental::index::get_index_streamed_raw().await?;
        while let Some(chunk) = index_stream.next().await {
            let chunk = format!("{}\n", chunk?);

            // Convert the package reference from the intern into the global PackageReference type.
            // This is necessary because the third field is "version_number" and not "version"
            // and I don't want to hack around serde to get it working.
            let pkg_ref = {
                let inner: IndexPackageReference = serde_json::from_str(&chunk)?;
                PackageReference::new(
                    inner.namespace,
                    inner.name,
                    Version::from_str(inner.version_number).unwrap(),
                )
            }.unwrap();

            let entry = LookupTableEntry {
                start,
                end: start + chunk.len(),
            };

            lookup.insert(pkg_ref, entry);

            // Increment the starting index by the byte length of the chunk.
            start += chunk.len();

            index_out.write_all(chunk.as_bytes()).await?;
        }
        
        let header_path = index_dir.join("header.json");
        let header = IndexHeader {
            update_time: experimental::index::get_index_update_time().await?
        };
        fs::write(header_path, serde_json::to_string_pretty(&header)?)?;

        let lookup_path = index_dir.join("lookup.json");
        fs::write(lookup_path, serde_json::to_string_pretty(&lookup)?)?;

        Ok(())
    }

    /// Open and serialize the on-disk index, retrieving a fresh copy if it doesn't already exist.
    pub async fn open(tcli_home: &Path) -> Result<&PackageIndex, Error> {
        // Maintain a cached version of the index so subsequent calls don't trigger a complete reload.
        static CACHE: OnceCell<PackageIndex> = OnceCell::new();
        if let Some(index) = CACHE.get() {
            return Ok(index);
        }

        let index_dir = tcli_home.join("index");
        let lookup: HashMap<PackageReference, LookupTableEntry> = {
            let contents = fs::read_to_string(index_dir.join("lookup.json"))?;
            serde_json::from_str(&contents)?
        };

        let mut entries = vec![];
        let mut tight = HashMap::new();
        let mut loose: HashMap<String, Vec<usize>> = HashMap::new();

        // There's likely a more "rusty" way to do this, but this is simple and it works.
        // Note that the ordering will not be consistent across reruns.
        for (index, (pkg_ref, entry)) in lookup.into_iter().enumerate() {
            entries.push(entry);
            tight.insert(pkg_ref.to_string(), index);

            let l_ident = pkg_ref.to_loose_ident_string();
            let l_entries = loose.entry(l_ident).or_default();
            l_entries.push(index);
        }

        let index_file = File::open(index_dir.join("index.json"))?;

        let index = PackageIndex {
            lookup: entries,
            loose_lookup: loose,
            tight_lookup: tight,
            index_file,
        };
        CACHE.set(index).unwrap();

        Ok(CACHE.get().unwrap())
    }

    /// Get a package which matches the given package reference.
    pub fn get_package(&self, reference: impl Borrow<PackageReference>) -> Option<PackageIndexEntry> {
        let entry_idx = self.tight_lookup.get(&reference.borrow().to_string())?;
        let entry = self.lookup.get(*entry_idx)?;

        let index_str = self.read_index_string(entry).ok()?;
        let entry: PackageIndexEntry = serde_json::from_str(&index_str).unwrap();

        Some(entry)
    }

    /// Get one or more packages that match the given loose package reference.
    pub fn get_packages(&self, loose_reference: String) -> Option<Vec<PackageIndexEntry>> {
        let entries = self.loose_lookup.get(&loose_reference)?;
        let pkgs = entries
            .iter()
            .filter_map(|x| self.lookup.get(*x))
            .filter_map(|x| self.read_index_string(x).ok())
            .map(|x| serde_json::from_str(&x))
            .collect::<Result<Vec<PackageIndexEntry>, _>>();

        if let Err(ref e) = pkgs {
            warn!("An error occurred while deserializing index entries for the identifier '{loose_reference}': {e:?}");
        }

        pkgs.ok()
    }

    fn read_index_string(&self, lt_entry: &LookupTableEntry) -> Result<String, Error> {
        let buf_len = lt_entry.end - lt_entry.start;

        let mut buffer = vec![0_u8; buf_len];
        let read_len = self.index_file.seek_read(&mut buffer[..], lt_entry.start as _)?;
        assert_eq!(buf_len, read_len);

        Ok(String::from_utf8(buffer).unwrap())
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct IndexPackageReference<'a> {
    namespace: &'a str,
    name: &'a str,
    version_number: &'a str,
}

#[derive(Serialize, Deserialize, Debug)]
struct LookupTableEntry {
    start: usize,
    end: usize,
}

/// Syncronizes the local TCLI cache with the remote repository.
pub async fn sync_index() -> Result<(), Error> {
    if INDEX_PATH.is_file() {
        fs::remove_file(INDEX_PATH.as_path())?;
    }

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(INDEX_PATH.as_path())
        .await?;

    let mut lookup_table: HashMap<String, LookupTableEntry> = HashMap::new();
    let mut start = 0_usize;

    let mut index_stream = experimental::index::get_index_streamed_raw().await?;
    while let Some(chunk) = index_stream.next().await {
        let mut chunk = chunk?;
        chunk.push('\n');

        let test: IndexPackageReference = serde_json::from_str(&chunk)?;
        let entry = LookupTableEntry {
            start,
            end: start + chunk.len(),
        };
        lookup_table.insert(format!("{}-{}-{}", test.namespace, test.name, test.version_number), entry);
        start += chunk.len();

        out.write_all(chunk.as_bytes()).await?;
    }

    let lookup_file = INDEX_PATH.parent().unwrap().join("lookup.json");
    fs::write(lookup_file, serde_json::to_string_pretty(&lookup_table)?)?;

    let entry = lookup_table.get(&"Keroro1454-Supply_Drop-1.2.3".to_string()).unwrap(); 
    println!("{entry:?}");

    let buf_size = entry.end - entry.start;
    let mut index = File::open(INDEX_PATH.as_path())?;
    let mut buf = vec![0; buf_size];

    index.seek(std::io::SeekFrom::Start(entry.start as _))?;
    index.read_exact(buf.as_mut_slice())?;

    println!("BUF_SIZE: {buf_size}");
    println!("BUF: {buf:?}");
    let line = String::from_utf8(buf).unwrap();
    println!("LINE: {line}");

    panic!("");
    Ok(())
}
