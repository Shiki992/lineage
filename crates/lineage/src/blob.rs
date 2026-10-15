// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Content-addressed blobs. The crate computes SHA-256 names; the host stores
//! the bytes (`Lineage/<familyId>/blobs/<sha256>`).
//!
//! A checkpoint's snapshot is a small manifest listing each package part's
//! blob, so a picture shared by every version is stored once. A sent copy is
//! also stored as one blob of the exact file, so "open what they received" gives
//! back the very bytes that were sent.

use std::collections::BTreeMap;

use lineage_core::Package;
use lineage_core::hash::sha256_hex;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The SHA-256 of a blob, as 64 lower-case hex digits. Hashes name files in
/// storage, so anything else is refused, including on read.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct BlobHash(String);

impl TryFrom<String> for BlobHash {
    type Error = String;

    fn try_from(text: String) -> std::result::Result<Self, String> {
        Self::parse(&text).map_err(|e| e.to_string())
    }
}

impl From<BlobHash> for String {
    fn from(hash: BlobHash) -> Self {
        hash.0
    }
}

impl BlobHash {
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(sha256_hex(bytes))
    }

    /// Checks and wraps a hash read from storage.
    ///
    /// # Errors
    /// When the text is not 64 lower-case hex digits.
    pub fn parse(text: &str) -> Result<Self> {
        if text.len() == 64
            && text
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            Ok(Self(text.to_owned()))
        } else {
            Err(Error::Ledger(format!("not a blob hash: {text:?}")))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A blob to store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blob {
    pub hash: BlobHash,
    pub bytes: Vec<u8>,
}

impl Blob {
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self {
            hash: BlobHash::of(&bytes),
            bytes,
        }
    }
}

/// Read access to stored blobs, provided by the host.
pub trait BlobSource {
    fn blob(&self, hash: &BlobHash) -> Option<Vec<u8>>;
}

/// Blobs in memory: for tests and for blobs a command has just produced.
#[derive(Clone, Debug, Default)]
pub struct MemoryBlobs(pub BTreeMap<BlobHash, Vec<u8>>);

impl MemoryBlobs {
    pub fn add(&mut self, blob: Blob) {
        self.0.insert(blob.hash, blob.bytes);
    }
}

impl BlobSource for MemoryBlobs {
    fn blob(&self, hash: &BlobHash) -> Option<Vec<u8>> {
        self.0.get(hash).cloned()
    }
}

/// Blobs from a source, plus some not yet stored.
pub(crate) struct Layered<'a> {
    pub fresh: &'a [Blob],
    pub stored: &'a dyn BlobSource,
}

impl BlobSource for Layered<'_> {
    fn blob(&self, hash: &BlobHash) -> Option<Vec<u8>> {
        self.fresh
            .iter()
            .find(|b| &b.hash == hash)
            .map(|b| b.bytes.clone())
            .or_else(|| self.stored.blob(hash))
    }
}

/// A checkpoint's snapshot: the parts of the file, in order.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub v: u32,
    pub parts: Vec<ManifestPart>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManifestPart {
    pub name: String,
    pub blob: BlobHash,
}

/// Splits a file into part blobs and a manifest blob. Returns the manifest's
/// hash and every blob to store (the manifest last).
///
/// # Errors
/// When the file is not a readable package.
pub fn snapshot(bytes: &[u8]) -> Result<(BlobHash, Vec<Blob>)> {
    let package = Package::read(bytes)?;
    let mut blobs = Vec::new();
    let mut parts = Vec::new();
    for (name, data) in package.parts() {
        let blob = Blob::new(data);
        parts.push(ManifestPart {
            name,
            blob: blob.hash.clone(),
        });
        if !blobs.iter().any(|b: &Blob| b.hash == blob.hash) {
            blobs.push(blob);
        }
    }
    let manifest = Manifest { v: 1, parts };
    let json = serde_json::to_vec(&manifest).map_err(|e| Error::Ledger(e.to_string()))?;
    let manifest_blob = Blob::new(json);
    let hash = manifest_blob.hash.clone();
    blobs.push(manifest_blob);
    Ok((hash, blobs))
}

/// Rebuilds a file from its snapshot. Every part is identical to the one saved.
///
/// # Errors
/// [`Error::MissingBlob`] when a blob is not stored, or the manifest is damaged.
pub fn restore(manifest: &BlobHash, blobs: &dyn BlobSource) -> Result<Vec<u8>> {
    let json = blobs
        .blob(manifest)
        .ok_or_else(|| Error::MissingBlob(manifest.as_str().to_owned()))?;
    let manifest: Manifest =
        serde_json::from_slice(&json).map_err(|e| Error::Ledger(e.to_string()))?;
    if manifest.v != 1 {
        return Err(Error::Ledger(format!(
            "unknown snapshot version {}",
            manifest.v
        )));
    }
    let mut parts = Vec::with_capacity(manifest.parts.len());
    for part in manifest.parts {
        let data = blobs
            .blob(&part.blob)
            .ok_or_else(|| Error::MissingBlob(part.blob.as_str().to_owned()))?;
        if BlobHash::of(&data) != part.blob {
            return Err(Error::Ledger(format!(
                "blob {} is damaged",
                part.blob.as_str()
            )));
        }
        parts.push((part.name, data));
    }
    Ok(Package::from_parts(parts).to_bytes()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineage_testkit::fixtures;

    #[test]
    fn snapshots_restore_every_part() {
        for (name, fixture) in fixtures::all() {
            let bytes = fixture.to_bytes();
            let (manifest, blobs) = snapshot(&bytes).expect("snapshots");
            let mut store = MemoryBlobs::default();
            for blob in blobs {
                store.add(blob);
            }
            let restored = restore(&manifest, &store).expect("restores");
            let (a, b) = (
                Package::read(&bytes).expect("reads"),
                Package::read(&restored).expect("reads"),
            );
            assert_eq!(a.parts(), b.parts(), "{name}");
        }
    }

    #[test]
    fn shared_parts_are_stored_once() {
        let bytes = fixtures::all()[0].1.to_bytes();
        let (_, first) = snapshot(&bytes).expect("snapshots");
        let (_, second) = snapshot(&bytes).expect("snapshots");
        assert_eq!(first, second, "the same file gives the same blobs");
    }

    #[test]
    fn a_damaged_blob_is_refused() {
        let bytes = fixtures::all()[0].1.to_bytes();
        let (manifest, blobs) = snapshot(&bytes).expect("snapshots");
        let mut store = MemoryBlobs::default();
        for mut blob in blobs {
            if blob.hash != manifest {
                blob.bytes.push(b' ');
            }
            store.0.insert(blob.hash, blob.bytes);
        }
        assert!(restore(&manifest, &store).is_err());
    }

    #[test]
    fn hashes_are_checked() {
        assert!(BlobHash::parse(&"a".repeat(64)).is_ok());
        assert!(BlobHash::parse("../etc/passwd").is_err());
    }
}
