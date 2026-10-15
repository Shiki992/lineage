// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Generates a tiny solid-colour PNG for the image fixture, so no binary image
//! file has to be committed or licensed.

use crate::checksum::{adler32, crc32};

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    let len = u32::try_from(data.len()).expect("PNG chunk under 4 GiB");
    out.extend_from_slice(&len.to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// A `size` × `size` RGB PNG filled with `rgb`. The image data uses one stored
/// (uncompressed) deflate block, so `size` must keep it under 64 KiB.
pub fn solid_square(size: u32, rgb: [u8; 3]) -> Vec<u8> {
    let side = usize::try_from(size).expect("size fits usize");
    // Each scanline: filter type 0 (none), then the pixels.
    let mut row = vec![0];
    row.extend(rgb.repeat(side));
    let raw = row.repeat(side);
    let len = u16::try_from(raw.len()).expect("image data fits one stored deflate block");

    let mut zlib = vec![0x78, 0x01]; // zlib header: deflate, no preset dictionary
    zlib.push(0x01); // final block, stored
    zlib.extend_from_slice(&len.to_le_bytes());
    zlib.extend_from_slice(&(!len).to_le_bytes());
    zlib.extend_from_slice(&raw);
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&size.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, RGB, deflate, no filter, no interlace

    let mut png = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &zlib);
    chunk(&mut png, b"IEND", &[]);
    png
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_png_signature_and_end_chunk() {
        let png = solid_square(4, [0x0E, 0x7C, 0x7B]);
        assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
        assert!(png.ends_with(&[0xAE, 0x42, 0x60, 0x82])); // CRC of an empty IEND chunk
    }
}
