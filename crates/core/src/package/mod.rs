// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Office packages: the zip container and the Open Packaging Conventions on top.
//!
//! A [`Package`] keeps every entry in archive order. Entries that are never
//! changed keep their original compressed bytes, so writing a package back
//! reproduces them exactly (see docs/decisions/0005-docx-package-and-xml.md).

pub mod chunks;
pub mod edit;
mod opc;
pub mod props;
mod read;
mod write;

pub use opc::{
    ContentTypes, Relationship, Relationships, part_dir, relative_target, rels_part_for,
    resolve_target,
};
pub use read::{MAX_ENTRIES, MAX_PART_BYTES, MAX_TOTAL_BYTES};

use crate::error::Result;

/// One zip entry as read, kept while its content is unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Stored {
    method: u16,
    flags: u16,
    crc: u32,
    raw: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    name: String,
    data: Vec<u8>,
    is_dir: bool,
    dos_time: u16,
    dos_date: u16,
    stored: Option<Stored>,
}

/// A zip-based Office package, with every part held in memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    entries: Vec<Entry>,
}

impl Package {
    /// Reads a package. The bytes are untrusted: sizes, names, checksums and
    /// compression methods are all checked, and limits apply.
    ///
    /// # Errors
    /// Returns [`crate::Error`] when the bytes are not a readable package.
    pub fn read(bytes: &[u8]) -> Result<Self> {
        Ok(Self {
            entries: read::read_entries(bytes)?,
        })
    }

    /// A package from parts, in this order. Used to rebuild a stored snapshot;
    /// every part is compressed afresh when written.
    #[must_use]
    pub fn from_parts(parts: Vec<(String, Vec<u8>)>) -> Self {
        let mut package = Self {
            entries: Vec::new(),
        };
        for (name, data) in parts {
            package.set_part(&name, data);
        }
        package
    }

    /// Every part as (name, bytes), in archive order.
    #[must_use]
    pub fn parts(&self) -> Vec<(String, Vec<u8>)> {
        self.entries
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| (entry.name.clone(), entry.data.clone()))
            .collect()
    }

    /// Writes the package. Unchanged entries keep their original compressed
    /// bytes; changed and new entries are compressed afresh.
    ///
    /// # Errors
    /// Returns [`crate::Error::TooLarge`] if the result would not fit a plain zip.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        write::write_entries(&self.entries)
    }

    /// Names of every part (directory entries excluded), in archive order.
    pub fn part_names(&self) -> impl Iterator<Item = &str> {
        self.entries
            .iter()
            .filter(|entry| !entry.is_dir)
            .map(|entry| entry.name.as_str())
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        let name = name.trim_start_matches('/');
        self.entries
            .iter()
            .position(|entry| !entry.is_dir && entry.name == name)
            .or_else(|| {
                // Part names are case-insensitive in OPC. Duplicates differing
                // only by case are rejected on read, so this match is unique.
                self.entries
                    .iter()
                    .position(|entry| !entry.is_dir && entry.name.eq_ignore_ascii_case(name))
            })
    }

    /// The stored name of a part, matched case-insensitively.
    #[must_use]
    pub fn canonical_name(&self, name: &str) -> Option<&str> {
        self.index_of(name)
            .map(|index| self.entries[index].name.as_str())
    }

    /// The uncompressed bytes of a part.
    #[must_use]
    pub fn part(&self, name: &str) -> Option<&[u8]> {
        self.index_of(name)
            .map(|index| self.entries[index].data.as_slice())
    }

    /// True when the package has this part.
    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.index_of(name).is_some()
    }

    /// Replaces a part's content, or adds the part at the end. Setting the same
    /// bytes again is a no-op, so the original compressed entry is kept.
    pub fn set_part(&mut self, name: &str, data: Vec<u8>) {
        match self.index_of(name) {
            Some(index) => {
                let entry = &mut self.entries[index];
                if entry.data != data {
                    entry.data = data;
                    entry.stored = None;
                }
            }
            None => self.entries.push(Entry {
                name: name.trim_start_matches('/').to_owned(),
                data,
                is_dir: false,
                dos_time: 0,
                dos_date: write::DOS_DATE_1980,
                stored: None,
            }),
        }
    }

    /// Removes a part. Returns false when there was no such part.
    pub fn remove_part(&mut self, name: &str) -> bool {
        match self.index_of(name) {
            Some(index) => {
                self.entries.remove(index);
                true
            }
            None => false,
        }
    }

    /// True when the part still has the exact bytes it was read with.
    #[must_use]
    pub fn is_unchanged(&self, name: &str) -> bool {
        self.index_of(name)
            .is_some_and(|index| self.entries[index].stored.is_some())
    }

    /// The compression method and compressed bytes of a part as read, if it is
    /// unchanged. Used by fidelity tests.
    #[must_use]
    pub fn original_entry(&self, name: &str) -> Option<(u16, &[u8])> {
        let entry = &self.entries[self.index_of(name)?];
        entry
            .stored
            .as_ref()
            .map(|stored| (stored.method, stored.raw.as_slice()))
    }
}

#[cfg(test)]
mod tests;
