// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Blobs the task pane has fetched from storage, kept in the engine's memory so
//! commands can read them without a round trip across the boundary.
//!
//! Storage is asynchronous and commands are not, so a command that needs a blob
//! the cache lacks fails with the list of what it missed, the host fetches those
//! and runs it again. When the missing blob is one part of a snapshot, the list
//! holds every missing part of that snapshot, so a snapshot costs one round.

use std::cell::RefCell;
use std::collections::{BTreeMap, VecDeque};

use lineage::blob::{BlobHash, BlobSource, manifest_parts};

/// Blobs by hash, verified on the way in, oldest dropped past a byte limit.
#[derive(Debug)]
pub struct BlobCache {
    blobs: BTreeMap<BlobHash, Vec<u8>>,
    order: VecDeque<BlobHash>,
    bytes: usize,
    limit: usize,
    /// Each cached manifest's parts.
    manifests: BTreeMap<BlobHash, Vec<BlobHash>>,
}

/// 64 MiB: far more than a family of resumes needs, small enough for a task pane.
pub const DEFAULT_LIMIT: usize = 64 * 1024 * 1024;

impl BlobCache {
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            blobs: BTreeMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            limit,
            manifests: BTreeMap::new(),
        }
    }

    /// Adds bytes fetched from storage under the hash they were stored as.
    ///
    /// # Errors
    /// When the bytes don't hash to `hash`: storage gave back something else.
    pub fn add(&mut self, hash: &BlobHash, bytes: Vec<u8>) -> Result<(), String> {
        if &BlobHash::of(&bytes) != hash {
            return Err(format!("blob {} does not match its bytes", hash.as_str()));
        }
        self.insert(hash.clone(), bytes);
        Ok(())
    }

    /// Adds a blob the engine itself just made (its hash is computed, not claimed).
    pub fn insert(&mut self, hash: BlobHash, bytes: Vec<u8>) {
        if self.blobs.contains_key(&hash) {
            return;
        }
        if bytes.first() == Some(&b'{')
            && let Some(parts) = manifest_parts(&bytes)
        {
            self.manifests.insert(hash.clone(), parts);
        }
        self.bytes += bytes.len();
        self.blobs.insert(hash.clone(), bytes);
        self.order.push_back(hash);
        while self.bytes > self.limit && self.order.len() > 1 {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(dropped) = self.blobs.remove(&oldest) {
                self.bytes -= dropped.len();
            }
            self.manifests.remove(&oldest);
        }
    }

    #[must_use]
    pub fn contains(&self, hash: &BlobHash) -> bool {
        self.blobs.contains_key(hash)
    }

    #[must_use]
    pub fn get(&self, hash: &BlobHash) -> Option<&[u8]> {
        self.blobs.get(hash).map(Vec::as_slice)
    }

    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// `missing`, plus every other uncached part of any cached snapshot that
    /// one of them belongs to. In first-seen order, without repeats.
    #[must_use]
    pub fn expand(&self, missing: &[BlobHash]) -> Vec<BlobHash> {
        let mut out: Vec<BlobHash> = Vec::new();
        let push = |hash: &BlobHash, out: &mut Vec<BlobHash>| {
            if !self.contains(hash) && !out.contains(hash) {
                out.push(hash.clone());
            }
        };
        for hash in missing {
            push(hash, &mut out);
            for parts in self.manifests.values().filter(|p| p.contains(hash)) {
                for part in parts {
                    push(part, &mut out);
                }
            }
        }
        out
    }
}

/// The cache as a [`BlobSource`] that remembers what it didn't have.
pub struct Recording<'a> {
    pub cache: &'a BlobCache,
    missed: RefCell<Vec<BlobHash>>,
}

impl<'a> Recording<'a> {
    #[must_use]
    pub fn new(cache: &'a BlobCache) -> Self {
        Self {
            cache,
            missed: RefCell::new(Vec::new()),
        }
    }

    /// What was asked for and not there, expanded to whole snapshots.
    #[must_use]
    pub fn missed(&self) -> Vec<BlobHash> {
        self.cache.expand(&self.missed.borrow())
    }
}

impl BlobSource for Recording<'_> {
    fn blob(&self, hash: &BlobHash) -> Option<Vec<u8>> {
        let found = self.cache.get(hash).map(<[u8]>::to_vec);
        if found.is_none() {
            let mut missed = self.missed.borrow_mut();
            if !missed.contains(hash) {
                missed.push(hash.clone());
            }
        }
        found
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineage::blob::snapshot;
    use lineage_testkit::fixtures;

    #[test]
    fn bytes_must_match_their_hash() {
        let mut cache = BlobCache::new(DEFAULT_LIMIT);
        let hash = BlobHash::of(b"hello");
        assert!(cache.add(&hash, b"hello".to_vec()).is_ok());
        assert!(cache.add(&BlobHash::of(b"x"), b"y".to_vec()).is_err());
        assert_eq!(cache.get(&hash), Some(&b"hello"[..]));
    }

    #[test]
    fn the_oldest_blobs_go_first_past_the_limit() {
        let mut cache = BlobCache::new(10);
        for text in ["aaaa", "bbbb", "cccc"] {
            cache.insert(BlobHash::of(text.as_bytes()), text.as_bytes().to_vec());
        }
        assert!(!cache.contains(&BlobHash::of(b"aaaa")));
        assert!(cache.contains(&BlobHash::of(b"cccc")));
        assert!(cache.bytes() <= 10);
        let mut tiny = BlobCache::new(1);
        tiny.insert(BlobHash::of(b"big"), b"big".to_vec());
        assert!(
            tiny.contains(&BlobHash::of(b"big")),
            "the newest blob stays"
        );
    }

    #[test]
    fn a_missing_part_brings_its_whole_snapshot() {
        let bytes = fixtures::all()[0].1.to_bytes();
        let (manifest, blobs) = snapshot(&bytes).expect("snapshots");
        let mut cache = BlobCache::new(DEFAULT_LIMIT);
        let manifest_blob = blobs.iter().find(|b| b.hash == manifest).expect("manifest");
        cache
            .add(&manifest, manifest_blob.bytes.clone())
            .expect("adds");
        let parts: Vec<BlobHash> = blobs
            .iter()
            .filter(|b| b.hash != manifest)
            .map(|b| b.hash.clone())
            .collect();
        // One part is already there.
        cache
            .add(&parts[0], blobs[0].bytes.clone())
            .expect("adds the first part");
        let source = Recording::new(&cache);
        assert!(source.blob(&parts[1]).is_none());
        let missed = source.missed();
        assert_eq!(missed.len(), parts.len() - 1);
        assert!(!missed.contains(&parts[0]));
        assert_eq!(missed[0], parts[1], "what was asked for comes first");
    }
}
