// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What changed inside a table: rows are aligned, then cells by column, then the
//! blocks inside each cell. Changed columns or merged cells make the whole table
//! one change, which is brought in by hand in this version.

use std::collections::BTreeMap;

use super::keys::{Keys, block_keys};
use super::lcs::lcs;
use super::similarity::similarity;
use super::{
    ByHand, CellStep, Change, ChangeKind, FormatNote, Path, Segment, SegmentKind, SeqDiff, Side,
    Unit, blank, heading_before, sequence,
};
use crate::docx::{Row, StoryKey, Table};
use crate::hash::Fnv;

/// What became of one row of the earlier table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowFate {
    Same(usize),
    /// Paired with a later row whose cells (or formatting) differ.
    Changed(usize),
    Deleted,
}

impl RowFate {
    #[must_use]
    pub fn target(self) -> Option<usize> {
        match self {
            Self::Same(j) | Self::Changed(j) => Some(j),
            Self::Deleted => None,
        }
    }
}

/// The alignment of two tables.
#[derive(Clone, Debug)]
pub struct TableDiff {
    /// Columns or merged cells changed: the table is one unit.
    pub structural: bool,
    /// Table-level properties (borders, widths, style) changed.
    pub props_changed: bool,
    /// For each earlier row.
    pub rows: Vec<RowFate>,
    /// For each later row, the earlier row it came from.
    pub row_from: Vec<Option<usize>>,
    /// For paired rows whose cells differ, keyed by (earlier row, column).
    pub cells: BTreeMap<(usize, usize), SeqDiff>,
}

impl TableDiff {
    /// The earlier row a new later row follows, or `None` for the first row.
    #[must_use]
    pub fn row_gap_before(&self, j: usize) -> Option<usize> {
        (0..j).rev().find_map(|k| self.row_from[k])
    }
}

fn row_keys(row: &Row, src: Option<&str>) -> Keys {
    let mut content = Fnv::new();
    let mut format = Fnv::new();
    format.u64(row.props_key);
    for cell in &row.cells {
        content.str("tc").u64(cell.grid_span as u64).u64(u64::from(
            cell.vmerge.map_or(0_u8, |restart| 1 + u8::from(restart)),
        ));
        format.u64(cell.props_key);
        for block in &cell.blocks {
            let keys = block_keys(block, src);
            content.u64(keys.content);
            format.u64(keys.format);
        }
    }
    Keys {
        content: content.finish(),
        format: format.finish(),
    }
}

/// How alike two rows are: the share of cells with the same content, or their
/// text similarity, whichever is higher.
fn row_similarity(x: &Row, y: &Row, a_src: Option<&str>, b_src: Option<&str>) -> u8 {
    let by_text = similarity(&x.text(), &y.text());
    if x.cells.is_empty() || x.cells.len() != y.cells.len() {
        return by_text;
    }
    let cell_key = |cell: &crate::docx::Cell, src: Option<&str>| {
        let mut h = Fnv::new();
        for block in &cell.blocks {
            h.u64(block_keys(block, src).content);
        }
        h.finish()
    };
    let equal = x
        .cells
        .iter()
        .zip(&y.cells)
        .filter(|(cx, cy)| cell_key(cx, a_src) == cell_key(cy, b_src))
        .count();
    let by_cells = u8::try_from(100 * equal / x.cells.len()).unwrap_or(100);
    by_text.max(by_cells)
}

fn align_rows(
    ta: &Table,
    tb: &Table,
    a_src: Option<&str>,
    b_src: Option<&str>,
) -> (Vec<RowFate>, Vec<Option<usize>>) {
    let ka: Vec<Keys> = ta.rows.iter().map(|r| row_keys(r, a_src)).collect();
    let kb: Vec<Keys> = tb.rows.iter().map(|r| row_keys(r, b_src)).collect();
    let mut fates = vec![RowFate::Deleted; ka.len()];
    let mut from = vec![None; kb.len()];
    let full_a: Vec<(u64, u64)> = ka.iter().map(|k| k.full()).collect();
    let full_b: Vec<(u64, u64)> = kb.iter().map(|k| k.full()).collect();
    let same = lcs(&full_a, &full_b);
    for &(i, j) in &same {
        fates[i] = RowFate::Same(j);
        from[j] = Some(i);
    }
    let mut starts = vec![(0, 0)];
    starts.extend(same.iter().map(|&(i, j)| (i + 1, j + 1)));
    let mut ends = same.clone();
    ends.push((ka.len(), kb.len()));
    for (&(a_lo, b_lo), &(a_hi, b_hi)) in starts.iter().zip(&ends) {
        let gap_a: Vec<usize> = (a_lo..a_hi).collect();
        let gap_b: Vec<usize> = (b_lo..b_hi).collect();
        if gap_a.is_empty() || gap_b.is_empty() {
            continue;
        }
        let ca: Vec<u64> = gap_a.iter().map(|&i| ka[i].content).collect();
        let cb: Vec<u64> = gap_b.iter().map(|&j| kb[j].content).collect();
        for (x, y) in lcs(&ca, &cb) {
            fates[gap_a[x]] = RowFate::Changed(gap_b[y]);
            from[gap_b[y]] = Some(gap_a[x]);
        }
        // Edited rows: pair in order when similar enough.
        let mut y0 = 0;
        for &i in &gap_a {
            if fates[i] != RowFate::Deleted {
                continue;
            }
            let rest: Vec<usize> = gap_b[y0..]
                .iter()
                .copied()
                .filter(|&j| from[j].is_none())
                .collect();
            if let Some(&j) = rest.iter().find(|&&j| {
                row_similarity(&ta.rows[i], &tb.rows[j], a_src, b_src)
                    >= super::align::MODIFY_SIMILARITY
            }) {
                fates[i] = RowFate::Changed(j);
                from[j] = Some(i);
                y0 = gap_b.iter().position(|&k| k == j).map_or(y0, |p| p + 1);
            }
        }
    }
    (fates, from)
}

fn row_path(side: &Side<'_>, table: usize, row: usize) -> Path {
    let mut path = side.path(table);
    path.cells.push(CellStep {
        row,
        column: None,
        block: None,
    });
    path
}

/// Diffs two paired tables, adding their changes to `out`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn diff_table(
    story: &StoryKey,
    a: &Side<'_>,
    b: &Side<'_>,
    ta: &Table,
    tb: &Table,
    i: usize,
    j: usize,
    out: &mut Vec<Change>,
) -> TableDiff {
    let (rows, row_from) = align_rows(ta, tb, a.src, b.src);
    let mut structural = ta.columns != tb.columns;
    for (ra, fate) in rows.iter().enumerate() {
        if let RowFate::Changed(rb) = *fate {
            let (x, y) = (&ta.rows[ra], &tb.rows[rb]);
            structural |= x.cells.len() != y.cells.len()
                || x.cells
                    .iter()
                    .zip(&y.cells)
                    .any(|(cx, cy)| cx.grid_span != cy.grid_span || cx.vmerge != cy.vmerge);
        }
    }
    let heading = heading_before(b.blocks, j).or_else(|| heading_before(a.blocks, i));
    let locked = ta.lock.is_some() || tb.lock.is_some();
    let mut diff = TableDiff {
        structural,
        props_changed: ta.props_key != tb.props_key,
        rows,
        row_from,
        cells: BTreeMap::new(),
    };

    let whole = |kind: ChangeKind, notes: Vec<FormatNote>| {
        let mut change = blank(story, b.part, Unit::Table);
        change.kind = kind;
        change.formatting_only = kind == ChangeKind::Format;
        change.before = Some(a.path(i));
        change.after = Some(b.path(j));
        change.before_text = Some(ta.text());
        change.after_text = Some(tb.text());
        change.heading.clone_from(&heading);
        change.segments = vec![Segment {
            kind: SegmentKind::Same,
            text: tb.text(),
        }];
        change.format_notes = notes;
        change.by_hand = Some(if locked {
            ByHand::Locked
        } else {
            ByHand::TableStructure
        });
        change
    };

    if structural {
        let same_content = super::keys::block_keys(&crate::docx::Block::Table(ta.clone()), a.src)
            .content
            == super::keys::block_keys(&crate::docx::Block::Table(tb.clone()), b.src).content;
        let kind = if same_content {
            ChangeKind::Format
        } else {
            ChangeKind::Modify
        };
        out.push(whole(kind, vec![FormatNote::Table]));
        return diff;
    }
    if diff.props_changed {
        out.push(whole(ChangeKind::Format, vec![FormatNote::Table]));
    }

    // Rows in the later table's order, with deletions before the next survivor.
    let mut row_changes: Vec<(usize, u8, Change)> = Vec::new();
    let next_surviving = |ra: usize| {
        (ra + 1..diff.rows.len())
            .find_map(|k| diff.rows[k].target())
            .unwrap_or(tb.rows.len())
    };
    for (ra, fate) in diff.rows.iter().enumerate() {
        match *fate {
            RowFate::Same(_) => {}
            RowFate::Deleted => {
                let mut change = blank(story, a.part, Unit::Row);
                change.kind = ChangeKind::Delete;
                change.before = Some(row_path(a, i, ra));
                change.before_text = Some(ta.rows[ra].text());
                change.heading.clone_from(&heading);
                change.segments = vec![Segment {
                    kind: SegmentKind::Removed,
                    text: ta.rows[ra].text(),
                }];
                change.by_hand = locked.then_some(ByHand::Locked);
                row_changes.push((next_surviving(ra), 0, change));
            }
            RowFate::Changed(rb) => {
                let (x, y) = (&ta.rows[ra], &tb.rows[rb]);
                if x.props_key != y.props_key {
                    let mut change = blank(story, b.part, Unit::Row);
                    change.kind = ChangeKind::Format;
                    change.formatting_only = true;
                    change.before = Some(row_path(a, i, ra));
                    change.after = Some(row_path(b, j, rb));
                    change.before_text = Some(x.text());
                    change.after_text = Some(y.text());
                    change.heading.clone_from(&heading);
                    change.format_notes = vec![FormatNote::Table];
                    change.segments = vec![Segment {
                        kind: SegmentKind::Reformatted,
                        text: y.text(),
                    }];
                    change.by_hand = Some(ByHand::TableStructure);
                    row_changes.push((rb, 1, change));
                }
                for (column, (cx, cy)) in x.cells.iter().zip(&y.cells).enumerate() {
                    let pa = a.path(i);
                    let pb = b.path(j);
                    let mut scope_a = pa.cells.clone();
                    scope_a.push(CellStep {
                        row: ra,
                        column: Some(column),
                        block: None,
                    });
                    let mut scope_b = pb.cells.clone();
                    scope_b.push(CellStep {
                        row: rb,
                        column: Some(column),
                        block: None,
                    });
                    let side_a = Side {
                        blocks: &cx.blocks,
                        src: a.src,
                        part: a.part,
                        top: Some(pa.block),
                        scope: &scope_a,
                    };
                    let side_b = Side {
                        blocks: &cy.blocks,
                        src: b.src,
                        part: b.part,
                        top: Some(pb.block),
                        scope: &scope_b,
                    };
                    let mut cell_changes = Vec::new();
                    let seq = sequence(story, side_a, side_b, &mut cell_changes);
                    if cx.props_key != cy.props_key {
                        let mut change = blank(story, b.part, Unit::Row);
                        change.kind = ChangeKind::Format;
                        change.formatting_only = true;
                        change.before = Some(row_path(a, i, ra));
                        change.after = Some(row_path(b, j, rb));
                        change.before_text = Some(cx.text());
                        change.after_text = Some(cy.text());
                        change.heading.clone_from(&heading);
                        change.format_notes = vec![FormatNote::Table];
                        change.by_hand = Some(ByHand::TableStructure);
                        cell_changes.push(change);
                    }
                    if cell_changes.is_empty() {
                        continue;
                    }
                    for mut change in cell_changes {
                        if change.heading.is_none() {
                            change.heading.clone_from(&heading);
                        }
                        if locked {
                            change.by_hand = Some(ByHand::Locked);
                        }
                        row_changes.push((rb, 1, change));
                    }
                    diff.cells.insert((ra, column), seq);
                }
            }
        }
    }
    for rb in 0..tb.rows.len() {
        if diff.row_from[rb].is_some() {
            continue;
        }
        let mut change = blank(story, b.part, Unit::Row);
        change.kind = ChangeKind::Insert;
        change.after = Some(row_path(b, j, rb));
        change.gap = Some(
            diff.row_gap_before(rb)
                .map_or_else(|| a.path(i), |ra| row_path(a, i, ra)),
        );
        change.after_text = Some(tb.rows[rb].text());
        change.heading.clone_from(&heading);
        change.segments = vec![Segment {
            kind: SegmentKind::Added,
            text: tb.rows[rb].text(),
        }];
        change.by_hand = locked.then_some(ByHand::Locked);
        row_changes.push((rb, 1, change));
    }
    row_changes.sort_by_key(|(pos, order, _)| (*pos, *order));
    out.extend(row_changes.into_iter().map(|(_, _, c)| c));
    diff
}
