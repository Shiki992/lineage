// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What changed in code: a line diff for macros (VBA) and queries (Power
//! Query M), and the view the panel draws, with removed and added lines
//! marked and a few unchanged lines around each change.
//!
//! Lines are matched with the patience LCS the document diff uses
//! ([`crate::diff::lcs`]), which is capped, so a very long module can't use
//! unbounded time or memory.

use crate::diff::lcs::lcs;
use crate::hash::fnv;

/// Unchanged lines shown around a change.
pub const CONTEXT: usize = 3;
/// Most lines one code view carries; the rest are counted.
pub const MAX_VIEW_LINES: usize = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Same,
    Removed,
    Added,
    /// Unchanged lines not shown: `text` says how many.
    Skipped,
}

/// One line of a code view. Line numbers count from 1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeLine {
    pub kind: LineKind,
    pub before: Option<u32>,
    pub after: Option<u32>,
    pub text: String,
}

/// Which lines of `a` and `b` are the same, and which were removed or
/// added. Indexes from 0.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineDiff {
    pub pairs: Vec<(usize, usize)>,
    pub removed: Vec<usize>,
    pub added: Vec<usize>,
}

impl LineDiff {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.added.is_empty()
    }

    /// "Lines changed" as a person counts them: in each run of changes,
    /// a line edited in place is one line, not one removed and one added.
    #[must_use]
    pub fn changed(&self) -> usize {
        // The gaps between matched pairs are the runs of changes.
        let mut total = 0;
        let mut last = (0usize, 0usize);
        for &(i, j) in &self.pairs {
            total += (i - last.0).max(j - last.1);
            last = (i + 1, j + 1);
        }
        let tail_a = self.removed.iter().filter(|&&i| i >= last.0).count();
        let tail_b = self.added.iter().filter(|&&j| j >= last.1).count();
        total + tail_a.max(tail_b)
    }
}

/// The line diff of `a` and `b`.
#[must_use]
pub fn line_diff(a: &[String], b: &[String]) -> LineDiff {
    let ka: Vec<u64> = a.iter().map(|l| fnv(l)).collect();
    let kb: Vec<u64> = b.iter().map(|l| fnv(l)).collect();
    // Common lines at both ends first: most edits touch a few lines.
    let mut head = 0;
    while head < ka.len() && head < kb.len() && ka[head] == kb[head] && a[head] == b[head] {
        head += 1;
    }
    let mut tail = 0;
    while tail < ka.len() - head
        && tail < kb.len() - head
        && ka[ka.len() - 1 - tail] == kb[kb.len() - 1 - tail]
        && a[a.len() - 1 - tail] == b[b.len() - 1 - tail]
    {
        tail += 1;
    }
    let mid_a = &ka[head..ka.len() - tail];
    let mid_b = &kb[head..kb.len() - tail];
    let mut pairs: Vec<(usize, usize)> = (0..head).map(|i| (i, i)).collect();
    for (i, j) in lcs(mid_a, mid_b) {
        // Keys can collide; a pair must be the same text.
        if a[head + i] == b[head + j] {
            pairs.push((head + i, head + j));
        }
    }
    for k in 0..tail {
        pairs.push((a.len() - tail + k, b.len() - tail + k));
    }
    let mut in_a = vec![false; a.len()];
    let mut in_b = vec![false; b.len()];
    for &(i, j) in &pairs {
        in_a[i] = true;
        in_b[j] = true;
    }
    LineDiff {
        removed: (0..a.len()).filter(|&i| !in_a[i]).collect(),
        added: (0..b.len()).filter(|&j| !in_b[j]).collect(),
        pairs,
    }
}

fn number(i: usize) -> Option<u32> {
    u32::try_from(i + 1).ok()
}

/// Every line of `a` and `b` in order, unchanged ones as `Same`.
fn full(a: &[String], b: &[String], d: &LineDiff) -> Vec<CodeLine> {
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    for &(pi, pj) in d.pairs.iter().chain(std::iter::once(&(a.len(), b.len()))) {
        while i < pi {
            out.push(CodeLine {
                kind: LineKind::Removed,
                before: number(i),
                after: None,
                text: a[i].clone(),
            });
            i += 1;
        }
        while j < pj {
            out.push(CodeLine {
                kind: LineKind::Added,
                before: None,
                after: number(j),
                text: b[j].clone(),
            });
            j += 1;
        }
        if pi < a.len() && pj < b.len() {
            out.push(CodeLine {
                kind: LineKind::Same,
                before: number(pi),
                after: number(pj),
                text: a[pi].clone(),
            });
            i = pi + 1;
            j = pj + 1;
        }
    }
    out
}

/// The view: changed lines with [`CONTEXT`] unchanged lines around them,
/// longer unchanged stretches as one `Skipped` line, and at most
/// [`MAX_VIEW_LINES`] lines (the rest counted in a last `Skipped` line).
#[must_use]
pub fn view(a: &[String], b: &[String], d: &LineDiff) -> Vec<CodeLine> {
    let all = full(a, b, d);
    let near: Vec<bool> = {
        let mut keep = vec![false; all.len()];
        for (k, line) in all.iter().enumerate() {
            if line.kind != LineKind::Same {
                let from = k.saturating_sub(CONTEXT);
                let to = (k + CONTEXT + 1).min(all.len());
                for slot in &mut keep[from..to] {
                    *slot = true;
                }
            }
        }
        keep
    };
    let mut out = Vec::new();
    let mut skipped = 0usize;
    let mut cut = 0usize;
    for (line, keep) in all.into_iter().zip(near) {
        if !keep {
            skipped += 1;
            continue;
        }
        if skipped > 0 {
            out.push(skipped_line(skipped));
            skipped = 0;
        }
        if out.len() >= MAX_VIEW_LINES {
            cut += 1;
            continue;
        }
        out.push(line);
    }
    if skipped > 0 && cut == 0 {
        out.push(skipped_line(skipped));
    }
    if cut > 0 {
        out.push(CodeLine {
            kind: LineKind::Skipped,
            before: None,
            after: None,
            text: format!("… and {cut} more {}", plural(cut, "line", "lines")),
        });
    }
    out
}

fn skipped_line(n: usize) -> CodeLine {
    CodeLine {
        kind: LineKind::Skipped,
        before: None,
        after: None,
        text: format!("{n} unchanged {}", plural(n, "line", "lines")),
    }
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 { one } else { many }
}

/// How alike two pieces of code are, 0 to 100: the share of lines in
/// common.
#[must_use]
pub fn similar(a: &[String], b: &[String]) -> u8 {
    if a.is_empty() && b.is_empty() {
        return 100;
    }
    let d = line_diff(a, b);
    let common = d.pairs.len() * 2;
    let total = a.len() + b.len();
    u8::try_from(common * 100 / total.max(1)).unwrap_or(100)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(s: &str) -> Vec<String> {
        s.lines().map(str::to_owned).collect()
    }

    #[test]
    fn an_edited_line_is_one_change() {
        let a = lines("Sub A()\n  x = 1\nEnd Sub");
        let b = lines("Sub A()\n  x = 2\nEnd Sub");
        let d = line_diff(&a, &b);
        assert_eq!(d.removed, [1]);
        assert_eq!(d.added, [1]);
        assert_eq!(d.changed(), 1);
        let v = view(&a, &b, &d);
        let kinds: Vec<LineKind> = v.iter().map(|l| l.kind).collect();
        assert_eq!(
            kinds,
            [
                LineKind::Same,
                LineKind::Removed,
                LineKind::Added,
                LineKind::Same
            ]
        );
        assert_eq!(v[1].before, Some(2));
        assert_eq!(v[2].after, Some(2));
    }

    #[test]
    fn long_unchanged_stretches_are_skipped() {
        let mut a: Vec<String> = (0..100).map(|n| format!("line {n}")).collect();
        let b = {
            let mut b = a.clone();
            b[50] = "changed".to_owned();
            b
        };
        let d = line_diff(&a, &b);
        let v = view(&a, &b, &d);
        assert_eq!(v.first().unwrap().text, "47 unchanged lines");
        assert_eq!(v.last().unwrap().text, "46 unchanged lines");
        assert_eq!(v.len(), 2 + 3 + 2 + 3);
        a.clear();
        assert_eq!(line_diff(&a, &b).added.len(), 100);
    }

    #[test]
    fn a_huge_change_is_capped() {
        let b: Vec<String> = (0..5_000).map(|n| format!("x = {n}")).collect();
        let d = line_diff(&[], &b);
        let v = view(&[], &b, &d);
        assert_eq!(v.len(), MAX_VIEW_LINES + 1);
        assert_eq!(v.last().unwrap().text, "… and 4600 more lines");
        assert_eq!(d.changed(), 5_000);
    }

    #[test]
    fn similarity_counts_shared_lines() {
        let a = lines("a\nb\nc\nd");
        assert_eq!(similar(&a, &a), 100);
        assert_eq!(similar(&a, &lines("a\nb\nx\ny")), 50);
        assert_eq!(similar(&a, &[]), 0);
    }
}
