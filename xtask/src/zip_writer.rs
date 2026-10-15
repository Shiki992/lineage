// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! A minimal zip writer for synthetic fixtures. Entries are stored (not
//! compressed), written in the order given, with a fixed timestamp, so the same
//! input always gives the same bytes on every OS. Word, LibreOffice and Google Docs
//! all open stored packages.
//!
//! This is test tooling only. The engine's own zip handling is a Phase 1 decision.

use crate::checksum::crc32;

/// 1980-01-01 00:00:00, the earliest DOS timestamp.
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = (1 << 5) | 1;
const VERSION: u16 = 20;

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn to_u16(value: usize, what: &str) -> u16 {
    u16::try_from(value).unwrap_or_else(|_| panic!("{what} too large for a zip fixture"))
}

fn to_u32(value: usize, what: &str) -> u32 {
    u32::try_from(value).unwrap_or_else(|_| panic!("{what} too large for a zip fixture"))
}

/// Writes `entries` (name, bytes) as a stored zip archive.
pub fn write_stored(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();

    for (name, data) in entries {
        let offset = to_u32(out.len(), "archive");
        let crc = crc32(data);
        let size = to_u32(data.len(), "entry");
        let name_len = to_u16(name.len(), "entry name");

        // Local file header.
        put_u32(&mut out, 0x0403_4B50);
        put_u16(&mut out, VERSION);
        put_u16(&mut out, 0); // flags
        put_u16(&mut out, 0); // method: stored
        put_u16(&mut out, DOS_TIME);
        put_u16(&mut out, DOS_DATE);
        put_u32(&mut out, crc);
        put_u32(&mut out, size); // compressed size
        put_u32(&mut out, size); // uncompressed size
        put_u16(&mut out, name_len);
        put_u16(&mut out, 0); // extra field length
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);

        // Central directory record.
        put_u32(&mut central, 0x0201_4B50);
        put_u16(&mut central, VERSION); // made by: MS-DOS, spec 2.0
        put_u16(&mut central, VERSION); // needed to extract
        put_u16(&mut central, 0); // flags
        put_u16(&mut central, 0); // method: stored
        put_u16(&mut central, DOS_TIME);
        put_u16(&mut central, DOS_DATE);
        put_u32(&mut central, crc);
        put_u32(&mut central, size);
        put_u32(&mut central, size);
        put_u16(&mut central, name_len);
        put_u16(&mut central, 0); // extra field length
        put_u16(&mut central, 0); // comment length
        put_u16(&mut central, 0); // disk number
        put_u16(&mut central, 0); // internal attributes
        put_u32(&mut central, 0); // external attributes
        put_u32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }

    let count = to_u16(entries.len(), "entry count");
    let central_offset = to_u32(out.len(), "archive");
    let central_size = to_u32(central.len(), "central directory");
    out.extend_from_slice(&central);

    // End of central directory record.
    put_u32(&mut out, 0x0605_4B50);
    put_u16(&mut out, 0); // this disk
    put_u16(&mut out, 0); // disk with central directory
    put_u16(&mut out, count);
    put_u16(&mut out, count);
    put_u32(&mut out, central_size);
    put_u32(&mut out, central_offset);
    put_u16(&mut out, 0); // comment length
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};

    #[test]
    fn independent_reader_accepts_archive() {
        let entries = vec![
            ("a.txt".to_owned(), b"hello".to_vec()),
            ("dir/b.bin".to_owned(), (0..=255).collect()),
            ("empty".to_owned(), Vec::new()),
        ];
        let bytes = write_stored(&entries);
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("valid zip");
        assert_eq!(archive.len(), entries.len());
        for (index, (name, data)) in entries.iter().enumerate() {
            let mut file = archive.by_index(index).expect("entry");
            assert_eq!(file.name().expect("entry name").as_ref(), name.as_str());
            let mut read = Vec::new();
            // Reading to the end also verifies the CRC.
            file.read_to_end(&mut read)
                .expect("entry reads and CRC matches");
            assert_eq!(&read, data);
        }
    }

    #[test]
    fn output_is_deterministic() {
        let entries = vec![("x.xml".to_owned(), b"<x/>".to_vec())];
        assert_eq!(write_stored(&entries), write_stored(&entries));
    }
}
