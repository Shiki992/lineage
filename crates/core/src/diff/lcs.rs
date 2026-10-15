// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Longest common subsequence of two key sequences, with patience anchors.
//!
//! Patience first matches keys that occur exactly once on each side and keeps the
//! longest in-order run of those matches. The gaps between them are filled
//! recursively, and finally by a plain dynamic-programming LCS, which is capped
//! so a pathological document cannot use unbounded time or memory.

/// Largest DP table (cells) before falling back to "no match in this gap".
const DP_CELLS: usize = 4_000_000;

/// Matched index pairs `(a, b)`, increasing in both.
#[must_use]
pub fn lcs<K: Eq + Ord + Copy>(a: &[K], b: &[K]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    patience(a, b, 0, 0, &mut out);
    out
}

fn patience<K: Eq + Ord + Copy>(
    a: &[K],
    b: &[K],
    a_off: usize,
    b_off: usize,
    out: &mut Vec<(usize, usize)>,
) {
    // Common prefix and suffix.
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    for i in 0..prefix {
        out.push((a_off + i, b_off + i));
    }
    let a = &a[prefix..];
    let b = &b[prefix..];
    let suffix = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let a_mid = &a[..a.len() - suffix];
    let b_mid = &b[..b.len() - suffix];
    let a_mid_off = a_off + prefix;
    let b_mid_off = b_off + prefix;

    if !a_mid.is_empty() && !b_mid.is_empty() {
        let anchors = unique_anchors(a_mid, b_mid);
        if anchors.is_empty() {
            dp(a_mid, b_mid, a_mid_off, b_mid_off, out);
        } else {
            let mut last = (0, 0);
            for (ai, bi) in anchors {
                patience(
                    &a_mid[last.0..ai],
                    &b_mid[last.1..bi],
                    a_mid_off + last.0,
                    b_mid_off + last.1,
                    out,
                );
                out.push((a_mid_off + ai, b_mid_off + bi));
                last = (ai + 1, bi + 1);
            }
            patience(
                &a_mid[last.0..],
                &b_mid[last.1..],
                a_mid_off + last.0,
                b_mid_off + last.1,
                out,
            );
        }
    }

    let a_suffix_start = a_off + prefix + a_mid.len();
    let b_suffix_start = b_off + prefix + b_mid.len();
    for i in 0..suffix {
        out.push((a_suffix_start + i, b_suffix_start + i));
    }
}

/// Keys unique on both sides, matched, reduced to the longest run increasing in `b`.
fn unique_anchors<K: Eq + Ord + Copy>(a: &[K], b: &[K]) -> Vec<(usize, usize)> {
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<K, (usize, usize, usize, usize)> = BTreeMap::new();
    for (i, k) in a.iter().enumerate() {
        let entry = counts.entry(*k).or_insert((0, 0, 0, 0));
        entry.0 += 1;
        entry.2 = i;
    }
    for (j, k) in b.iter().enumerate() {
        if let Some(entry) = counts.get_mut(k) {
            entry.1 += 1;
            entry.3 = j;
        }
    }
    let mut pairs: Vec<(usize, usize)> = counts
        .values()
        .filter(|(ca, cb, _, _)| *ca == 1 && *cb == 1)
        .map(|(_, _, i, j)| (*i, *j))
        .collect();
    pairs.sort_unstable();
    longest_increasing(&pairs)
}

/// Longest subsequence of `pairs` (sorted by `.0`) that also increases in `.1`.
#[must_use]
pub fn longest_increasing(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    if pairs.is_empty() {
        return Vec::new();
    }
    // tails[k] = index into pairs of the smallest tail of an increasing run of length k+1.
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; pairs.len()];
    for (index, &(_, j)) in pairs.iter().enumerate() {
        let pos = tails.partition_point(|&t| pairs[t].1 < j);
        if pos > 0 {
            prev[index] = Some(tails[pos - 1]);
        }
        if pos == tails.len() {
            tails.push(index);
        } else {
            tails[pos] = index;
        }
    }
    let mut out = Vec::with_capacity(tails.len());
    let mut cursor = tails.last().copied();
    while let Some(index) = cursor {
        out.push(pairs[index]);
        cursor = prev[index];
    }
    out.reverse();
    out
}

fn dp<K: Eq + Copy>(a: &[K], b: &[K], a_off: usize, b_off: usize, out: &mut Vec<(usize, usize)>) {
    let (n, m) = (a.len(), b.len());
    if n.saturating_mul(m) > DP_CELLS {
        return;
    }
    let width = m + 1;
    let mut table = vec![0_u32; (n + 1) * width];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i * width + j] = if a[i] == b[j] {
                table[(i + 1) * width + j + 1] + 1
            } else {
                table[(i + 1) * width + j].max(table[i * width + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((a_off + i, b_off + j));
            i += 1;
            j += 1;
        } else if table[(i + 1) * width + j + 1] == table[i * width + j] {
            // Treat the two as one changed line when that loses no matches.
            // With repeated lines, this keeps each paired with its counterpart
            // in the same place, in both directions, rather than shifting every
            // pairing by one.
            i += 1;
            j += 1;
        } else {
            let skip_a = table[(i + 1) * width + j];
            let skip_b = table[i * width + j + 1];
            // On a remaining tie, stay near the diagonal.
            if skip_a > skip_b || (skip_a == skip_b && n - i > m - j) {
                i += 1;
            } else {
                j += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(a: &str, b: &str, expected: usize) {
        let a: Vec<char> = a.chars().collect();
        let b: Vec<char> = b.chars().collect();
        let pairs = lcs(&a, &b);
        assert_eq!(pairs.len(), expected, "{a:?} {b:?}");
        for window in pairs.windows(2) {
            assert!(window[0].0 < window[1].0 && window[0].1 < window[1].1);
        }
        for (i, j) in pairs {
            assert_eq!(a[i], b[j]);
        }
    }

    #[test]
    fn finds_common_subsequences() {
        check("", "", 0);
        check("abc", "", 0);
        check("abc", "abc", 3);
        check("abcd", "acbd", 3);
        check("xaybzc", "abc", 3);
        check("kitten", "sitting", 4);
        check("aaaa", "aa", 2);
    }

    #[test]
    fn patience_keeps_unique_lines_in_order() {
        // A moved unique line is not matched; the rest is.
        let a = [1, 2, 3, 4, 5];
        let b = [1, 3, 4, 5, 2];
        let pairs = lcs(&a, &b);
        assert_eq!(pairs, [(0, 0), (2, 1), (3, 2), (4, 3)]);
    }

    /// Found by the property tests: with repeated lines, ties must keep lines
    /// paired with their counterparts in the same place.
    #[test]
    fn ties_stay_on_the_diagonal() {
        // a: data, data, nightly, nightly
        // b: data, "data data", nightly, "nightly data"
        let a = [1, 1, 2, 2];
        let b = [1, 3, 2, 4];
        assert_eq!(lcs(&a, &b), [(0, 0), (2, 2)]);
        // An edit to the first of two equal lines, in both directions.
        let edited = [5, 9, 9, 7];
        let plain = [6, 9, 9, 7];
        assert_eq!(lcs(&[9, 9, 7], &[8, 9, 7]), [(1, 1), (2, 2)]);
        assert_eq!(lcs(&[8, 9, 7], &[9, 9, 7]), [(1, 1), (2, 2)]);
        assert_eq!(lcs(&edited, &plain), [(1, 1), (2, 2), (3, 3)]);
    }

    #[test]
    fn longest_increasing_run() {
        let pairs = [(0, 3), (1, 0), (2, 1), (3, 4), (4, 2)];
        assert_eq!(longest_increasing(&pairs), [(1, 0), (2, 1), (4, 2)]);
    }
}
