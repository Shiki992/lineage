// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Reading zip archives with rawzip, inflating with miniz_oxide. Returned files are
//! untrusted, so every size, name and checksum is checked before it is believed.

use std::collections::BTreeSet;

use rawzip::{CompressionMethod, ZipArchive, ZipVerification};

use super::{Entry, Stored};
use crate::error::{Error, Result};

/// Most entries a package may have.
pub const MAX_ENTRIES: usize = 10_000;
/// Largest uncompressed part, in bytes.
pub const MAX_PART_BYTES: u64 = 128 * 1024 * 1024;
/// Largest total uncompressed size, in bytes.
pub const MAX_TOTAL_BYTES: u64 = 256 * 1024 * 1024;

fn damaged(error: &rawzip::Error) -> Error {
    Error::Damaged(error.to_string())
}

pub(super) fn read_entries(bytes: &[u8]) -> Result<Vec<Entry>> {
    let archive = ZipArchive::from_slice(bytes).map_err(|e| Error::NotAZip(e.to_string()))?;
    if archive.entries_hint() > MAX_ENTRIES as u64 {
        return Err(Error::TooLarge(format!(
            "{} entries",
            archive.entries_hint()
        )));
    }

    let mut entries = Vec::new();
    let mut names = BTreeSet::new();
    let mut ranges = Vec::new();
    let mut total: u64 = 0;

    for record in archive.entries() {
        let record = record.map_err(|e| damaged(&e))?;
        if entries.len() == MAX_ENTRIES {
            return Err(Error::TooLarge("too many entries".to_owned()));
        }
        let flags = record.flags();
        if flags.is_encrypted() || flags.has_strong_encryption() {
            return Err(Error::Encrypted);
        }
        let name = std::str::from_utf8(record.file_path().as_bytes())
            .map_err(|_| Error::Damaged("entry name is not UTF-8".to_owned()))?
            .to_owned();
        if name.is_empty() {
            return Err(Error::Damaged("empty entry name".to_owned()));
        }
        if !names.insert(name.to_ascii_lowercase()) {
            return Err(Error::Damaged("two entries share a name".to_owned()));
        }

        let declared = record.uncompressed_size_hint();
        if declared > MAX_PART_BYTES {
            return Err(Error::TooLarge("a part is too large".to_owned()));
        }
        total += declared;
        if total > MAX_TOTAL_BYTES {
            return Err(Error::TooLarge(
                "the parts are too large in total".to_owned(),
            ));
        }
        let declared = usize::try_from(declared)
            .map_err(|_| Error::TooLarge("a part is too large".to_owned()))?;

        let entry = archive
            .get_entry(record.wayfinder())
            .map_err(|e| damaged(&e))?;
        ranges.push(entry.compressed_data_range());
        let raw = entry.data();
        let method = record.compression_method();
        let data = if method == CompressionMethod::STORE {
            raw.to_vec()
        } else if method == CompressionMethod::DEFLATE {
            // The limit is the declared size: a stream that inflates to more than
            // it claims is damaged (or a zip bomb), never silently truncated.
            miniz_oxide::inflate::decompress_to_vec_with_limit(raw, declared)
                .map_err(|e| Error::Damaged(format!("cannot inflate an entry: {:?}", e.status)))?
        } else {
            return Err(Error::Unsupported(format!(
                "compression method {}",
                method.as_u16()
            )));
        };
        entry
            .claim_verifier()
            .valid(ZipVerification {
                crc: rawzip::crc32(&data),
                uncompressed_size: data.len() as u64,
            })
            .map_err(|e| damaged(&e))?;

        let dos = record.last_modified_dos();
        entries.push(Entry {
            name,
            is_dir: record.is_dir(),
            dos_time: dos.packed_time(),
            dos_date: dos.packed_date(),
            stored: Some(Stored {
                method: method.as_u16(),
                flags: flags.bits(),
                crc: record.crc32(),
                raw: raw.to_vec(),
            }),
            data,
        });
    }

    // Overlapping entries are a classic way to build zip bombs.
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(Error::Damaged("entries overlap".to_owned()));
    }
    Ok(entries)
}
