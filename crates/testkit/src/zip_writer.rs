// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! A minimal zip writer for synthetic fixtures. Entries are written in the order
//! given with a fixed timestamp, so the same input always gives the same bytes on
//! every OS. Word, LibreOffice and Google Docs all open these packages.
//!
//! Three layouts: stored (the default), deflated like Word, and "streamed" (deflated
//! with data descriptors and directory entries, like streaming zip writers).
//!
//! This is test tooling only. The engine has its own writer in `lineage-core`, so
//! fixtures never depend on the code they test.

use crate::checksum::crc32;

/// 1980-01-01 00:00:00, the earliest DOS timestamp.
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = (1 << 5) | 1;
const VERSION: u16 = 20;
const DATA_DESCRIPTOR: u16 = 1 << 3;

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

struct Layout {
    deflate: bool,
    descriptors: bool,
    directories: bool,
}

/// Writes `entries` (name, bytes) as a stored zip archive.
pub fn write_stored(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    write(
        entries,
        &Layout {
            deflate: false,
            descriptors: false,
            directories: false,
        },
    )
}

/// Writes `entries` deflated. With `streamed`, also writes data descriptors
/// (sizes after the data) and a directory entry before each folder's first file.
pub fn write_deflated(entries: &[(String, Vec<u8>)], streamed: bool) -> Vec<u8> {
    write(
        entries,
        &Layout {
            deflate: true,
            descriptors: streamed,
            directories: streamed,
        },
    )
}

fn with_directories(entries: &[(String, Vec<u8>)]) -> Vec<(String, Vec<u8>)> {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (name, data) in entries {
        let mut prefix = String::new();
        for segment in name
            .split('/')
            .rev()
            .skip(1)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            prefix.push_str(segment);
            prefix.push('/');
            if !seen.contains(&prefix) {
                seen.push(prefix.clone());
                out.push((prefix.clone(), Vec::new()));
            }
        }
        out.push((name.clone(), data.clone()));
    }
    out
}

fn write(entries: &[(String, Vec<u8>)], layout: &Layout) -> Vec<u8> {
    let owned;
    let entries = if layout.directories {
        owned = with_directories(entries);
        &owned
    } else {
        entries
    };
    let mut out = Vec::new();
    let mut central = Vec::new();

    for (name, data) in entries {
        let is_dir = name.ends_with('/');
        let deflate = layout.deflate && !is_dir;
        let (method, body) = if deflate {
            (8, miniz_oxide::deflate::compress_to_vec(data, 6))
        } else {
            (0, data.clone())
        };
        let flags = if layout.descriptors && !is_dir {
            DATA_DESCRIPTOR
        } else {
            0
        };
        let offset = to_u32(out.len(), "archive");
        let crc = crc32(data);
        let size = to_u32(data.len(), "entry");
        let compressed = to_u32(body.len(), "entry");
        let name_len = to_u16(name.len(), "entry name");
        let in_header = |value: u32| if flags == 0 { value } else { 0 };

        // Local file header.
        put_u32(&mut out, 0x0403_4B50);
        put_u16(&mut out, VERSION);
        put_u16(&mut out, flags);
        put_u16(&mut out, method);
        put_u16(&mut out, DOS_TIME);
        put_u16(&mut out, DOS_DATE);
        put_u32(&mut out, in_header(crc));
        put_u32(&mut out, in_header(compressed));
        put_u32(&mut out, in_header(size));
        put_u16(&mut out, name_len);
        put_u16(&mut out, 0); // extra field length
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&body);
        if flags != 0 {
            put_u32(&mut out, 0x0807_4B50);
            put_u32(&mut out, crc);
            put_u32(&mut out, compressed);
            put_u32(&mut out, size);
        }

        // Central directory record.
        put_u32(&mut central, 0x0201_4B50);
        put_u16(&mut central, VERSION); // made by: MS-DOS, spec 2.0
        put_u16(&mut central, VERSION); // needed to extract
        put_u16(&mut central, flags);
        put_u16(&mut central, method);
        put_u16(&mut central, DOS_TIME);
        put_u16(&mut central, DOS_DATE);
        put_u32(&mut central, crc);
        put_u32(&mut central, compressed);
        put_u32(&mut central, size);
        put_u16(&mut central, name_len);
        put_u16(&mut central, 0); // extra field length
        put_u16(&mut central, 0); // comment length
        put_u16(&mut central, 0); // disk number
        put_u16(&mut central, 0); // internal attributes
        put_u32(&mut central, if is_dir { 0x10 } else { 0 }); // external attributes
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

    fn sample() -> Vec<(String, Vec<u8>)> {
        vec![
            ("a.txt".to_owned(), b"hello".to_vec()),
            ("dir/b.bin".to_owned(), (0..=255).collect()),
            ("dir/sub/c.xml".to_owned(), b"<c/>".repeat(50)),
            ("empty".to_owned(), Vec::new()),
        ]
    }

    fn read_all(bytes: Vec<u8>) -> Vec<(String, Vec<u8>)> {
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("valid zip");
        (0..archive.len())
            .map(|index| {
                let mut file = archive.by_index(index).expect("entry");
                let name = file.name().expect("entry name").to_string();
                let mut read = Vec::new();
                // Reading to the end also verifies the CRC.
                file.read_to_end(&mut read)
                    .expect("entry reads and CRC matches");
                (name, read)
            })
            .collect()
    }

    #[test]
    fn independent_reader_accepts_every_layout() {
        assert_eq!(read_all(write_stored(&sample())), sample());
        assert_eq!(read_all(write_deflated(&sample(), false)), sample());
        let streamed = read_all(write_deflated(&sample(), true));
        let files: Vec<_> = streamed
            .iter()
            .filter(|(name, _)| !name.ends_with('/'))
            .cloned()
            .collect();
        assert_eq!(files, sample());
        let dirs: Vec<_> = streamed
            .iter()
            .filter(|(name, _)| name.ends_with('/'))
            .map(|(name, _)| name.as_str())
            .collect();
        assert_eq!(dirs, ["dir/", "dir/sub/"]);
    }

    #[test]
    fn output_is_deterministic() {
        let entries = vec![("x.xml".to_owned(), b"<x/>".to_vec())];
        assert_eq!(write_stored(&entries), write_stored(&entries));
        assert_eq!(
            write_deflated(&entries, true),
            write_deflated(&entries, true)
        );
    }
}
