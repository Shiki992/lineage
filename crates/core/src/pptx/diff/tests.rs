// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_testkit::decks::{edits, ids, pitch_deck};
use lineage_testkit::pptx::Deck as Built;

use super::*;
use crate::pptx::Deck;

fn open(deck: &Built) -> Deck {
    Deck::open(&deck.to_bytes()).expect("opens")
}

fn changes(a: &Built, b: &Built) -> Vec<Change> {
    diff(&open(a), &open(b)).changes
}

fn kinds(changes: &[Change]) -> Vec<(ChangeKind, Unit)> {
    changes.iter().map(|c| (c.kind, c.unit)).collect()
}

fn strip_cids(mut deck: Built) -> Built {
    for slide in &mut deck.slides {
        slide.creation_id = None;
    }
    deck
}

#[test]
fn a_deck_has_no_changes_from_itself() {
    assert!(changes(&pitch_deck(), &pitch_deck()).is_empty());
}

#[test]
fn new_traction_numbers_are_two_changes_on_slide_six() {
    let found = changes(&pitch_deck(), &edits::traction_update(pitch_deck()));
    assert_eq!(
        kinds(&found),
        [
            (ChangeKind::Modify, Unit::Shape),
            (ChangeKind::Modify, Unit::Shape)
        ]
    );
    for change in &found {
        assert_eq!(change.place(), "Slide 6 · Traction");
        assert!(!change.formatting_only);
        assert_eq!(change.by_hand, None);
    }
    let chart = found
        .iter()
        .find(|c| c.shape_kind == Some(ShapeKind::Chart))
        .expect("the chart");
    assert!(
        chart
            .before_text
            .as_deref()
            .is_some_and(|t| t.contains("Q4 2026: 1200"))
    );
    assert!(
        chart
            .after_text
            .as_deref()
            .is_some_and(|t| t.contains("Q4 2026: 2000"))
    );
    let text = found
        .iter()
        .find(|c| c.shape_kind == Some(ShapeKind::Text))
        .expect("the text box");
    let removed: String = text
        .segments
        .iter()
        .filter(|s| s.kind == SegmentKind::Removed)
        .map(|s| s.text.as_str())
        .collect();
    let added: String = text
        .segments
        .iter()
        .filter(|s| s.kind == SegmentKind::Added)
        .map(|s| s.text.as_str())
        .collect();
    assert_eq!((removed.as_str(), added.as_str()), ("1.2M", "2.0M"));
}

#[test]
fn reordering_is_one_move() {
    for (base, moved) in [
        (pitch_deck(), edits::reordered(pitch_deck())),
        (
            strip_cids(pitch_deck()),
            strip_cids(edits::reordered(pitch_deck())),
        ),
    ] {
        let found = changes(&base, &moved);
        assert_eq!(kinds(&found), [(ChangeKind::Move, Unit::Slide)]);
        assert_eq!(found[0].slide_title.as_deref(), Some("Market"));
        // Market now follows Traction (index 5 in the base).
        assert_eq!(found[0].gap, Some(Gap::Slide(Some(5))));
    }
}

#[test]
fn a_duplicated_slide_is_an_added_slide() {
    let found = changes(&pitch_deck(), &edits::duplicated(pitch_deck()));
    assert_eq!(kinds(&found), [(ChangeKind::Insert, Unit::Slide)]);
    assert_eq!(found[0].slide_number, 7);
    assert_eq!(found[0].gap, Some(Gap::Slide(Some(5))));
}

#[test]
fn an_edited_chart_is_one_object_change() {
    let found = changes(&pitch_deck(), &edits::chart_edited(pitch_deck()));
    assert_eq!(kinds(&found), [(ChangeKind::Modify, Unit::Shape)]);
    assert_eq!(found[0].shape_kind, Some(ShapeKind::Chart));
}

#[test]
fn notes_hidden_slides_and_layouts() {
    let found = changes(&pitch_deck(), &edits::notes_edited(pitch_deck()));
    assert_eq!(kinds(&found), [(ChangeKind::Modify, Unit::Notes)]);

    let mut shown = pitch_deck();
    shown.slides.push(edits::appendix(false));
    let mut hidden = pitch_deck();
    hidden.slides.push(edits::appendix(true));
    let found = changes(&pitch_deck(), &hidden);
    assert_eq!(kinds(&found), [(ChangeKind::Insert, Unit::Slide)]);
    let found = changes(&shown, &hidden);
    assert_eq!(kinds(&found), [(ChangeKind::Modify, Unit::Settings)]);
    assert_eq!(
        found[0].settings.as_ref().and_then(|s| s.hidden),
        Some(true)
    );
    assert!(!found[0].formatting_only);

    let found = changes(&pitch_deck(), &edits::layout_changed(pitch_deck()));
    let settings = found
        .iter()
        .find(|c| c.unit == Unit::Settings)
        .expect("layout change");
    assert_eq!(
        settings.settings.as_ref().and_then(|s| s.layout.as_deref()),
        Some("Title Only")
    );
}

#[test]
fn moving_a_picture_and_bold_text_are_formatting() {
    let found = changes(&pitch_deck(), &edits::picture_moved(pitch_deck()));
    assert_eq!(kinds(&found), [(ChangeKind::Format, Unit::Shape)]);
    assert!(found[0].formatting_only && found[0].geometry_only);
    let found = changes(&pitch_deck(), &edits::made_bold(pitch_deck()));
    assert_eq!(kinds(&found), [(ChangeKind::Format, Unit::Shape)]);
    assert!(found[0].formatting_only && !found[0].geometry_only);
}

#[test]
fn removing_a_slide_and_adding_another_with_its_id() {
    let found = changes(&pitch_deck(), &edits::team_removed(pitch_deck()));
    assert_eq!(kinds(&found), [(ChangeKind::Delete, Unit::Slide)]);
    assert_eq!(found[0].slide_title.as_deref(), Some("Team"));
    // Two decks that each added a different slide 264 are not the same slide.
    let found = changes(&edits::added_a(pitch_deck()), &edits::added_b(pitch_deck()));
    assert_eq!(
        kinds(&found),
        [
            (ChangeKind::Insert, Unit::Slide),
            (ChangeKind::Delete, Unit::Slide)
        ]
    );
}

#[test]
fn a_new_slide_that_reuses_a_deleted_id_is_not_the_old_slide() {
    // PowerPoint can give a new slide the id of one just deleted. The creation
    // ids differ, so it is a removal and an addition, not an edit.
    let base = pitch_deck();
    let mut b = edits::team_removed(pitch_deck());
    let mut fresh = lineage_testkit::pptx::Slide::new(
        ids::TEAM,
        lineage_testkit::pptx::Layout::TitleAndContent,
        vec![lineage_testkit::pptx::Shape::Title(vec![
            lineage_testkit::pptx::p("Advisors"),
        ])],
    );
    fresh.creation_id = Some(42);
    fresh.file = 30;
    b.slides.insert(6, fresh);
    let found = changes(&base, &b);
    assert_eq!(
        kinds(&found),
        [
            (ChangeKind::Insert, Unit::Slide),
            (ChangeKind::Delete, Unit::Slide)
        ]
    );
}

#[test]
fn ids_are_stable_and_unique() {
    let a = changes(
        &pitch_deck(),
        &edits::northgate(edits::traction_update(pitch_deck())),
    );
    let b = changes(
        &pitch_deck(),
        &edits::northgate(edits::traction_update(pitch_deck())),
    );
    assert_eq!(a, b);
    let mut ids: Vec<&str> = a.iter().map(|c| c.id.as_str()).collect();
    assert!(ids.iter().all(|id| id.starts_with('c') && id.len() >= 16));
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), a.len());
}

#[test]
fn new_shapes_and_tables_are_found() {
    let found = changes(&pitch_deck(), &edits::links(pitch_deck()));
    assert_eq!(kinds(&found), [(ChangeKind::Insert, Unit::Shape)]);
    assert_eq!(
        found[0].gap,
        Some(Gap::Shape {
            slide: 7,
            after: Some(1)
        })
    );
    let found = changes(&pitch_deck(), &edits::table(pitch_deck()));
    assert_eq!(kinds(&found), [(ChangeKind::Insert, Unit::Slide)]);
    assert_eq!(found[0].by_hand, None, "tables are written");
}

#[test]
fn words_are_marked() {
    let segments = word_segments("Raising $2.5M seed", "Raising $3M seed");
    let kinds: Vec<SegmentKind> = segments.iter().map(|s| s.kind).collect();
    assert_eq!(
        kinds,
        [
            SegmentKind::Same,
            SegmentKind::Removed,
            SegmentKind::Added,
            SegmentKind::Same
        ]
    );
    assert_eq!(segments[1].text, "2.5M");
    assert_eq!(segments[2].text, "3M");
    assert_eq!(word_segments("same", "same").len(), 1);
}
