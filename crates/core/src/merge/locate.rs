// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Following a place in the base document into another version through their
//! alignment: where did this paragraph (or cell, or gap) end up?

use crate::diff::{CellStep, Path, RowFate, SeqDiff, StoryDiff};
use crate::docx::{Block, Docx, StoryKey};

/// Where a base block is in the other version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Mapped {
    /// In place and unchanged.
    Same(Path),
    /// In place, edited or reformatted.
    Changed(Path),
    /// Moved elsewhere.
    Moved(Path),
    Gone,
    /// Inside a table whose structure changed: no reliable mapping.
    Unknown,
}

impl Mapped {
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::Same(p) | Self::Changed(p) | Self::Moved(p) => Some(p),
            Self::Gone | Self::Unknown => None,
        }
    }
}

fn wrap(top: usize, step: CellStep, inner: Mapped) -> Mapped {
    let lift = |p: Path| {
        let mut cells = vec![CellStep {
            row: step.row,
            column: step.column,
            block: Some(p.block),
        }];
        cells.extend(p.cells);
        Path { block: top, cells }
    };
    match inner {
        Mapped::Same(p) => Mapped::Same(lift(p)),
        Mapped::Changed(p) => Mapped::Changed(lift(p)),
        Mapped::Moved(p) => Mapped::Moved(lift(p)),
        other => other,
    }
}

/// Maps a base path through a sequence alignment (`None` = identity, for
/// sequences that did not change).
pub fn map_in(seq: Option<&SeqDiff>, block: usize, steps: &[CellStep]) -> Mapped {
    let Some(seq) = seq else {
        return Mapped::Same(Path {
            block,
            cells: steps.to_vec(),
        });
    };
    let Some(fate) = seq.alignment.fates.get(block).copied() else {
        return Mapped::Gone;
    };
    use crate::diff::Fate;
    if steps.is_empty() {
        return match fate {
            Fate::Same(j) => Mapped::Same(Path::top(j)),
            Fate::Formatted(j) | Fate::Modified { to: j, .. } => Mapped::Changed(Path::top(j)),
            Fate::Moved { to, .. } => Mapped::Moved(Path::top(to)),
            Fate::Deleted => Mapped::Gone,
        };
    }
    let j = match fate {
        Fate::Same(j) => {
            return Mapped::Same(Path {
                block: j,
                cells: steps.to_vec(),
            });
        }
        Fate::Formatted(j) | Fate::Modified { to: j, .. } => j,
        Fate::Moved { to, .. } => {
            return Mapped::Moved(Path {
                block: to,
                cells: steps.to_vec(),
            });
        }
        Fate::Deleted => return Mapped::Gone,
    };
    let Some(table) = seq.tables.get(&block) else {
        return Mapped::Unknown;
    };
    if table.structural {
        return Mapped::Unknown;
    }
    let step = steps[0].clone();
    let Some(row_fate) = table.rows.get(step.row).copied() else {
        return Mapped::Gone;
    };
    let rb = match row_fate {
        RowFate::Deleted => return Mapped::Gone,
        RowFate::Same(rb) => {
            let mut cells = vec![CellStep { row: rb, ..step }];
            cells.extend_from_slice(&steps[1..]);
            return Mapped::Same(Path { block: j, cells });
        }
        RowFate::Changed(rb) => rb,
    };
    let Some(column) = step.column else {
        return Mapped::Changed(Path {
            block: j,
            cells: vec![CellStep {
                row: rb,
                column: None,
                block: None,
            }],
        });
    };
    let cell_seq = table.cells.get(&(step.row, column));
    let Some(inner_block) = step.block else {
        // The start of the cell.
        return Mapped::Same(Path {
            block: j,
            cells: vec![CellStep {
                row: rb,
                column: Some(column),
                block: None,
            }],
        });
    };
    wrap(
        j,
        CellStep {
            row: rb,
            column: Some(column),
            block: None,
        },
        map_in(cell_seq, inner_block, &steps[1..]),
    )
}

/// Maps a base path in one story.
#[must_use]
pub fn map_path(story: Option<&StoryDiff>, path: &Path) -> Mapped {
    match story {
        // A story missing from the diff did not change.
        None => Mapped::Same(path.clone()),
        Some(story) => match &story.blocks {
            Some(seq) => map_in(Some(seq), path.block, &path.cells),
            None => Mapped::Gone,
        },
    }
}

/// The sequence (story or cell) a base path lives in, and the index in it.
fn container_seq<'a>(story: &'a StoryDiff, path: &Path) -> Option<(Option<&'a SeqDiff>, usize)> {
    let mut seq = story.blocks.as_ref();
    let mut index = path.block;
    for step in &path.cells {
        let table = seq?.tables.get(&index);
        let Some(table) = table else {
            // An unchanged table: everything inside maps to itself.
            return step.block.map(|b| (None, b));
        };
        let column = step.column?;
        seq = table.cells.get(&(step.row, column));
        index = step.block?;
        if seq.is_none() {
            return Some((None, index));
        }
    }
    Some((seq, index))
}

/// Where new content goes in the other version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InsertPoint {
    /// After this block.
    After(Path),
    /// At the start of the story (`None`) or of this cell (a path whose last
    /// step has no block).
    Start(Option<Path>),
}

/// Where to insert, in the other version, content that follows base `gap` (see
/// [`crate::diff::Change::gap`]): after the nearest earlier base block that
/// stayed in place there, or at the start of the sequence. `None` when the place
/// cannot be found reliably.
#[must_use]
pub fn insert_point(story: Option<&StoryDiff>, gap: Option<&Path>) -> Option<InsertPoint> {
    let Some(gap) = gap else {
        return Some(InsertPoint::Start(None));
    };
    let cell_start = |g: &Path| -> Option<InsertPoint> {
        if g.cells.is_empty() {
            return Some(InsertPoint::Start(None));
        }
        let mut cell = g.clone();
        if let Some(last) = cell.cells.last_mut() {
            last.block = None;
        }
        match map_path(story, &cell) {
            Mapped::Same(p) | Mapped::Changed(p) => Some(InsertPoint::Start(Some(p))),
            _ => None,
        }
    };
    if gap.cells.last().is_some_and(|s| s.block.is_none()) {
        return cell_start(gap);
    }
    let index = match gap.cells.last() {
        Some(last) => last.block?,
        None => gap.block,
    };
    let seq = story
        .and_then(|s| container_seq(s, gap))
        .and_then(|(seq, _)| seq);
    for k in (0..=index).rev() {
        let stayed = seq.is_none_or(|seq| {
            seq.alignment
                .fates
                .get(k)
                .is_some_and(crate::diff::Fate::stayed)
        });
        if !stayed {
            continue;
        }
        let mut base = gap.clone();
        match base.cells.last_mut() {
            Some(last) => last.block = Some(k),
            None => base.block = k,
        }
        return match map_path(story, &base) {
            Mapped::Same(p) | Mapped::Changed(p) => Some(InsertPoint::After(p)),
            _ => None,
        };
    }
    cell_start(gap)
}

/// The block at a path, if there is one.
#[must_use]
pub fn block_at<'d>(docx: &'d Docx, story: &StoryKey, path: &Path) -> Option<&'d Block> {
    let mut blocks = &docx.story(story)?.blocks;
    let mut block = blocks.get(path.block)?;
    for step in &path.cells {
        let Block::Table(table) = block else {
            return None;
        };
        let cell = table.rows.get(step.row)?.cells.get(step.column?)?;
        blocks = &cell.blocks;
        block = blocks.get(step.block?)?;
    }
    Some(block)
}

/// The blocks of the sequence a path points into (the story, or a cell).
#[must_use]
pub fn sequence_at<'d>(docx: &'d Docx, story: &StoryKey, path: &Path) -> Option<&'d [Block]> {
    let mut blocks: &[Block] = &docx.story(story)?.blocks;
    let mut index = path.block;
    for step in &path.cells {
        let Block::Table(table) = blocks.get(index)? else {
            return None;
        };
        let cell = table.rows.get(step.row)?.cells.get(step.column?)?;
        blocks = &cell.blocks;
        match step.block {
            Some(b) => index = b,
            None => return Some(blocks),
        }
    }
    Some(blocks)
}
