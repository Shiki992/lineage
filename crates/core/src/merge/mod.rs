// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Three-way merge: bringing the changes from `base` to `theirs` into `ours`.
//!
//! The unit is a paragraph, a table cell, a table row, a section break or an
//! opaque block (BLUEPRINT §4, decision 0007). A change from `theirs` is applied
//! automatically only when `ours` left its unit alone. When both changed the same
//! unit, the item is an overlap with both sides, and the person chooses. When
//! `ours` already has the change, the item says so (it is detected as merged).
//! Nothing is ever written silently when the engine is unsure.

pub mod locate;

use crate::diff::{ByHand, Change, ChangeKind, DocDiff, Path, Unit, diff, keys::block_keys};
use crate::docx::{Docx, StoryKey};
use locate::{block_at, insert_point};

/// Least confidence for "ours already has this change" to count as merged.
/// Below it, the item is shown to the person instead (decision 0002).
pub const DETECTED_CONFIDENCE: u8 = 90;

/// What the merge found for one change from `theirs`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Status {
    /// Nothing in `ours` touches this unit: it can be brought in.
    Clean,
    /// `ours` already has it, with this confidence.
    AlreadyThere { confidence: u8 },
    /// Both changed the same unit.
    Overlap(OverlapKind),
    /// Shown, but this version cannot write it.
    ByHand(ByHand),
}

/// How the two sides overlap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OverlapKind {
    /// Both edited the same paragraph (or cell, or row).
    BothEdited,
    /// The master deleted what this version edited.
    DeletedVsEdited,
    /// The master edited what this version deleted.
    EditedVsDeleted,
    /// Both added different content in the same place.
    BothAdded,
    /// One side changed only formatting, the other the text.
    FormattingVsText,
}

/// What the person decided for one item.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Decision {
    /// Bring a clean change in.
    BringIn,
    /// Leave it out.
    Decline,
    /// For an overlap: keep this version's side (the same as declining).
    KeepMine,
    /// For an overlap: use the master's side.
    UseMasters,
    /// For "both added here": keep both, in this order.
    KeepBoth { masters_first: bool },
}

impl Decision {
    /// True when the decision brings the master's change in, so the change is
    /// tagged merged rather than declined.
    #[must_use]
    pub fn brings_in(self) -> bool {
        matches!(
            self,
            Self::BringIn | Self::UseMasters | Self::KeepBoth { .. }
        )
    }
}

/// One change from `theirs`, with what the merge found for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub change: Change,
    pub status: Status,
    /// This version's change to the same unit, for overlaps.
    pub mine: Option<Change>,
    /// For overlaps: whether "use the master's version" can be written.
    pub masters_writable: bool,
}

impl Item {
    /// The choices the panel offers for this item.
    #[must_use]
    pub fn choices(&self) -> Vec<Decision> {
        match &self.status {
            Status::Clean => vec![Decision::BringIn, Decision::Decline],
            Status::Overlap(_) if !self.masters_writable => vec![Decision::KeepMine],
            Status::Overlap(OverlapKind::BothAdded) => vec![
                Decision::KeepMine,
                Decision::KeepBoth {
                    masters_first: false,
                },
                Decision::KeepBoth {
                    masters_first: true,
                },
                Decision::UseMasters,
            ],
            Status::Overlap(_) => vec![Decision::KeepMine, Decision::UseMasters],
            Status::AlreadyThere { .. } | Status::ByHand(_) => Vec::new(),
        }
    }
}

/// The result of merging: every change from `theirs`, classified.
#[derive(Clone, Debug)]
pub struct Plan<'d> {
    pub base: &'d Docx,
    pub ours: &'d Docx,
    pub theirs: &'d Docx,
    /// What `ours` changed since `base`.
    pub ours_diff: DocDiff,
    /// What `theirs` changed since `base`: the changes on offer.
    pub theirs_diff: DocDiff,
    pub items: Vec<Item>,
}

impl Plan<'_> {
    #[must_use]
    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.change.id == id)
    }
}

/// A unit a change touches. Changes inside a table cell all touch that cell.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum UnitKey {
    Block(StoryKey, usize),
    Gap(StoryKey, Option<usize>),
    Cell(StoryKey, usize, usize, usize),
    Row(StoryKey, usize, usize),
    RowGap(StoryKey, usize, Option<usize>),
}

fn path_unit(story: &StoryKey, path: &Path, gap: bool, row_unit: bool) -> UnitKey {
    match path.cells.first() {
        None if row_unit && gap => UnitKey::RowGap(story.clone(), path.block, None),
        None if gap => UnitKey::Gap(story.clone(), Some(path.block)),
        None => UnitKey::Block(story.clone(), path.block),
        Some(step) => match step.column {
            Some(column) => UnitKey::Cell(story.clone(), path.block, step.row, column),
            None if gap => UnitKey::RowGap(story.clone(), path.block, Some(step.row)),
            None => UnitKey::Row(story.clone(), path.block, step.row),
        },
    }
}

/// The base units a change touches.
fn units(change: &Change) -> Vec<UnitKey> {
    let story = &change.story;
    let row = change.unit == Unit::Row;
    let mut out = Vec::new();
    if let Some(before) = &change.before {
        out.push(path_unit(story, before, false, false));
    }
    if matches!(change.kind, ChangeKind::Insert | ChangeKind::Move) {
        out.push(match &change.gap {
            Some(gap) => path_unit(story, gap, true, row),
            None => UnitKey::Gap(story.clone(), None),
        });
    }
    out
}

fn overlaps(x: &UnitKey, y: &UnitKey) -> bool {
    use UnitKey::{Block, Cell, Row, RowGap};
    if x == y {
        return true;
    }
    match (x, y) {
        // A change to a whole table touches everything inside it.
        (Block(s, t), Cell(s2, t2, ..) | Row(s2, t2, _) | RowGap(s2, t2, _))
        | (Cell(s2, t2, ..) | Row(s2, t2, _) | RowGap(s2, t2, _), Block(s, t)) => {
            s == s2 && t == t2
        }
        // Deleting a row touches its cells.
        (Row(s, t, r), Cell(s2, t2, r2, _)) | (Cell(s2, t2, r2, _), Row(s, t, r)) => {
            s == s2 && t == t2 && r == r2
        }
        _ => false,
    }
}

fn keys_at(docx: &Docx, story: &StoryKey, path: Option<&Path>) -> Option<crate::diff::keys::Keys> {
    let block = block_at(docx, story, path?)?;
    let src = docx.story(story).and_then(|s| docx.part_text(&s.part).ok());
    Some(block_keys(block, src))
}

/// Row changes have no single block; compare their text instead.
fn row_text(change: &Change) -> Option<&str> {
    change.after_text.as_deref()
}

/// How sure we are that `mine` is the same change as `theirs`, if it is.
fn same_change(plan_ours: &Docx, plan_theirs: &Docx, theirs: &Change, mine: &Change) -> Option<u8> {
    let edits = |k: ChangeKind| matches!(k, ChangeKind::Modify | ChangeKind::Format);
    if theirs.kind != mine.kind && !(edits(theirs.kind) && edits(mine.kind)) {
        return None;
    }
    if theirs.kind == ChangeKind::Delete {
        return (theirs.before == mine.before).then_some(100);
    }
    if theirs.kind != ChangeKind::Insert && theirs.before != mine.before {
        return None;
    }
    if theirs.kind == ChangeKind::Move && theirs.gap != mine.gap {
        return None;
    }
    if theirs.unit == Unit::Row || mine.unit == Unit::Row {
        return (theirs.unit == mine.unit && row_text(theirs) == row_text(mine)).then_some(100);
    }
    let tk = keys_at(plan_theirs, &theirs.story, theirs.after.as_ref())?;
    let mk = keys_at(plan_ours, &mine.story, mine.after.as_ref())?;
    if tk == mk {
        Some(100)
    } else if tk.content == mk.content {
        Some(85)
    } else {
        None
    }
}

fn overlap_kind(theirs: &Change, mine: &Change) -> OverlapKind {
    match (theirs.kind, mine.kind) {
        (ChangeKind::Insert, ChangeKind::Insert | ChangeKind::Move) => OverlapKind::BothAdded,
        (ChangeKind::Delete, _) => OverlapKind::DeletedVsEdited,
        (_, ChangeKind::Delete) => OverlapKind::EditedVsDeleted,
        (ChangeKind::Format, ChangeKind::Modify) | (ChangeKind::Modify, ChangeKind::Format) => {
            OverlapKind::FormattingVsText
        }
        _ => OverlapKind::BothEdited,
    }
}

/// Merges: classifies every change from `base` to `theirs` against `ours`.
#[must_use]
pub fn plan<'d>(base: &'d Docx, ours: &'d Docx, theirs: &'d Docx) -> Plan<'d> {
    let ours_diff = diff(base, ours);
    let theirs_diff = diff(base, theirs);
    let check = crate::write::Check::new(ours, theirs);
    let mut items = Vec::new();
    for change in &theirs_diff.changes {
        let touched = units(change);
        let mine: Vec<&Change> = ours_diff
            .changes
            .iter()
            .filter(|o| {
                units(o)
                    .iter()
                    .any(|u| touched.iter().any(|t| overlaps(u, t)))
            })
            .collect();

        // Does ours already have it? For insertions, look anywhere in the same
        // story: text copied in by hand may sit somewhere else.
        let mut best: Option<(u8, &Change)> = None;
        let candidates: Vec<&Change> = if change.kind == ChangeKind::Insert {
            ours_diff
                .changes
                .iter()
                .filter(|o| o.story == change.story && o.kind == ChangeKind::Insert)
                .collect()
        } else {
            mine.clone()
        };
        for candidate in candidates {
            if let Some(confidence) = same_change(ours, theirs, change, candidate)
                && best.is_none_or(|(c, _)| confidence > c)
            {
                best = Some((confidence, candidate));
            }
        }

        let (status, mine_change) = match best {
            Some((confidence, found)) if confidence >= DETECTED_CONFIDENCE => {
                (Status::AlreadyThere { confidence }, Some(found.clone()))
            }
            _ => {
                if let Some(reason) = change.by_hand {
                    (Status::ByHand(reason), mine.first().map(|m| (*m).clone()))
                } else if let Some(first) = mine.first() {
                    (
                        Status::Overlap(overlap_kind(change, first)),
                        Some((*first).clone()),
                    )
                } else {
                    match check.change(&ours_diff, change) {
                        Ok(()) => (Status::Clean, None),
                        Err(reason) => (Status::ByHand(reason), None),
                    }
                }
            }
        };
        let mut item = Item {
            change: change.clone(),
            status,
            mine: mine_change,
            masters_writable: false,
        };
        item.masters_writable = check.masters(&ours_diff, &item);
        items.push(item);
    }
    Plan {
        base,
        ours,
        theirs,
        ours_diff,
        theirs_diff,
        items,
    }
}

/// Where an insertion from `theirs` would go in `ours`, if it can be placed.
#[must_use]
pub fn insertion_point(plan: &Plan<'_>, change: &Change) -> Option<locate::InsertPoint> {
    insert_point(plan.ours_diff.story(&change.story), change.gap.as_ref())
}

#[cfg(test)]
mod tests;
