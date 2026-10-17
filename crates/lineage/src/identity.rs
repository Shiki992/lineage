// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Measuring how alike two files are, for recognising a file whose ids were
//! stripped (BLUEPRINT §4, Identity). [`crate::returned`] decides what the
//! measures mean (decision 0013).
//!
//! - The fingerprint, stored with every checkpoint, is a bottom-k sketch
//!   (k = 128) of the hashes of every five-word run of the body text, plus the
//!   sequence of paragraph styles. Two sketches estimate how much text two files
//!   share, as a whole percentage, with no blobs fetched.
//! - [`content_match`] compares two whole documents word by word, through the
//!   engine's "what changed", to tell near-identical candidates apart.

use lineage_core::doc::{Doc, changed_words};
use lineage_core::hash::{Fnv, fnv};
use serde::{Deserialize, Serialize};

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

/// Words as the fingerprint counts them: runs of letters and digits.
fn split_words(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
}

fn words_of(doc: &Doc) -> (Vec<String>, u64) {
    let (lines, structure) = doc.text_and_structure();
    let words = lines
        .iter()
        .flat_map(|line| split_words(line).map(str::to_lowercase))
        .collect();
    (words, structure)
}

/// The fingerprint of a document.
#[must_use]
pub fn fingerprint(doc: &Doc) -> Fingerprint {
    let (words, structure) = words_of(doc);
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

/// How alike two documents' body text is, word by word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    /// `100 × 2 × common / (words in a + words in b)`, 0 to 100.
    pub percent: u8,
    /// Words removed plus words added from `a` to `b`.
    pub changed: usize,
}

/// How alike two documents' body text is. The common words are those the
/// engine's "what changed" from `a` to `b` leaves untouched, so a reworded
/// sentence costs only its changed words and a moved paragraph costs nothing.
/// Formatting is ignored.
#[must_use]
pub fn content_match(a: &Doc, b: &Doc) -> Match {
    let count = |text: &str| split_words(text).count();
    let total = |d: &Doc| {
        d.text_and_structure()
            .0
            .iter()
            .map(|l| count(l))
            .sum::<usize>()
    };
    let (wa, wb) = (total(a), total(b));
    if wa + wb == 0 {
        return Match {
            percent: 100,
            changed: 0,
        };
    }
    let (removed, added) = changed_words(a, b);
    let common = wa.saturating_sub(removed).min(wb.saturating_sub(added));
    Match {
        percent: u8::try_from(200 * common / (wa + wb)).unwrap_or(100),
        changed: removed + added,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineage_testkit::{fixtures, resume};

    fn print(lines: &[resume::Line]) -> Fingerprint {
        fingerprint(&Doc::open(&resume::package("t", lines).to_bytes()).expect("opens"))
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
        let pitch = fingerprint(&Doc::open(&fixtures::all()[2].1.to_bytes()).expect("opens"));
        assert!(
            similarity(&master, &pitch) <= 5,
            "{}",
            similarity(&master, &pitch)
        );
    }

    fn open(lines: &[resume::Line]) -> Doc {
        Doc::open(&resume::package("t", lines).to_bytes()).expect("opens")
    }

    #[test]
    fn content_match_counts_words_left_untouched() {
        let v1 = open(&resume::master_v1_lines());
        assert_eq!(
            content_match(&v1, &v1),
            Match {
                percent: 100,
                changed: 0
            }
        );
        let v2 = open(&resume::master_v2_lines());
        let close = content_match(&v1, &v2);
        assert!((80..100).contains(&close.percent), "{close:?}");
        assert!(close.changed > 10, "the new job's words: {close:?}");
        // A moved section costs nothing.
        assert_eq!(
            content_match(&v1, &open(&resume::reordered_lines())).changed,
            0
        );
        // Formatting doesn't count: v3 also makes the name bold, and adds "Lead".
        let v3 = open(&resume::master_v3_lines());
        let lead = content_match(&v2, &v3);
        assert!((1..=3).contains(&lead.changed), "{lead:?}");
        let pitch = Doc::open(&fixtures::all()[2].1.to_bytes()).expect("opens");
        let far = content_match(&v1, &pitch);
        assert!(far.percent <= 10, "{far:?}");
    }

    #[test]
    fn content_match_tells_neighbouring_checkpoints_apart() {
        // A copy of v2 with one word changed is closer to v2 than to v3, which
        // changed a different paragraph.
        let v2 = open(&resume::master_v2_lines());
        let v3 = open(&resume::master_v3_lines());
        let mut edited = resume::master_v2_lines();
        edited[3] = resume::Line::Plain(
            "Data engineer with seven years of experience building reliable pipelines for retail and logistics teams.".to_owned(),
        );
        let edited = open(&edited);
        assert!(content_match(&v2, &edited).changed < content_match(&v3, &edited).changed);
    }
}
