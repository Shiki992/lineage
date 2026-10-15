// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Writing zip archives. Sizes and checksums go in the local header (no data
//! descriptors), the way Word writes its own files. Unchanged entries are copied
//! as their original compressed bytes. The output depends only on the entries,
//! so the same package always gives the same bytes.

use super::Entry;
use crate::error::{Error, Result};

/// 1980-01-01, the earliest MS-DOS date, for entries with no date of their own.
pub(super) const DOS_DATE_1980: u16 = (1 << 5) | 1;

const STORE: u16 = 0;
const DEFLATE: u16 = 8;
/// Bits 1 and 2 (deflate options) and 11 (UTF-8 names) are kept; bit 3 (data
/// descriptor) and everything else is cleared, because we write the sizes up front.
const KEPT_FLAGS: u16 = 0b0000_1000_0000_0110;
const UTF8_FLAG: u16 = 1 << 11;
const VERSION: u16 = 20;
const DEFLATE_LEVEL: u8 = 6;

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn fit_u32(value: usize) -> Result<u32> {
    u32::try_from(value).map_err(|_| Error::TooLarge("package over 4 GiB".to_owned()))
}

fn fit_u16(value: usize) -> Result<u16> {
    u16::try_from(value).map_err(|_| Error::TooLarge("too many entries or a long name".to_owned()))
}

struct Prepared<'a> {
    method: u16,
    flags: u16,
    crc: u32,
    raw: std::borrow::Cow<'a, [u8]>,
    size: u32,
}

fn prepare(entry: &Entry) -> Result<Prepared<'_>> {
    let utf8 = if entry.name.is_ascii() { 0 } else { UTF8_FLAG };
    if let Some(stored) = &entry.stored {
        return Ok(Prepared {
            method: stored.method,
            flags: (stored.flags & KEPT_FLAGS) | utf8,
            crc: stored.crc,
            raw: std::borrow::Cow::Borrowed(&stored.raw),
            size: fit_u32(entry.data.len())?,
        });
    }
    let size = fit_u32(entry.data.len())?;
    let crc = rawzip::crc32(&entry.data);
    if entry.is_dir || entry.data.is_empty() {
        return Ok(Prepared {
            method: STORE,
            flags: utf8,
            crc,
            raw: std::borrow::Cow::Borrowed(&entry.data),
            size,
        });
    }
    let deflated = miniz_oxide::deflate::compress_to_vec(&entry.data, DEFLATE_LEVEL);
    if deflated.len() < entry.data.len() {
        Ok(Prepared {
            method: DEFLATE,
            flags: utf8,
            crc,
            raw: std::borrow::Cow::Owned(deflated),
            size,
        })
    } else {
        Ok(Prepared {
            method: STORE,
            flags: utf8,
            crc,
            raw: std::borrow::Cow::Borrowed(&entry.data),
            size,
        })
    }
}

pub(super) fn write_entries(entries: &[Entry]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for entry in entries {
        let prepared = prepare(entry)?;
        let offset = fit_u32(out.len())?;
        let compressed = fit_u32(prepared.raw.len())?;
        let name_len = fit_u16(entry.name.len())?;

        put_u32(&mut out, 0x0403_4B50);
        put_u16(&mut out, VERSION);
        put_u16(&mut out, prepared.flags);
        put_u16(&mut out, prepared.method);
        put_u16(&mut out, entry.dos_time);
        put_u16(&mut out, entry.dos_date);
        put_u32(&mut out, prepared.crc);
        put_u32(&mut out, compressed);
        put_u32(&mut out, prepared.size);
        put_u16(&mut out, name_len);
        put_u16(&mut out, 0); // extra field length
        out.extend_from_slice(entry.name.as_bytes());
        out.extend_from_slice(&prepared.raw);

        put_u32(&mut central, 0x0201_4B50);
        put_u16(&mut central, VERSION); // made by: MS-DOS, spec 2.0
        put_u16(&mut central, VERSION); // needed to extract
        put_u16(&mut central, prepared.flags);
        put_u16(&mut central, prepared.method);
        put_u16(&mut central, entry.dos_time);
        put_u16(&mut central, entry.dos_date);
        put_u32(&mut central, prepared.crc);
        put_u32(&mut central, compressed);
        put_u32(&mut central, prepared.size);
        put_u16(&mut central, name_len);
        put_u16(&mut central, 0); // extra field length
        put_u16(&mut central, 0); // comment length
        put_u16(&mut central, 0); // disk number
        put_u16(&mut central, 0); // internal attributes
        put_u32(&mut central, if entry.is_dir { 0x10 } else { 0 }); // MS-DOS directory bit
        put_u32(&mut central, offset);
        central.extend_from_slice(entry.name.as_bytes());
    }

    let count = fit_u16(entries.len())?;
    let central_offset = fit_u32(out.len())?;
    let central_size = fit_u32(central.len())?;
    out.extend_from_slice(&central);

    put_u32(&mut out, 0x0605_4B50);
    put_u16(&mut out, 0); // this disk
    put_u16(&mut out, 0); // disk with the central directory
    put_u16(&mut out, count);
    put_u16(&mut out, count);
    put_u32(&mut out, central_size);
    put_u32(&mut out, central_offset);
    put_u16(&mut out, 0); // comment length
    Ok(out)
}
