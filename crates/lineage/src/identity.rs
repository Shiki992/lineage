// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Recognising a file: first by the two ids in its custom properties, then by
//! a content fingerprint when the ids were stripped (BLUEPRINT §4, Identity).
//!
//! The fingerprint is a bottom-k sketch (k = 128) of the hashes of every
//! five-word run of the body text, plus the sequence of paragraph styles. Two
//! sketches estimate how much text two files share, as a whole percentage. The
//! threshold for "this is the same file" is decided in Phase 3 (BLUEPRINT open
//! decision 3); this module only measures.

use lineage_core::Docx;
use lineage_core::docx::Block;
use lineage_core::docx::props::{LineageIds, read_ids};
use lineage_core::hash::{Fnv, fnv};
use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Size of the sketch.
pub const SKETCH: usize = 128;
/// Words per shingle.
pub const SHINGLE: usize = 5;

/// A content fingerprint.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fingerprint {
    /// The smallest shingle hashes (truncated to 32 bits), sorted.
    pub sketch: Vec<u32>,
    /// Hash of the paragraph style sequence.
    pub structure: u64,
    /// Number of words in the body.
    pub words: u32,
}

fn words_of(docx: &Docx) -> (Vec<String>, u64) {
    let mut words = Vec::new();
    let mut structure = Fnv::new();
    fn walk(blocks: &[Block], words: &mut Vec<String>, structure: &mut Fnv) {
        for block in blocks {
            match block {
                Block::Paragraph(p) => {
                    structure.str(p.props.style.as_deref().unwrap_or(""));
                    words.extend(
                        p.text()
                            .split(|c: char| !c.is_alphanumeric())
                            .filter(|w| !w.is_empty())
                            .map(str::to_lowercase),
                    );
                }
                Block::Table(t) => {
                    structure.str("table");
                    for row in &t.rows {
                        for cell in &row.cells {
                            walk(&cell.blocks, words, structure);
                        }
                    }
                }
                Block::SectionBreak(_) | Block::Opaque(_) => {}
            }
        }
    }
    walk(&docx.body().blocks, &mut words, &mut structure);
    (words, structure.finish())
}

/// The fingerprint of a document.
#[must_use]
pub fn fingerprint(docx: &Docx) -> Fingerprint {
    let (words, structure) = words_of(docx);
    let mut hashes: Vec<u32> = if words.len() < SHINGLE {
        words.iter().map(|w| truncate(fnv(w))).collect()
    } else {
        words
            .windows(SHINGLE)
            .map(|window| {
                let mut h = Fnv::new();
                for w in window {
                    h.str(w);
                }
                truncate(h.finish())
            })
            .collect()
    };
    hashes.sort_unstable();
    hashes.dedup();
    hashes.truncate(SKETCH);
    Fingerprint {
        sketch: hashes,
        structure,
        words: u32::try_from(words.len()).unwrap_or(u32::MAX),
    }
}

fn truncate(h: u64) -> u32 {
    // The high and low halves folded together keep FNV's mixing.
    let folded = (h >> 32) ^ (h & 0xFFFF_FFFF);
    u32::try_from(folded).unwrap_or(u32::MAX)
}

/// How much text two fingerprints share, 0 to 100 (an estimate of the Jaccard
/// similarity of their shingle sets).
#[must_use]
pub fn similarity(a: &Fingerprint, b: &Fingerprint) -> u8 {
    if a.sketch.is_empty() && b.sketch.is_empty() {
        return if a.structure == b.structure { 100 } else { 0 };
    }
    // Bottom-k of the union, then how many of those are in both.
    let mut union: Vec<u32> = a.sketch.iter().chain(&b.sketch).copied().collect();
    union.sort_unstable();
    union.dedup();
    union.truncate(SKETCH);
    let both = union
        .iter()
        .filter(|h| a.sketch.binary_search(h).is_ok() && b.sketch.binary_search(h).is_ok())
        .count();
    u8::try_from(100 * both / union.len().max(1)).unwrap_or(100)
}

/// How a file was recognised.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recognised {
    /// By the ids inside it.
    ByIds(LineageIds),
    /// By content, with the best match and how sure.
    ByContent {
        checkpoint: crate::Id,
        confidence: u8,
    },
    Unknown,
}

/// Recognises a file against stored checkpoints: the ids if present, else the
/// most similar fingerprint.
///
/// # Errors
/// When the file is not a readable Word document.
pub fn recognise<'a>(
    bytes: &[u8],
    checkpoints: impl Iterator<Item = (&'a crate::Id, &'a Fingerprint)>,
) -> Result<Recognised> {
    let docx = Docx::open(bytes)?;
    if let Some(ids) = read_ids(&docx.package)? {
        return Ok(Recognised::ByIds(ids));
    }
    let print = fingerprint(&docx);
    let best = checkpoints
        .map(|(id, other)| (similarity(&print, other), id))
        .max_by_key(|(sim, id)| (*sim, std::cmp::Reverse((*id).clone())));
    Ok(match best {
        Some((confidence, checkpoint)) if confidence > 0 => Recognised::ByContent {
            checkpoint: checkpoint.clone(),
            confidence,
        },
        _ => Recognised::Unknown,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineage_testkit::{fixtures, resume};

    fn print(lines: &[resume::Line]) -> Fingerprint {
        fingerprint(&Docx::open(&resume::package("t", lines).to_bytes()).expect("opens"))
    }

    #[test]
    fn same_text_is_identical() {
        let a = print(&resume::master_v1_lines());
        assert_eq!(similarity(&a, &a), 100);
        assert!(a.sketch.len() <= SKETCH && !a.sketch.is_empty());
    }

    #[test]
    fn versions_are_close_and_strangers_are_far() {
        let master = print(&resume::master_v1_lines());
        let tailored = print(&resume::tailored_data_lines());
        let newer = print(&resume::master_v3_lines());
        assert!(
            similarity(&master, &tailored) >= 40,
            "{}",
            similarity(&master, &tailored)
        );
        assert!(
            similarity(&master, &newer) >= 60,
            "{}",
            similarity(&master, &newer)
        );
        let pitch = fingerprint(&Docx::open(&fixtures::all()[2].1.to_bytes()).expect("opens"));
        assert!(
            similarity(&master, &pitch) <= 5,
            "{}",
            similarity(&master, &pitch)
        );
    }

    #[test]
    fn ids_win_over_content() {
        let mut package = lineage_core::Package::read(
            &resume::package("t", &resume::master_v1_lines()).to_bytes(),
        )
        .expect("reads");
        let ids = LineageIds {
            family: "1".repeat(32),
            checkpoint: "2".repeat(32),
        };
        lineage_core::docx::props::write_ids(&mut package, &ids).expect("writes");
        let bytes = package.to_bytes().expect("bytes");
        assert_eq!(
            recognise(&bytes, std::iter::empty()).expect("reads"),
            Recognised::ByIds(ids)
        );
    }

    #[test]
    fn content_finds_the_closest_checkpoint() {
        let a = crate::Id::parse(&"a".repeat(32)).expect("id");
        let b = crate::Id::parse(&"b".repeat(32)).expect("id");
        let v1 = print(&resume::master_v1_lines());
        let v3 = print(&resume::master_v3_lines());
        let returned = resume::package("t", &resume::master_v3_lines()).to_bytes();
        let found = recognise(&returned, [(&a, &v1), (&b, &v3)].into_iter()).expect("reads");
        assert_eq!(
            found,
            Recognised::ByContent {
                checkpoint: b,
                confidence: 100
            }
        );
    }
}
