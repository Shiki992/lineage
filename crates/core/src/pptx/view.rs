// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! "What changed" for decks, as the cards the panel already draws, and the
//! open change format (`spec/format/v0`, format "pptx").

use super::Deck;
use super::diff::{Change, DeckDiff, Loc, Unit};
use super::summary::summarize;
use crate::diff::ChangeKind;
use crate::diff::spec::{ChangeList, Location, SpecChange};
use crate::view::{ChangeCard, Choice, WhatChanged, by_hand_reason};

/// The card for one change.
#[must_use]
pub fn card(change: &Change, choices: Vec<Choice>) -> ChangeCard {
    ChangeCard {
        id: change.id.clone(),
        summary: summarize(change),
        place: (change.unit != Unit::Design).then(|| change.place()),
        text: change.segments.clone(),
        choices,
        by_hand: change.by_hand.map(|r| by_hand_reason(r).to_owned()),
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// "What changed" between two states of a deck. No decisions: it is a view.
#[must_use]
pub fn from_diff(diff: &DeckDiff) -> WhatChanged {
    let mut view = WhatChanged::default();
    for change in &diff.changes {
        let card = card(change, Vec::new());
        if change.formatting_only {
            view.formatting.push(card);
        } else {
            view.changes.push(card);
        }
    }
    let total = view.changes.len() + view.formatting.len();
    view.headline = if total == 0 {
        "No changes".to_owned()
    } else {
        count(total, "change", "changes")
    };
    view
}

fn location(deck: &Deck, loc: Option<Loc>, change: &Change) -> Location {
    let index = loc.map_or(0, |l| l.slide);
    let slide = deck.slides.get(index);
    let mut location = Location::pptx(
        slide.map_or(deck.main_part.as_str(), |s| s.part.as_str()),
        index,
    );
    if change.unit == Unit::Design {
        location.part.clone_from(&deck.main_part);
        location.target = Some("design".to_owned());
        return location;
    }
    location.slide_id = slide.map(|s| s.sld_id);
    match change.unit {
        Unit::Shape => {
            location.shape_id = loc
                .and_then(|l| slide?.shapes.get(l.shape?))
                .map(|s| s.id)
                .filter(|id| *id != 0);
        }
        unit => location.target = Some(unit.as_str().to_owned()),
    }
    location
}

/// The change list for a deck diff: locations in `after` (in `before` for removals).
#[must_use]
pub fn change_list(diff: &DeckDiff, before: &Deck, after: &Deck) -> ChangeList {
    let changes = diff
        .changes
        .iter()
        .map(|change| {
            let (location, from) = match change.kind {
                ChangeKind::Delete => (location(before, change.before, change), None),
                ChangeKind::Move => (
                    location(after, change.after, change),
                    Some(location(before, change.before, change)),
                ),
                _ if change.unit == Unit::Design => (location(after, None, change), None),
                _ => (location(after, change.after, change), None),
            };
            let summary = summarize(change);
            SpecChange {
                id: change.id.clone(),
                kind: change.kind.as_str().to_owned(),
                location,
                from_location: from,
                before: match change.kind {
                    ChangeKind::Insert => None,
                    _ => Some(change.before_text.clone().unwrap_or_default()),
                },
                after: match change.kind {
                    ChangeKind::Delete => None,
                    _ => Some(change.after_text.clone().unwrap_or_default()),
                },
                formatting_only: change.formatting_only,
                confidence: change.confidence,
                summary: (!summary.is_empty()).then_some(summary),
            }
        })
        .collect();
    ChangeList {
        spec_version: crate::SPEC_VERSION.to_owned(),
        format: "pptx".to_owned(),
        changes,
    }
}

#[cfg(test)]
mod tests {
    use lineage_testkit::decks::{edits, pitch_deck};

    use super::*;
    use crate::pptx::diff::diff;

    #[test]
    fn the_view_sorts_content_from_formatting() {
        let a = Deck::open(&pitch_deck().to_bytes()).expect("opens");
        let b = Deck::open(&edits::picture_moved(edits::traction_update(pitch_deck())).to_bytes())
            .expect("opens");
        let view = from_diff(&diff(&a, &b));
        assert_eq!(view.headline, "3 changes");
        assert_eq!(view.changes.len(), 2);
        assert_eq!(view.formatting.len(), 1);
        assert_eq!(view.changes[0].place.as_deref(), Some("Slide 6 · Traction"));
        assert_eq!(
            view.formatting[0].place.as_deref(),
            Some("Slide 5 · Product")
        );
    }

    #[test]
    fn change_lists_name_slides_and_shapes() {
        let a = Deck::open(&pitch_deck().to_bytes()).expect("opens");
        let b = Deck::open(&edits::northgate(edits::traction_update(pitch_deck())).to_bytes())
            .expect("opens");
        let list = change_list(&diff(&a, &b), &a, &b);
        assert_eq!(list.format, "pptx");
        let text = list
            .changes
            .iter()
            .find(|c| c.after.as_deref() == Some("$2.0M ARR\nNet revenue retention 128%"))
            .expect("the traction text");
        assert_eq!(text.location.slide_id, Some(261));
        assert_eq!(text.location.shape_id, Some(4));
        assert_eq!(text.location.part, "ppt/slides/slide6.xml");
        let added = list
            .changes
            .iter()
            .find(|c| c.kind == "insert")
            .expect("added slide");
        assert_eq!(added.before, None);
        assert_eq!(added.location.target.as_deref(), Some("slide"));
        // Reading it back gives the same list.
        let again: ChangeList = serde_json::from_str(&list.to_json()).expect("parses");
        assert_eq!(again, list);
    }
}
