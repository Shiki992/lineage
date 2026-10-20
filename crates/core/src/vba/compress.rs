// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The decompression of [MS-OVBA] 2.4.1.3, for the `dir` stream and module
//! source. Lineage never compresses: it doesn't write VBA (decision 0028).
//!
//! Untrusted input: every copy token is checked against what has been
//! decompressed so far in its chunk, chunks may not claim more than the
//! container holds, and the output is capped.
//!
//! [MS-OVBA]: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-ovba/492124cc-5afc-48c8-b439-b42ad7087a7b

use crate::error::{Error, Result};

const CHUNK: usize = 4096;

fn damaged(detail: &str) -> Error {
    Error::Damaged(format!("VBA compression: {detail}"))
}

/// Decompresses a CompressedContainer, giving at most `limit` bytes.
///
/// # Errors
/// [`Error::Damaged`] when the container is malformed, [`Error::TooLarge`]
/// when it decompresses to more than `limit`.
pub fn decompress(container: &[u8], limit: usize) -> Result<Vec<u8>> {
    let Some((&signature, mut rest)) = container.split_first() else {
        return Err(damaged("empty container"));
    };
    if signature != 0x01 {
        return Err(damaged("wrong signature byte"));
    }
    let mut out = Vec::new();
    while !rest.is_empty() {
        if rest.len() < 2 {
            // A single trailing byte can't be a chunk header; some writers
            // pad. Ignore it.
            break;
        }
        let header = u16::from_le_bytes([rest[0], rest[1]]);
        let size = usize::from(header & 0x0FFF) + 3;
        if (header >> 12) & 0b111 != 0b011 {
            return Err(damaged("wrong chunk signature"));
        }
        let compressed = header & 0x8000 != 0;
        let end = size.min(rest.len());
        let data = &rest[2..end];
        let chunk_start = out.len();
        if compressed {
            chunk(data, &mut out, chunk_start, limit)?;
        } else {
            if data.len() < CHUNK {
                return Err(damaged("a raw chunk shorter than 4096 bytes"));
            }
            push_all(&mut out, &data[..CHUNK], limit)?;
        }
        rest = &rest[end..];
    }
    Ok(out)
}

fn push_all(out: &mut Vec<u8>, bytes: &[u8], limit: usize) -> Result<()> {
    if out.len() + bytes.len() > limit {
        return Err(Error::TooLarge("VBA source too large".to_owned()));
    }
    out.extend_from_slice(bytes);
    Ok(())
}

fn chunk(data: &[u8], out: &mut Vec<u8>, start: usize, limit: usize) -> Result<()> {
    let mut at = 0;
    while at < data.len() {
        let flags = data[at];
        at += 1;
        for bit in 0..8 {
            if at >= data.len() {
                break;
            }
            if out.len() - start >= CHUNK {
                return Err(damaged("a chunk decompresses past 4096 bytes"));
            }
            if flags & (1 << bit) == 0 {
                push_all(out, &data[at..=at], limit)?;
                at += 1;
            } else {
                if at + 2 > data.len() {
                    return Err(damaged("a copy token cut short"));
                }
                let token = u16::from_le_bytes([data[at], data[at + 1]]);
                at += 2;
                let difference = out.len() - start;
                let mut bit_count = 4u32;
                while (1usize << bit_count) < difference {
                    bit_count += 1;
                }
                let length_mask = 0xFFFFu16 >> bit_count;
                let offset = usize::from(token >> (16 - bit_count)) + 1;
                let length = usize::from(token & length_mask) + 3;
                if offset > difference {
                    return Err(damaged("a copy token points before its chunk"));
                }
                if out.len() + length > limit {
                    return Err(Error::TooLarge("VBA source too large".to_owned()));
                }
                // Byte by byte: a copy may overlap what it is writing.
                let from = out.len() - offset;
                for k in 0..length {
                    let b = out[from + k];
                    out.push(b);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_examples_decompress() {
        // MS-OVBA 3.2.1 to 3.2.3.
        let none = [
            0x01, 0x19, 0xB0, 0x00, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x00, 0x69,
            0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0x70, 0x00, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76,
            0x2E,
        ];
        assert_eq!(
            decompress(&none, 1 << 20).unwrap(),
            b"abcdefghijklmnopqrstuv."
        );
        let normal = [
            0x01, 0x2F, 0xB0, 0x00, 0x23, 0x61, 0x61, 0x61, 0x62, 0x63, 0x64, 0x65, 0x82, 0x66,
            0x00, 0x70, 0x61, 0x67, 0x68, 0x69, 0x6A, 0x01, 0x38, 0x08, 0x61, 0x6B, 0x6C, 0x00,
            0x20, 0x6D, 0x6E, 0x6F, 0x70, 0x06, 0x71, 0x02, 0x70, 0x04, 0x00, 0x72, 0x73, 0x74,
            0x75, 0x76, 0x10, 0x77, 0x78, 0x79, 0x7A, 0x00, 0x2C,
        ];
        assert_eq!(
            decompress(&normal, 1 << 20).unwrap(),
            b"#aaabcdefaaaaghijaaaaaklaaamnopqaaaaaaaaaaaarstuvwxyzaaa"
        );
        let maximum = [0x01, 0x03, 0xB0, 0x02, 0x61, 0x45, 0x00];
        assert_eq!(decompress(&maximum, 1 << 20).unwrap(), [b'a'; 73]);
    }

    #[test]
    fn a_token_before_its_chunk_is_refused() {
        // One literal, then a copy token claiming an offset of 2 (its top
        // four bits are offset - 1).
        let container = [0x01, 0x04, 0xB0, 0b10, b'a', 0x00, 0x10];
        assert!(matches!(
            decompress(&container, 100),
            Err(Error::Damaged(_))
        ));
    }

    #[test]
    fn the_limit_holds() {
        let text = "x".repeat(10_000);
        let packed = lineage_testkit::vba::compress(text.as_bytes());
        assert!(matches!(
            decompress(&packed, 5_000),
            Err(Error::TooLarge(_))
        ));
        assert_eq!(decompress(&packed, 10_000).unwrap(), text.as_bytes());
    }

    #[test]
    fn a_bad_signature_is_refused() {
        assert!(decompress(&[0x02, 0x00, 0xB0], 10).is_err());
        assert!(decompress(&[], 10).is_err());
    }
}
