// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Three-way merge of decks. The unit is a shape, a slide's notes, settings or
//! animations, a whole slide (added, removed) or a slide's place (moved). A
//! change is clean only when the other side left its unit alone; anything both
//! sides touched is an overlap the person decides (BLUEPRINT §4).

use super::Deck;
use super::diff::{Change, DeckDiff, Gap, Unit, diff, slide_content};
use crate::diff::{ByHand, ChangeKind};
use crate::merge::{Decision, OverlapKind, Status, choices};

/// One change from `theirs`, with what the merge found for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub change: Change,
    pub status: Status,
    /// This version's change to the same unit, for overlaps.
    pub mine: Option<Change>,
    /// For overlaps: whether "use the master's" can be written.
    pub masters_writable: bool,
}

impl Item {
    /// The choices the panel offers for this item.
    #[must_use]
    pub fn choices(&self) -> Vec<Decision> {
        choices(&self.status, self.masters_writable)
    }
}

/// The result of merging two decks that share `base`.
#[derive(Clone, Debug)]
pub struct Plan<'d> {
    pub base: &'d Deck,
    pub ours: &'d Deck,
    pub theirs: &'d Deck,
    pub ours_diff: DeckDiff,
    pub theirs_diff: DeckDiff,
    pub items: Vec<Item>,
}

impl Plan<'_> {
    #[must_use]
    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.change.id == id)
    }
}

/// What a change touches, in the base deck's terms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Key {
    /// The whole slide: removed.
    Slide(usize),
    /// A slide's place: moved.
    Order(usize),
    Shape(usize, usize),
    ShapeGap(usize, Option<usize>),
    Notes(usize),
    Settings(usize),
    Animations(usize),
    SlideGap(Option<usize>),
    Design,
}

impl Key {
    /// The base slide the key is on, if any.
    fn slide(self) -> Option<usize> {
        match self {
            Self::Slide(s)
            | Self::Order(s)
            | Self::Shape(s, _)
            | Self::ShapeGap(s, _)
            | Self::Notes(s)
            | Self::Settings(s)
            | Self::Animations(s) => Some(s),
            Self::SlideGap(_) | Self::Design => None,
        }
    }
}

pub(crate) fn key(change: &Change) -> Key {
    let before = change.before;
    let slide = before.map(|b| b.slide);
    match (change.unit, change.kind) {
        (Unit::Design, _) => Key::Design,
        (Unit::Slide, ChangeKind::Insert) => match change.gap {
            Some(Gap::Slide(after)) => Key::SlideGap(after),
            _ => Key::SlideGap(None),
        },
        (Unit::Slide, ChangeKind::Move) => Key::Order(slide.unwrap_or(0)),
        (Unit::Slide, _) => Key::Slide(slide.unwrap_or(0)),
        (Unit::Shape, ChangeKind::Insert) => match change.gap {
            Some(Gap::Shape { slide, after }) => Key::ShapeGap(slide, after),
            _ => Key::ShapeGap(0, None),
        },
        (Unit::Shape, _) => Key::Shape(
            slide.unwrap_or(0),
            before.and_then(|b| b.shape).unwrap_or(0),
        ),
        (Unit::Notes, _) => Key::Notes(slide.unwrap_or(0)),
        (Unit::Settings, _) => Key::Settings(slide.unwrap_or(0)),
        (Unit::Animations, _) => Key::Animations(slide.unwrap_or(0)),
    }
}

/// True when both changes lead to the same content.
fn same_result(a: &Change, b: &Change) -> bool {
    a.kind == b.kind
        && match a.kind {
            ChangeKind::Delete => true,
            ChangeKind::Move => a.gap == b.gap,
            _ => {
                a.after_key == b.after_key
                    || (a.after_content != 0 && a.after_content == b.after_content)
            }
        }
}

fn status_for(
    change: &Change,
    plan_ours: &DeckDiff,
    ours: &Deck,
    theirs: &Deck,
) -> (Status, Option<Change>, bool) {
    let k = key(change);
    let mine: Vec<&Change> = plan_ours.changes.iter().filter(|o| key(o) == k).collect();

    // Already there: the same unit changed the same way, or a slide or shape
    // this version also added.
    if let Some(o) = mine.iter().find(|o| same_result(change, o)) {
        let confidence = if o.after_key == change.after_key {
            100
        } else {
            95
        };
        return (
            Status::AlreadyThere { confidence },
            Some((*o).clone()),
            false,
        );
    }
    if change.kind == ChangeKind::Insert {
        let found = plan_ours.changes.iter().find(|o| {
            o.kind == ChangeKind::Insert
                && o.unit == change.unit
                && o.after_content != 0
                && o.after_content == change.after_content
        });
        if let Some(o) = found {
            return (
                Status::AlreadyThere { confidence: 95 },
                Some(o.clone()),
                false,
            );
        }
    }
    if let Some(reason) = change.by_hand {
        return (
            Status::ByHand(reason),
            mine.first().map(|c| (*c).clone()),
            false,
        );
    }

    // The master removed a slide this version changed.
    if let Key::Slide(s) = k
        && change.kind == ChangeKind::Delete
        && let Some(o) = plan_ours
            .changes
            .iter()
            .find(|o| key(o).slide() == Some(s) && !matches!(key(o), Key::Order(_)))
    {
        return (
            Status::Overlap(OverlapKind::DeletedVsEdited),
            Some(o.clone()),
            true,
        );
    }
    // The master changed something on a slide this version removed.
    if let Some(s) = k.slide()
        && let Some(o) = plan_ours
            .changes
            .iter()
            .find(|o| key(o) == Key::Slide(s) && o.kind == ChangeKind::Delete)
    {
        return (
            Status::Overlap(OverlapKind::EditedVsDeleted),
            Some(o.clone()),
            false,
        );
    }
    if let Some(o) = mine.first() {
        let kind = match (change.kind, o.kind) {
            (ChangeKind::Delete, _) => OverlapKind::DeletedVsEdited,
            (_, ChangeKind::Delete) => OverlapKind::EditedVsDeleted,
            (ChangeKind::Insert, ChangeKind::Insert) if change.unit == Unit::Slide => {
                OverlapKind::BothAdded
            }
            // Two new shapes on one slide don't touch each other.
            (ChangeKind::Insert, ChangeKind::Insert) => {
                return writable(change, ours, theirs).map_or_else(
                    |reason| (Status::ByHand(reason), None, false),
                    |()| (Status::Clean, None, false),
                );
            }
            _ if change.formatting_only != o.formatting_only => OverlapKind::FormattingVsText,
            _ => OverlapKind::BothEdited,
        };
        let masters_writable =
            kind != OverlapKind::EditedVsDeleted && writable(change, ours, theirs).is_ok();
        return (Status::Overlap(kind), Some((*o).clone()), masters_writable);
    }
    match writable(change, ours, theirs) {
        Ok(()) => (Status::Clean, None, false),
        Err(reason) => (Status::ByHand(reason), None, false),
    }
}

/// Whether this version can take the change as Lineage writes it.
fn writable(change: &Change, ours: &Deck, theirs: &Deck) -> Result<(), ByHand> {
    let theirs_slide = change.after.and_then(|a| theirs.slides.get(a.slide));
    match change.unit {
        Unit::Slide if change.kind == ChangeKind::Insert => {
            let slide = theirs_slide.ok_or(ByHand::Placement)?;
            if let Some(layout) = &slide.layout
                && ours.layout_like(layout).is_none()
            {
                return Err(ByHand::Layout);
            }
            if slide.notes.is_some() && ours.notes_master.is_none() {
                return Err(ByHand::Layout);
            }
            Ok(())
        }
        Unit::Settings => {
            let layout = change.settings.as_ref().and_then(|s| s.layout.as_ref());
            if layout.is_some()
                && let Some(l) = theirs_slide.and_then(|s| s.layout.as_ref())
                && ours.layout_like(l).is_none()
            {
                return Err(ByHand::Layout);
            }
            Ok(())
        }
        Unit::Notes => {
            if ours.notes_master.is_none() {
                return Err(ByHand::Layout);
            }
            Ok(())
        }
        Unit::Shape | Unit::Slide => Ok(()),
        Unit::Design => Err(ByHand::Design),
        Unit::Animations => Err(ByHand::Animation),
    }
}

/// Merges `theirs` into `ours`, both made from `base`.
#[must_use]
pub fn plan<'d>(base: &'d Deck, ours: &'d Deck, theirs: &'d Deck) -> Plan<'d> {
    let ours_diff = diff(base, ours);
    let theirs_diff = diff(base, theirs);
    let items = theirs_diff
        .changes
        .iter()
        .map(|change| {
            let (status, mine, masters_writable) = status_for(change, &ours_diff, ours, theirs);
            Item {
                change: change.clone(),
                status,
                mine,
                masters_writable,
            }
        })
        .collect();
    Plan {
        base,
        ours,
        theirs,
        ours_diff,
        theirs_diff,
        items,
    }
}

/// True when `ours` already has every slide of `theirs` with the same content,
/// in the same order: nothing left to bring in.
#[must_use]
pub fn same_content(ours: &Deck, theirs: &Deck) -> bool {
    ours.slides.len() == theirs.slides.len()
        && ours
            .slides
            .iter()
            .zip(&theirs.slides)
            .all(|(a, b)| slide_content(a) == slide_content(b))
}

#[cfg(test)]
mod tests;
