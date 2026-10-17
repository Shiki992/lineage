// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Slide operations: how to turn the deck open in PowerPoint into another deck
//! with only what Office.js can do to an open presentation (decision 0019).
//! PowerPoint has no API to replace the open file, but it can delete a slide,
//! insert slides from another file (`insertSlidesFromBase64`) and move a slide
//! (`moveTo`). The engine works out the steps; the host only carries them out.
//!
//! Slides are named as Office.js names them, `"<sldId>#<creationId>"`, so the
//! host never looks inside the file. Both decks come from each other through
//! edits that keep slide ids (the splice-only writer, or these operations), so
//! slides are paired by their two ids only, never by content.
//!
//! The order matters: a changed slide is deleted before its new version is
//! inserted, because PowerPoint then gives the inserted slide its old ids
//! (the Phase 4 spike, decision 0018). Inserted while the old one is still
//! there, it would get new ones.

use std::collections::BTreeSet;

use super::{Deck, Slide};
use crate::diff::lcs::longest_increasing;
use crate::error::{Error, Result};

/// One step PowerPoint takes on the open deck.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlideOp {
    /// Delete this slide.
    Delete { slide: String },
    /// Insert this slide of the source deck after `after` (`None`: first).
    Insert {
        slide: String,
        after: Option<String>,
    },
    /// Move this slide so it ends up at `index` (0-based).
    Move { slide: String, index: usize },
}

/// The steps from one deck to another, and the slide order they lead to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SlideOps {
    pub ops: Vec<SlideOp>,
    /// The slides of the target deck, in order, as Office.js names them.
    pub order: Vec<String>,
}

impl SlideOps {
    /// True when there is nothing to do.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// The slides inserted from the source deck.
    #[must_use]
    pub fn inserted(&self) -> Vec<&str> {
        self.ops
            .iter()
            .filter_map(|op| match op {
                SlideOp::Insert { slide, .. } => Some(slide.as_str()),
                _ => None,
            })
            .collect()
    }
}

/// A slide's name in Office.js: `"<sldId>#<creationId>"`. `None` for a slide
/// without a creation id, which Office.js can't name for `sourceSlideIds`.
#[must_use]
pub fn office_id(slide: &Slide) -> Option<String> {
    slide
        .creation_id
        .map(|creation| format!("{}#{creation}", slide.sld_id))
}

fn ids(deck: &Deck) -> Result<Vec<String>> {
    deck.slides
        .iter()
        .map(|slide| {
            office_id(slide).ok_or_else(|| {
                Error::CannotEdit(format!(
                    "slide {} has no creation id, so PowerPoint can't be told which it is",
                    slide.sld_id
                ))
            })
        })
        .collect()
}

/// The steps that turn `from` (the deck PowerPoint holds) into `to`, with
/// slides inserted from `to`:
///
/// 1. delete every slide `to` doesn't have, and every slide whose new version
///    differs (`Slide::full`);
/// 2. insert, in `to`'s order, each slide that isn't there now, after the
///    slide before it in `to`;
/// 3. move the slides that are out of order: those off the longest run
///    already in order, each after the slide before it in `to`.
///
/// # Errors
/// [`Error::CannotEdit`] when a slide has no creation id, or a deck names a
/// slide twice.
pub fn slide_ops(from: &Deck, to: &Deck) -> Result<SlideOps> {
    let (from_ids, to_ids) = (ids(from)?, ids(to)?);
    for list in [&from_ids, &to_ids] {
        let unique: BTreeSet<&String> = list.iter().collect();
        if unique.len() != list.len() {
            return Err(Error::CannotEdit("a slide is listed twice".to_owned()));
        }
    }
    let mut ops = Vec::new();
    let mut current = from_ids.clone();

    // 1. Deletions, the old versions of changed slides included.
    for (slide, id) in from.slides.iter().zip(&from_ids) {
        let keep = to_ids
            .iter()
            .position(|t| t == id)
            .is_some_and(|j| to.slides[j].full == slide.full);
        if !keep {
            ops.push(SlideOp::Delete { slide: id.clone() });
            current.retain(|c| c != id);
        }
    }

    // 2. Insertions in the target order. Every earlier target slide is there by
    //    now: it was kept, or inserted on an earlier turn.
    for (j, id) in to_ids.iter().enumerate() {
        if current.contains(id) {
            continue;
        }
        let after = j.checked_sub(1).map(|k| to_ids[k].clone());
        let at =
            match &after {
                Some(previous) => {
                    current.iter().position(|c| c == previous).ok_or_else(|| {
                        Error::CannotEdit("lost a slide while inserting".to_owned())
                    })? + 1
                }
                None => 0,
            };
        current.insert(at, id.clone());
        ops.push(SlideOp::Insert {
            slide: id.clone(),
            after,
        });
    }

    // 3. Moves: the slides off the longest run already in target order.
    let target = |id: &String| to_ids.iter().position(|t| t == id).unwrap_or(usize::MAX);
    let pairs: Vec<(usize, usize)> = current
        .iter()
        .enumerate()
        .map(|(i, id)| (i, target(id)))
        .collect();
    let stay: BTreeSet<usize> = longest_increasing(&pairs)
        .into_iter()
        .map(|(_, j)| j)
        .collect();
    for (j, id) in to_ids.iter().enumerate() {
        if stay.contains(&j) {
            continue;
        }
        let from_at = current
            .iter()
            .position(|c| c == id)
            .ok_or_else(|| Error::CannotEdit("lost a slide while moving".to_owned()))?;
        current.remove(from_at);
        let index = match j.checked_sub(1) {
            Some(k) => {
                current
                    .iter()
                    .position(|c| *c == to_ids[k])
                    .ok_or_else(|| Error::CannotEdit("lost a slide while moving".to_owned()))?
                    + 1
            }
            None => 0,
        };
        current.insert(index, id.clone());
        if index != from_at {
            ops.push(SlideOp::Move {
                slide: id.clone(),
                index,
            });
        }
    }
    if current != to_ids {
        return Err(Error::CannotEdit(
            "the slide steps don't lead to the deck".to_owned(),
        ));
    }
    Ok(SlideOps { ops, order: to_ids })
}

/// Carries out `ops` on a list of slide names, as PowerPoint would. For tests
/// and the property test: what the host is expected to end with.
///
/// # Errors
/// [`Error::CannotEdit`] when a step names a slide that isn't there.
pub fn simulate(start: &[String], ops: &[SlideOp]) -> Result<Vec<String>> {
    let missing = |id: &str| Error::CannotEdit(format!("no slide {id}"));
    let mut current = start.to_vec();
    for op in ops {
        match op {
            SlideOp::Delete { slide } => {
                let at = current
                    .iter()
                    .position(|c| c == slide)
                    .ok_or_else(|| missing(slide))?;
                current.remove(at);
            }
            SlideOp::Insert { slide, after } => {
                if current.contains(slide) {
                    return Err(Error::CannotEdit(format!("{slide} is already there")));
                }
                let at = match after {
                    Some(previous) => {
                        current
                            .iter()
                            .position(|c| c == previous)
                            .ok_or_else(|| missing(previous))?
                            + 1
                    }
                    None => 0,
                };
                current.insert(at, slide.clone());
            }
            SlideOp::Move { slide, index } => {
                let at = current
                    .iter()
                    .position(|c| c == slide)
                    .ok_or_else(|| missing(slide))?;
                let moved = current.remove(at);
                current.insert((*index).min(current.len()), moved);
            }
        }
    }
    Ok(current)
}

#[cfg(test)]
mod tests;
