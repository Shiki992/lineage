// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Aligning two block sequences so that moved paragraphs are found as moves.
//!
//! 1. Blocks identical in content and formatting are matched with patience LCS:
//!    blocks unique on both sides anchor the alignment, so a paragraph that moved
//!    does not drag everything around it out of line.
//! 2. Between those matches, blocks with the same content but different
//!    formatting are paired.
//! 3. Blocks identical in content anywhere else are moves.
//! 4. Between the matches, blocks at least 50% similar are paired as edits, and
//!    a lone block left on each side of one gap is paired as a rewrite.
//! 5. What is left is checked for moves that were also edited: at least 85% similar.

use super::keys::{Keys, block_keys, block_kind};
use super::lcs::lcs;
use super::similarity::similarity;
use crate::docx::Block;

/// Least similarity for two blocks in the same place to count as one block edited.
pub const MODIFY_SIMILARITY: u8 = 50;
/// Least similarity for blocks in different places to count as one block moved.
pub const MOVE_SIMILARITY: u8 = 85;
/// Largest similarity table for one gap before pairing falls back to greedy.
const PAIRING_CELLS: usize = 250_000;

/// What became of one block of the earlier sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fate {
    /// Unchanged, at `b` index.
    Same(usize),
    /// Same content, formatting changed.
    Formatted(usize),
    /// Edited in place.
    Modified {
        to: usize,
        similarity: u8,
    },
    /// Moved, maybe also edited.
    Moved {
        to: usize,
        content_same: bool,
        format_same: bool,
        similarity: u8,
    },
    Deleted,
}

impl Fate {
    /// The block in the later sequence, if any.
    #[must_use]
    pub fn target(&self) -> Option<usize> {
        match *self {
            Self::Same(to)
            | Self::Formatted(to)
            | Self::Modified { to, .. }
            | Self::Moved { to, .. } => Some(to),
            Self::Deleted => None,
        }
    }

    /// True when the block stayed in place, so it can anchor insertions.
    #[must_use]
    pub fn stayed(&self) -> bool {
        matches!(
            self,
            Self::Same(_) | Self::Formatted(_) | Self::Modified { .. }
        )
    }

    /// True when the block is exactly as it was, in place.
    #[must_use]
    pub fn unchanged(&self) -> bool {
        matches!(self, Self::Same(_))
    }
}

/// The alignment of sequence `a` (earlier) against `b` (later).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Alignment {
    pub fates: Vec<Fate>,
    /// For each later block, the earlier block it came from.
    pub from: Vec<Option<usize>>,
    pub a_keys: Vec<Keys>,
    pub b_keys: Vec<Keys>,
}

impl Alignment {
    /// Later blocks that are new.
    pub fn inserted(&self) -> impl Iterator<Item = usize> + '_ {
        self.from
            .iter()
            .enumerate()
            .filter(|(_, from)| from.is_none())
            .map(|(j, _)| j)
    }

    /// The earlier block a later block follows: the nearest earlier later-block
    /// that stayed in place. `None` means the start of the story.
    #[must_use]
    pub fn gap_before(&self, j: usize) -> Option<usize> {
        (0..j).rev().find_map(|k| {
            let a = self.from[k]?;
            self.fates[a].stayed().then_some(a)
        })
    }
}

/// Aligns `a` against `b`. `a_src` and `b_src` are the part texts, used to key
/// hyperlinks by their targets.
#[must_use]
pub fn align(a: &[Block], b: &[Block], a_src: Option<&str>, b_src: Option<&str>) -> Alignment {
    let a_keys: Vec<Keys> = a.iter().map(|x| block_keys(x, a_src)).collect();
    let b_keys: Vec<Keys> = b.iter().map(|x| block_keys(x, b_src)).collect();
    let mut fates = vec![Fate::Deleted; a.len()];
    let mut from: Vec<Option<usize>> = vec![None; b.len()];

    let full_a: Vec<(u64, u64)> = a_keys.iter().map(|k| k.full()).collect();
    let full_b: Vec<(u64, u64)> = b_keys.iter().map(|k| k.full()).collect();
    let same = lcs(&full_a, &full_b);
    for &(i, j) in &same {
        fates[i] = Fate::Same(j);
        from[j] = Some(i);
    }

    // The gaps between exact matches.
    let mut starts = vec![(0, 0)];
    starts.extend(same.iter().map(|&(i, j)| (i + 1, j + 1)));
    let mut ends: Vec<(usize, usize)> = same.clone();
    ends.push((a.len(), b.len()));
    let gaps: Vec<((usize, usize), (usize, usize))> = starts.into_iter().zip(ends).collect();

    // Same content, different formatting, in place.
    for &((a_lo, b_lo), (a_hi, b_hi)) in &gaps {
        let gap_a: Vec<usize> = (a_lo..a_hi)
            .filter(|&i| fates[i] == Fate::Deleted)
            .collect();
        let gap_b: Vec<usize> = (b_lo..b_hi).filter(|&j| from[j].is_none()).collect();
        let content_a: Vec<u64> = gap_a.iter().map(|&i| a_keys[i].content).collect();
        let content_b: Vec<u64> = gap_b.iter().map(|&j| b_keys[j].content).collect();
        for (x, y) in lcs(&content_a, &content_b) {
            fates[gap_a[x]] = Fate::Formatted(gap_b[y]);
            from[gap_b[y]] = Some(gap_a[x]);
        }
    }

    // Blocks that moved unchanged (or only reformatted), before edits are paired,
    // so a moved block never stands in for an edited one.
    detect_moves(a, b, &a_keys, &b_keys, &mut fates, &mut from, 0..2);

    // Edited in place, between the blocks that stayed (unchanged or only
    // reformatted), so the pairs found here never cross those: crossing pairs
    // would be a move nobody recorded.
    let mut stayed: Vec<(usize, usize)> = fates
        .iter()
        .enumerate()
        .filter_map(|(i, f)| match f {
            Fate::Same(j) | Fate::Formatted(j) => Some((i, *j)),
            _ => None,
        })
        .collect();
    stayed.sort_unstable();
    let mut starts = vec![(0, 0)];
    starts.extend(stayed.iter().map(|&(i, j)| (i + 1, j + 1)));
    let mut ends = stayed;
    ends.push((a.len(), b.len()));
    let gaps: Vec<((usize, usize), (usize, usize))> = starts.into_iter().zip(ends).collect();
    for &((a_lo, b_lo), (a_hi, b_hi)) in &gaps {
        let rest_a: Vec<usize> = (a_lo..a_hi)
            .filter(|&i| fates[i] == Fate::Deleted)
            .collect();
        let rest_b: Vec<usize> = (b_lo..b_hi).filter(|&j| from[j].is_none()).collect();
        let pairs = pair_similar(a, b, &rest_a, &rest_b);
        if pairs.is_empty() && rest_a.len() == 1 && rest_b.len() == 1 {
            // The only block between the same two neighbours on both sides: one
            // block rewritten. Its place is the evidence, so the confidence is
            // at least the pairing threshold.
            let (i, j) = (rest_a[0], rest_b[0]);
            if block_kind(&a[i]) == block_kind(&b[j]) {
                let sim = similarity(&a[i].text(), &b[j].text()).max(MODIFY_SIMILARITY);
                fates[i] = Fate::Modified {
                    to: j,
                    similarity: sim,
                };
                from[j] = Some(i);
            }
            continue;
        }
        for (i, j, sim) in pairs {
            fates[i] = Fate::Modified {
                to: j,
                similarity: sim,
            };
            from[j] = Some(i);
        }
    }

    // Blocks that moved and were edited.
    detect_moves(a, b, &a_keys, &b_keys, &mut fates, &mut from, 2..3);
    Alignment {
        fates,
        from,
        a_keys,
        b_keys,
    }
}

fn pair_weight(a: &Block, b: &Block) -> Option<u8> {
    if block_kind(a) != block_kind(b) {
        return None;
    }
    let mut sim = similarity(&a.text(), &b.text());
    if let (Block::Paragraph(pa), Block::Paragraph(pb)) = (a, b)
        && pa.anchor.is_some()
        && pa.anchor == pb.anchor
    {
        // Word kept the paragraph id: a strong hint they are one paragraph.
        sim = sim.saturating_add(20).min(100);
    }
    (sim >= MODIFY_SIMILARITY).then_some(sim)
}

/// Order-preserving pairing that maximises total similarity.
fn pair_similar(
    a: &[Block],
    b: &[Block],
    rest_a: &[usize],
    rest_b: &[usize],
) -> Vec<(usize, usize, u8)> {
    let (n, m) = (rest_a.len(), rest_b.len());
    if n == 0 || m == 0 {
        return Vec::new();
    }
    let weight = |x: usize, y: usize| pair_weight(&a[rest_a[x]], &b[rest_b[y]]);
    if n * m > PAIRING_CELLS {
        // Greedy in order: good enough for huge gaps, which real documents don't have.
        let mut out = Vec::new();
        let mut y0 = 0;
        for (x, &i) in rest_a.iter().enumerate() {
            if let Some((y, sim)) = (y0..m).find_map(|y| weight(x, y).map(|s| (y, s))) {
                out.push((i, rest_b[y], sim));
                y0 = y + 1;
            }
        }
        return out;
    }
    let width = m + 1;
    let mut score = vec![0_u32; (n + 1) * width];
    let mut weights = vec![None; n * m];
    for x in 0..n {
        for y in 0..m {
            weights[x * m + y] = weight(x, y);
        }
    }
    for x in (0..n).rev() {
        for y in (0..m).rev() {
            let skip = score[(x + 1) * width + y].max(score[x * width + y + 1]);
            let take =
                weights[x * m + y].map_or(0, |w| u32::from(w) + score[(x + 1) * width + y + 1]);
            score[x * width + y] = skip.max(take);
        }
    }
    let mut out = Vec::new();
    let (mut x, mut y) = (0, 0);
    while x < n && y < m {
        let here = score[x * width + y];
        if let Some(w) = weights[x * m + y]
            && here == u32::from(w) + score[(x + 1) * width + y + 1]
        {
            out.push((rest_a[x], rest_b[y], w));
            x += 1;
            y += 1;
        } else if here == score[(x + 1) * width + y] {
            x += 1;
        } else {
            y += 1;
        }
    }
    out
}

fn detect_moves(
    a: &[Block],
    b: &[Block],
    a_keys: &[Keys],
    b_keys: &[Keys],
    fates: &mut [Fate],
    from: &mut [Option<usize>],
    passes: std::ops::Range<u8>,
) {
    let mut lone_a: Vec<usize> = (0..a.len())
        .filter(|&i| fates[i] == Fate::Deleted)
        .collect();
    let mut lone_b: Vec<usize> = (0..b.len()).filter(|&j| from[j].is_none()).collect();
    // Identical, then same content, then similar enough.
    for pass in passes {
        let mut matched_a = Vec::new();
        for &i in &lone_a {
            let found = lone_b.iter().position(|&j| match pass {
                0 => a_keys[i] == b_keys[j],
                1 => a_keys[i].content == b_keys[j].content,
                _ => {
                    block_kind(&a[i]) == block_kind(&b[j])
                        && !a[i].text().trim().is_empty()
                        && similarity(&a[i].text(), &b[j].text()) >= MOVE_SIMILARITY
                }
            });
            if let Some(pos) = found {
                let j = lone_b.remove(pos);
                let sim = if pass < 2 {
                    100
                } else {
                    similarity(&a[i].text(), &b[j].text())
                };
                fates[i] = Fate::Moved {
                    to: j,
                    content_same: pass < 2,
                    format_same: pass == 0,
                    similarity: sim,
                };
                from[j] = Some(i);
                matched_a.push(i);
            }
        }
        lone_a.retain(|i| !matched_a.contains(i));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docx::Docx;
    use lineage_testkit::resume;

    fn body(lines: &[resume::Line]) -> Docx {
        Docx::open(&resume::package("t", lines).to_bytes()).expect("opens")
    }

    #[test]
    fn identical_documents_align_completely() {
        let a = body(&resume::master_v1_lines());
        let al = align(&a.body().blocks, &a.body().blocks, None, None);
        assert!(al.fates.iter().all(Fate::unchanged));
        assert_eq!(al.inserted().count(), 0);
    }

    #[test]
    fn insertions_have_a_gap() {
        let a = body(&resume::master_v1_lines());
        let b = body(&resume::master_v2_lines());
        let al = align(&a.body().blocks, &b.body().blocks, None, None);
        let inserted: Vec<usize> = al.inserted().collect();
        assert_eq!(inserted, [5, 6]);
        // Both new blocks follow "Experience" (block 4 in v1).
        assert_eq!(al.gap_before(5), Some(4));
        assert_eq!(al.gap_before(6), Some(4));
        assert!(al.fates.iter().all(Fate::unchanged));
    }

    #[test]
    fn moved_sections_are_moves() {
        let a = body(&resume::master_v1_lines());
        let b = body(&resume::reordered_lines());
        let al = align(&a.body().blocks, &b.body().blocks, None, None);
        let moved = al
            .fates
            .iter()
            .filter(|f| matches!(f, Fate::Moved { .. }))
            .count();
        assert!((2..=4).contains(&moved), "{:?}", al.fates);
        assert!(!al.fates.contains(&Fate::Deleted));
        assert_eq!(al.inserted().count(), 0);
    }

    /// Found by the property tests: a reformatted block and an edited block
    /// paired in separate passes crossed each other, which put them in the
    /// wrong order when written.
    #[test]
    fn edited_pairs_never_cross_reformatted_ones() {
        use lineage_testkit::docx::{Package, para, run, run_with};
        let doc = |paras: &[(&str, bool)]| {
            let body: String = paras
                .iter()
                .map(|(t, bold)| {
                    if *bold {
                        para("", &run_with("<w:b/>", t))
                    } else {
                        para("", &run(t))
                    }
                })
                .collect();
            Docx::open(&Package::new("t", body).to_bytes()).expect("opens")
        };
        let a = doc(&[
            ("data", false),
            ("data", false),
            ("engineer", false),
            ("engineer", false),
        ]);
        let b = doc(&[
            ("engineer", true),
            ("data data", false),
            ("engineer", false),
            ("data", false),
        ]);
        let al = align(&a.body().blocks, &b.body().blocks, None, None);
        let mut in_place: Vec<(usize, usize)> = al
            .fates
            .iter()
            .enumerate()
            .filter(|(_, f)| f.stayed())
            .filter_map(|(i, f)| f.target().map(|j| (i, j)))
            .collect();
        in_place.sort_unstable();
        assert!(
            in_place.windows(2).all(|w| w[0].1 < w[1].1),
            "{:?}",
            al.fates
        );
    }

    #[test]
    fn edits_and_formatting_are_told_apart() {
        let a = body(&resume::master_v2_lines());
        let b = body(&resume::master_v3_lines());
        let al = align(&a.body().blocks, &b.body().blocks, None, None);
        assert_eq!(al.fates[0], Fate::Formatted(0), "the name made bold");
        let corvia = resume::master_v2_lines()
            .iter()
            .position(|l| *l == resume::Line::H2(resume::CORVIA_TITLE.to_owned()))
            .expect("corvia");
        assert!(matches!(al.fates[corvia], Fate::Modified { to, .. } if to == corvia));
    }
}
