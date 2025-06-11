use std::borrow::Borrow;
use std::collections::HashMap;
use std::fs::{self, File};
use std::mem;
use std::path::Path;
use std::str::FromStr;
use std::sync::Mutex;

use chrono::NaiveDateTime;
use futures_util::StreamExt;
use log::warn;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;

use crate::error::{Error, IoError};
use crate::ts::experimental;
use crate::ts::experimental::index::PackageIndexEntry;
use crate::ts::package_reference::PackageReference;
use crate::ts::version::Version;
use crate::util::file;

// Memory cache for the package index. We set this when we initially load the index lookup table
// so we don't need to query the disk, however we must also be able to update it when necessary.
static INDEX_CACHE: OnceCell<Mutex<PackageIndex>> = OnceCell::new();

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
    update_time: NaiveDateTime,
    lookup: Vec<LookupTableEntry>,

    strict_lookup: HashMap<String, usize>,
    loose_lookup: HashMap<String, Vec<usize>>,

    index_file: File,
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
    len: usize,
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
            Err(IoError::DirNotFound(tcli_home.into()))?;
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
            }
            .unwrap();

            let entry = LookupTableEntry {
                start,
                len: chunk.len(),
            };

            lookup.insert(pkg_ref, entry);

            // Increment the starting index by the byte length of the chunk.
            start += chunk.len();

            index_out.write_all(chunk.as_bytes()).await?;
        }

        let header_path = index_dir.join("header.json");
        let header = IndexHeader {
            update_time: experimental::index::get_index_update_time().await?,
        };
        fs::write(header_path, serde_json::to_string_pretty(&header)?)?;

        let lookup_path = index_dir.join("lookup.json");
        fs::write(lookup_path, serde_json::to_string_pretty(&lookup)?)?;

        Ok(())
    }

    /// Open and serialize the on-disk index, retrieving a fresh copy if it doesn't already exist.
    pub async fn open(tcli_home: &Path) -> Result<&Mutex<PackageIndex>, Error> {
        // Sync the index before we open it if it's in an invalid state.
        if !is_index_valid(tcli_home) {
            PackageIndex::sync(tcli_home).await?;
        }

        // Maintain a cached version of the index so subsequent calls don't trigger a complete reload.
        if let Some(index) = INDEX_CACHE.get() {
            return Ok(&index);
        }

        let index_dir = tcli_home.join("index");
        let lookup: HashMap<PackageReference, LookupTableEntry> = {
            let contents = fs::read_to_string(index_dir.join("lookup.json"))?;
            serde_json::from_str(&contents)?
        };

        let mut entries = vec![];
        let mut strict = HashMap::new();
        let mut loose: HashMap<String, Vec<usize>> = HashMap::new();

        // There's likely a more "rusty" way to do this, but this is simple and it works.
        // Note that the ordering will not be consistent across reruns.
        for (index, (pkg_ref, entry)) in lookup.into_iter().enumerate() {
            entries.push(entry);
            strict.insert(pkg_ref.to_string(), index);

            let l_ident = pkg_ref.to_loose_ident_string();
            let l_entries = loose.entry(l_ident).or_default();
            l_entries.push(index);
        }

        let index_file = File::open(index_dir.join("index.json"))?;
        let header: IndexHeader = {
            let header = fs::read_to_string(index_dir.join("header.json"))?;
            serde_json::from_str(&header)
        }?;

        let index = PackageIndex {
            update_time: header.update_time,
            lookup: entries,
            loose_lookup: loose,
            strict_lookup: strict,
            index_file,
        };

        // Set or otherwise update the memory cache.
        hydrate_cache(index);

        Ok(&INDEX_CACHE.get().unwrap())
    }

    /// Get a package which matches the given package reference.
    pub fn get_package(
        &self,
        reference: impl Borrow<PackageReference>,
    ) -> Option<PackageIndexEntry> {
        let entry_idx = self.strict_lookup.get(&reference.borrow().to_string())?;
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
            .map(|ref x| serde_json::from_str(x))
            .collect::<Result<Vec<PackageIndexEntry>, _>>();

        if let Err(ref e) = pkgs {
            warn!("An error occurred while deserializing index entries for the identifier '{loose_reference}': {e:?}");
        }

        pkgs.ok()
    }

    fn read_index_string(&self, lt_entry: &LookupTableEntry) -> Result<String, Error> {
        let mut buffer = vec![0_u8; lt_entry.len];
        let read_len = file::read_offset(&self.index_file, &mut buffer[..], lt_entry.start as _)?;
        assert_eq!(lt_entry.len, read_len);

        Ok(String::from_utf8(buffer).unwrap())
    }
}

/// Determine if the index is in a valid state or not.
fn is_index_valid(tcli_home: &Path) -> bool {
    let index_dir = tcli_home.join("index");

    let lookup = index_dir.join("lookup.json");
    let index = index_dir.join("index.json");
    let header = index_dir.join("header.json");

    index_dir.exists() && lookup.exists() && index.exists() && header.exists()
}

/// Determine if the in-memory cache is older than the index on disk.
fn is_cache_expired(tcli_home: &Path) -> bool {
    let index_dir = tcli_home.join("index");
    let Ok(header) = fs::read_to_string(index_dir.join("header.json")) else {
        warn!("Failed to read from index header, invalidating the cache.");
        return true;
    };

    let Ok(header) = serde_json::from_str::<IndexHeader>(&header) else {
        return true;
    };

    if let Some(index) = INDEX_CACHE.get() {
        let index = index.lock().unwrap();
        index.update_time != header.update_time
    } else {
        true
    }
}

/// Init the cache or hydrate it with updated data.
fn hydrate_cache(index: PackageIndex) {
    // If the cache hasn't set, do so and stop.
    if INDEX_CACHE.get().is_none() {
        INDEX_CACHE.get_or_init(|| Mutex::new(index));
        return;
    };

    // Otherwise update the cache.
    let cache = INDEX_CACHE.get().unwrap();
    let _ = mem::replace(&mut *cache.lock().unwrap(), index);
}
