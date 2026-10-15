// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Storage behind the same interface the TypeScript adapters implement
//! (`packages/shell/src/storage.ts`): the person's files, and per family a
//! ledger and a blob store.
//!
//! Layout (BLUEPRINT §4):
//!
//! ```text
//! <folder>/<file>.docx
//! <folder>/Lineage/<familyId>/ledger.jsonl
//! <folder>/Lineage/<familyId>/blobs/<sha256>
//! ```
//!
//! Appends are optimistic: the caller passes the ledger version it read (its
//! length in bytes, standing in for an ETag), and the append fails if the ledger
//! has moved on since.

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use lineage::Id;
use lineage::blob::{Blob, BlobHash};

/// A ledger version: its length in bytes when it was read.
pub type Version = u64;

/// Storage errors.
#[derive(Debug)]
pub enum StorageError {
    Io(std::io::Error),
    /// The ledger changed since it was read.
    Conflict,
    /// A blob's bytes do not match its name.
    BadBlob,
    NotFound(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Conflict => write!(f, "the ledger changed since it was read"),
            Self::BadBlob => write!(f, "a blob does not match its hash"),
            Self::NotFound(name) => write!(f, "{name} not found"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            Self::NotFound(e.to_string())
        } else {
            Self::Io(e)
        }
    }
}

/// Where a family's history and the person's files live.
pub trait Storage {
    fn read_file(&self, name: &str) -> Result<Vec<u8>, StorageError>;
    fn write_file(&mut self, name: &str, bytes: &[u8]) -> Result<(), StorageError>;
    fn file_names(&self) -> Result<Vec<String>, StorageError>;
    /// The ledger and its version; an empty ledger has version 0.
    fn read_ledger(&self, family: &Id) -> Result<(Vec<u8>, Version), StorageError>;
    /// Appends lines if the ledger is still at `expected`.
    fn append_ledger(
        &mut self,
        family: &Id,
        expected: Version,
        lines: &[u8],
    ) -> Result<Version, StorageError>;
    /// Stores a blob. Storing the same blob twice is fine.
    fn put_blob(&mut self, family: &Id, blob: &Blob) -> Result<(), StorageError>;
    fn get_blob(&self, family: &Id, hash: &BlobHash) -> Option<Vec<u8>>;
    fn families(&self) -> Result<Vec<Id>, StorageError>;
}

/// Everything in memory.
#[derive(Clone, Debug, Default)]
pub struct MemoryStorage {
    pub files: BTreeMap<String, Vec<u8>>,
    pub ledgers: BTreeMap<Id, Vec<u8>>,
    pub blobs: BTreeMap<(Id, BlobHash), Vec<u8>>,
}

impl Storage for MemoryStorage {
    fn read_file(&self, name: &str) -> Result<Vec<u8>, StorageError> {
        self.files
            .get(name)
            .cloned()
            .ok_or_else(|| StorageError::NotFound(name.to_owned()))
    }

    fn write_file(&mut self, name: &str, bytes: &[u8]) -> Result<(), StorageError> {
        self.files.insert(name.to_owned(), bytes.to_vec());
        Ok(())
    }

    fn file_names(&self) -> Result<Vec<String>, StorageError> {
        Ok(self.files.keys().cloned().collect())
    }

    fn read_ledger(&self, family: &Id) -> Result<(Vec<u8>, Version), StorageError> {
        let bytes = self.ledgers.get(family).cloned().unwrap_or_default();
        let version = bytes.len() as u64;
        Ok((bytes, version))
    }

    fn append_ledger(
        &mut self,
        family: &Id,
        expected: Version,
        lines: &[u8],
    ) -> Result<Version, StorageError> {
        let ledger = self.ledgers.entry(family.clone()).or_default();
        if ledger.len() as u64 != expected {
            return Err(StorageError::Conflict);
        }
        ledger.extend_from_slice(lines);
        Ok(ledger.len() as u64)
    }

    fn put_blob(&mut self, family: &Id, blob: &Blob) -> Result<(), StorageError> {
        if BlobHash::of(&blob.bytes) != blob.hash {
            return Err(StorageError::BadBlob);
        }
        self.blobs
            .entry((family.clone(), blob.hash.clone()))
            .or_insert_with(|| blob.bytes.clone());
        Ok(())
    }

    fn get_blob(&self, family: &Id, hash: &BlobHash) -> Option<Vec<u8>> {
        self.blobs.get(&(family.clone(), hash.clone())).cloned()
    }

    fn families(&self) -> Result<Vec<Id>, StorageError> {
        Ok(self.ledgers.keys().cloned().collect())
    }
}

/// A folder on disk.
#[derive(Clone, Debug)]
pub struct LocalFolderStorage {
    root: PathBuf,
}

impl LocalFolderStorage {
    /// Uses `root`, creating it if needed.
    ///
    /// # Errors
    /// When the folder cannot be created.
    pub fn new(root: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let root = root.into();
        std::fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn family_dir(&self, family: &Id) -> PathBuf {
        // Ids are validated hex, so they are safe as folder names.
        self.root.join("Lineage").join(family.as_str())
    }

    fn file_path(&self, name: &str) -> Result<PathBuf, StorageError> {
        // Only plain file names in the folder: no separators, no "..".
        if name.is_empty()
            || name.contains(['/', '\\'])
            || name == "."
            || name == ".."
            || name.contains(':')
        {
            return Err(StorageError::NotFound(name.to_owned()));
        }
        Ok(self.root.join(name))
    }
}

impl Storage for LocalFolderStorage {
    fn read_file(&self, name: &str) -> Result<Vec<u8>, StorageError> {
        Ok(std::fs::read(self.file_path(name)?)?)
    }

    fn write_file(&mut self, name: &str, bytes: &[u8]) -> Result<(), StorageError> {
        let path = self.file_path(name)?;
        // Write next to it, then rename, so a crash never leaves half a file.
        let temp = path.with_extension("lineage-tmp");
        std::fs::write(&temp, bytes)?;
        std::fs::rename(&temp, &path)?;
        Ok(())
    }

    fn file_names(&self) -> Result<Vec<String>, StorageError> {
        let mut names = Vec::new();
        for entry in std::fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                names.push(entry.file_name().to_string_lossy().into_owned());
            }
        }
        names.sort();
        Ok(names)
    }

    fn read_ledger(&self, family: &Id) -> Result<(Vec<u8>, Version), StorageError> {
        match std::fs::read(self.family_dir(family).join("ledger.jsonl")) {
            Ok(bytes) => {
                let version = bytes.len() as u64;
                Ok((bytes, version))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Vec::new(), 0)),
            Err(e) => Err(e.into()),
        }
    }

    fn append_ledger(
        &mut self,
        family: &Id,
        expected: Version,
        lines: &[u8],
    ) -> Result<Version, StorageError> {
        use std::io::Write;
        let dir = self.family_dir(family);
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("ledger.jsonl");
        let current = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        if current != expected {
            return Err(StorageError::Conflict);
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        file.write_all(lines)?;
        file.sync_all()?;
        Ok(current + lines.len() as u64)
    }

    fn put_blob(&mut self, family: &Id, blob: &Blob) -> Result<(), StorageError> {
        if BlobHash::of(&blob.bytes) != blob.hash {
            return Err(StorageError::BadBlob);
        }
        let dir = self.family_dir(family).join("blobs");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(blob.hash.as_str());
        if path.exists() {
            return Ok(());
        }
        let temp = dir.join(format!("{}.tmp", blob.hash.as_str()));
        std::fs::write(&temp, &blob.bytes)?;
        std::fs::rename(&temp, &path)?;
        Ok(())
    }

    fn get_blob(&self, family: &Id, hash: &BlobHash) -> Option<Vec<u8>> {
        std::fs::read(self.family_dir(family).join("blobs").join(hash.as_str())).ok()
    }

    fn families(&self) -> Result<Vec<Id>, StorageError> {
        let dir = self.root.join("Lineage");
        if !dir.exists() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in std::fs::read_dir(dir)? {
            if let Ok(id) = Id::parse(&entry?.file_name().to_string_lossy()) {
                out.push(id);
            }
        }
        out.sort();
        Ok(out)
    }
}
