// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Splitting a large XML part into chunks for storage (decision 0024).
//!
//! A worksheet of 200,000 rows is about 50 MB of XML, and stored whole it
//! would add that much to every checkpoint. Split at row boundaries chosen
//! by the rows' own content, an edit or an inserted row changes the chunk
//! it falls in (and at most the next), and every other chunk is the same
//! blob as before. Chunks are stored deflated.

use crate::error::{Error, Result};
use crate::hash::Fnv;

/// Parts at least this big are chunked.
pub const CHUNK_PARTS_FROM: usize = 1024 * 1024;
/// No chunk is smaller than this, except the last.
pub const MIN_CHUNK: usize = 256 * 1024;
/// No chunk is bigger than this (a single row can make one bigger).
pub const MAX_CHUNK: usize = 4 * 1024 * 1024;
/// About one cut in this many rows, past the minimum size.
const DIVISOR: u64 = 64;

/// The positions where the XML elements `tag` start (`<row ` or `<row>`,
/// with any prefix: `<x:row `).
fn starts(bytes: &[u8], tag: &[u8]) -> Vec<usize> {
    let mut out = Vec::new();
    let mut i = 0;
    while i + tag.len() + 2 < bytes.len() {
        if bytes[i] == b'<' {
            // Skip a namespace prefix.
            let mut j = i + 1;
            while j < bytes.len() && j < i + 16 && bytes[j].is_ascii_alphanumeric() {
                j += 1;
            }
            let name_start = if j < bytes.len() && bytes[j] == b':' {
                j + 1
            } else {
                i + 1
            };
            if bytes[name_start..].starts_with(tag)
                && matches!(
                    bytes.get(name_start + tag.len()),
                    Some(b' ' | b'>' | b'/' | b'\t' | b'\n' | b'\r')
                )
            {
                out.push(i);
            }
        }
        i += 1;
    }
    out
}

/// Where to cut `bytes` into chunks: positions, increasing, never 0 or the
/// end. Empty for a part too small, or with no rows or strings to cut at.
#[must_use]
pub fn cuts(bytes: &[u8]) -> Vec<usize> {
    if bytes.len() < CHUNK_PARTS_FROM {
        return Vec::new();
    }
    let mut points = starts(bytes, b"row");
    if points.len() < 2 {
        points = starts(bytes, b"si");
    }
    let mut out = Vec::new();
    let mut last = 0usize;
    for pair in points.windows(2) {
        let (prev, here) = (pair[0], pair[1]);
        let size = here - last;
        if size < MIN_CHUNK {
            continue;
        }
        let mut h = Fnv::new();
        h.bytes(&bytes[prev..here]);
        if h.finish().is_multiple_of(DIVISOR) || size >= MAX_CHUNK {
            out.push(here);
            last = here;
        }
    }
    out
}

/// The chunks of `bytes`, by [`cuts`].
#[must_use]
pub fn split(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut from = 0;
    for at in cuts(bytes) {
        out.push(&bytes[from..at]);
        from = at;
    }
    out.push(&bytes[from..]);
    out
}

/// Deflates a chunk for storage.
#[must_use]
pub fn deflate(data: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec(data, 6)
}

/// Inflates a stored chunk, refusing anything that inflates past `limit`
/// bytes (a damaged or hostile blob).
///
/// # Errors
/// [`Error::Damaged`] when the blob isn't deflate data within the limit.
pub fn inflate(stored: &[u8], limit: usize) -> Result<Vec<u8>> {
    miniz_oxide::inflate::decompress_to_vec_with_limit(stored, limit)
        .map_err(|e| Error::Damaged(format!("a stored chunk doesn't inflate: {:?}", e.status)))
}

#[cfg(test)]
mod tests {
    use lineage_testkit::workbooks;

    use super::*;

    fn sheet(rows: u32) -> Vec<u8> {
        let wb = workbooks::large(rows);
        wb.parts()
            .into_iter()
            .find(|(n, _)| n == "xl/worksheets/sheet1.xml")
            .map(|(_, b)| b)
            .expect("sheet")
    }

    #[test]
    fn small_parts_are_one_chunk() {
        assert_eq!(split(b"<worksheet/>").len(), 1);
        assert!(cuts(&sheet(100)).is_empty());
    }

    #[test]
    fn a_large_sheet_is_cut_at_rows_and_rejoins_exactly() {
        let bytes = sheet(30_000);
        let chunks = split(&bytes);
        assert!(chunks.len() > 2, "{}", chunks.len());
        assert_eq!(chunks.concat(), bytes);
        for c in &chunks[1..] {
            assert!(c.starts_with(b"<row "), "cut at a row");
        }
        for c in &chunks[..chunks.len() - 1] {
            assert!(
                c.len() >= MIN_CHUNK && c.len() <= MAX_CHUNK + 4096,
                "{}",
                c.len()
            );
        }
        let stored = deflate(chunks[0]);
        assert!(stored.len() * 4 < chunks[0].len(), "compresses");
        assert_eq!(
            inflate(&stored, chunks[0].len()).expect("inflates"),
            chunks[0]
        );
        assert!(
            inflate(&stored, chunks[0].len() - 1).is_err(),
            "the limit holds"
        );
    }

    #[test]
    fn an_edit_changes_only_nearby_chunks() {
        let mut list = workbooks::orders(30_000);
        let before = workbooks::orders_workbook(&list)
            .parts()
            .into_iter()
            .find(|(n, _)| n == "xl/worksheets/sheet1.xml")
            .expect("sheet")
            .1;
        // An order inserted near the middle.
        let extra = list[15_000].clone();
        list.insert(
            15_000,
            workbooks::Order {
                id: 99_999,
                ..extra
            },
        );
        let after = workbooks::orders_workbook(&list)
            .parts()
            .into_iter()
            .find(|(n, _)| n == "xl/worksheets/sheet1.xml")
            .expect("sheet")
            .1;
        let a: Vec<&[u8]> = split(&before);
        let b: Vec<&[u8]> = split(&after);
        let common = a.iter().filter(|c| b.contains(c)).count();
        // Rows below the insert renumber, so their chunks differ; rows above
        // don't. At least the first third stays the same.
        assert!(common * 3 >= a.len(), "{common} of {} chunks kept", a.len());
    }
}
