// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_testkit::decks::{edits, ids, northgate, pitch_deck, reordered, traction_update};
use lineage_testkit::pptx::{Deck as Built, creation_id_for};

use super::*;
use crate::pptx::check::landed;

fn open(deck: &Built) -> Deck {
    Deck::open(&deck.to_bytes()).expect("opens")
}

fn name(id: u32) -> String {
    format!("{id}#{}", creation_id_for(id))
}

fn order(deck: &Deck) -> Vec<String> {
    deck.slides.iter().filter_map(office_id).collect()
}

/// The steps, checked the way the host would carry them out.
fn steps(from: &Deck, to: &Deck) -> SlideOps {
    let ops = slide_ops(from, to).expect("steps");
    assert_eq!(ops.order, order(to));
    assert_eq!(
        simulate(&order(from), &ops.ops).expect("simulates"),
        order(to),
        "{:?}",
        ops.ops
    );
    ops
}

#[test]
fn a_slide_is_named_as_office_js_names_it() {
    let deck = open(&pitch_deck());
    assert_eq!(
        office_id(deck.slide(ids::TRACTION).expect("t")),
        Some(name(ids::TRACTION))
    );
}

#[test]
fn the_same_deck_needs_nothing() {
    let deck = open(&pitch_deck());
    assert!(steps(&deck, &deck).is_empty());
}

#[test]
fn a_changed_slide_is_deleted_then_inserted_after_the_slide_before_it() {
    let from = open(&pitch_deck());
    let to = open(&traction_update(pitch_deck()));
    let ops = steps(&from, &to);
    assert_eq!(
        ops.ops,
        [
            SlideOp::Delete {
                slide: name(ids::TRACTION)
            },
            SlideOp::Insert {
                slide: name(ids::TRACTION),
                after: Some(name(ids::PRODUCT)),
            },
        ]
    );
    assert_eq!(ops.inserted(), [name(ids::TRACTION)]);
}

#[test]
fn a_changed_first_slide_goes_back_at_the_start() {
    let from = open(&pitch_deck());
    let to = open(&northgate(pitch_deck()));
    let ops = steps(&from, &to);
    assert!(ops.ops.contains(&SlideOp::Insert {
        slide: name(ids::TITLE),
        after: None,
    }));
}

#[test]
fn a_new_slide_is_inserted_and_a_removed_one_deleted() {
    let from = open(&pitch_deck());
    let to = open(&edits::added_a(pitch_deck()));
    let added = steps(&from, &to);
    assert_eq!(added.ops.len(), 1);
    assert!(matches!(
        &added.ops[0],
        SlideOp::Insert { after: Some(a), .. } if *a == name(ids::TEAM)
    ));

    let to = open(&edits::team_removed(pitch_deck()));
    assert_eq!(
        steps(&from, &to).ops,
        [SlideOp::Delete {
            slide: name(ids::TEAM)
        }]
    );
}

#[test]
fn a_moved_slide_is_one_move() {
    let from = open(&pitch_deck());
    let to = open(&reordered(pitch_deck()));
    let ops = steps(&from, &to);
    assert_eq!(
        ops.ops,
        [SlideOp::Move {
            slide: name(ids::MARKET),
            index: 5,
        }]
    );
    // And back.
    assert_eq!(steps(&to, &from).ops.len(), 1);
}

#[test]
fn changes_moves_insertions_and_deletions_together() {
    let from = open(&pitch_deck());
    // Northgate's version: a new subtitle and a new slide.
    let to = open(&edits::team_removed(reordered(traction_update(northgate(
        pitch_deck(),
    )))));
    let ops = steps(&from, &to);
    // Deletions come first, so a changed slide keeps its ids when it goes back.
    let first_insert = ops
        .ops
        .iter()
        .position(|o| matches!(o, SlideOp::Insert { .. }))
        .expect("inserts");
    assert!(
        ops.ops[..first_insert]
            .iter()
            .all(|o| matches!(o, SlideOp::Delete { .. }))
    );
    // And the way back is as exact.
    steps(&to, &from);
}

#[test]
fn every_slide_moved_still_reaches_the_order() {
    let from = open(&pitch_deck());
    let mut built = pitch_deck();
    built.slides.reverse();
    let to = open(&built);
    let ops = steps(&from, &to);
    assert!(ops.ops.iter().all(|o| matches!(o, SlideOp::Move { .. })));
    assert!(ops.ops.len() < from.slides.len());
}

#[test]
fn a_slide_without_a_creation_id_is_refused() {
    let from = open(&pitch_deck());
    let mut built = traction_update(pitch_deck());
    for slide in &mut built.slides {
        slide.creation_id = None;
    }
    let to = open(&built);
    assert!(matches!(slide_ops(&from, &to), Err(Error::CannotEdit(_))));
}

#[test]
fn a_step_on_a_missing_slide_is_refused() {
    let start = vec![name(ids::TITLE)];
    let missing = [SlideOp::Delete {
        slide: name(ids::TEAM),
    }];
    assert!(simulate(&start, &missing).is_err());
    let twice = [SlideOp::Insert {
        slide: name(ids::TITLE),
        after: None,
    }];
    assert!(simulate(&start, &twice).is_err());
}

#[test]
fn a_deck_has_landed_when_it_is_the_deck() {
    let deck = open(&traction_update(pitch_deck()));
    landed(&deck, &deck).expect("lands");
    landed(&deck, &open(&traction_update(pitch_deck()))).expect("the same deck again");
}

#[test]
fn other_text_order_ids_or_sections_have_not_landed() {
    let expected = open(&traction_update(pitch_deck()));
    for (what, held) in [
        ("text", open(&pitch_deck())),
        ("order", open(&reordered(traction_update(pitch_deck())))),
        (
            "sections",
            open(&edits::sections(traction_update(pitch_deck()))),
        ),
    ] {
        assert!(
            matches!(landed(&expected, &held), Err(Error::NotLanded(_))),
            "{what}"
        );
    }
    // A slide that came back with new ids, as when inserted before deleting.
    let mut renamed = traction_update(pitch_deck());
    renamed.slide_mut(ids::TRACTION).creation_id = Some(1_780_260_602);
    assert!(matches!(
        landed(&expected, &open(&renamed)),
        Err(Error::NotLanded(_))
    ));
}
