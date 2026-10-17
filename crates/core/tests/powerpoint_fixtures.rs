// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Decks saved and edited by real PowerPoint (fixtures/pptx/powerpoint/, made
//! by scripts/powerpoint-resave.ps1 from the synthetic pitch deck). What
//! PowerPoint writes must be matched the way the synthetic decks are, with
//! nothing PowerPoint renews on every save counted as a change, and changes
//! written into PowerPoint's decks must hold together.
//!
//! With `LINEAGE_SMOKE_DIR` set, the merged decks are written there for
//! scripts/powerpoint-smoke.ps1 -Compare.

use std::path::PathBuf;

use lineage_core::Deck;
use lineage_core::diff::ChangeKind;
use lineage_core::merge::{Decision, Status};
use lineage_core::pptx::check::{landed, structure};
use lineage_core::pptx::diff::{Unit, diff};
use lineage_core::pptx::merge::plan;
use lineage_core::pptx::slides::{SlideOp, simulate, slide_ops};
use lineage_core::pptx::summary::summarize;
use lineage_core::pptx::write::apply;

fn bytes(dir: &str, name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/pptx")
        .join(dir)
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn open(dir: &str, name: &str) -> Deck {
    Deck::open(&bytes(dir, name)).expect("opens")
}

/// A change as expected: its kind, unit and summary.
type Expected = (ChangeKind, Unit, &'static str);

fn saved() -> Deck {
    open("powerpoint", "pitch-deck-saved.pptx")
}

fn changes(a: &Deck, b: &Deck) -> Vec<(ChangeKind, Unit, String)> {
    diff(a, b)
        .changes
        .iter()
        .map(|c| (c.kind, c.unit, summarize(c)))
        .collect()
}

fn smoke(name: &str, bytes: &[u8]) {
    let Some(dir) = std::env::var_os("LINEAGE_SMOKE_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("folder");
    std::fs::write(dir.join(format!("{name}.pptx")), bytes).expect("writes");
    let deck = Deck::open(bytes).expect("opens");
    let mut lines = Vec::new();
    for (i, slide) in deck.slides.iter().enumerate() {
        lines.push(format!(
            "# Slide {}{}",
            i + 1,
            if slide.hidden { " (hidden)" } else { "" }
        ));
        for shape in slide
            .shapes
            .iter()
            .filter(|s| s.kind == lineage_core::pptx::ShapeKind::Text)
        {
            lines.extend(
                shape
                    .paras
                    .iter()
                    .map(|p| p.text.clone())
                    .filter(|t| !t.trim().is_empty()),
            );
        }
    }
    std::fs::write(dir.join(format!("{name}.powerpoint.txt")), lines.join("\n")).expect("writes");
}

#[test]
fn powerpoint_saving_a_deck_changes_nothing_a_person_made() {
    // Every PowerPoint save renews chart series ids: not a change.
    let again = open("powerpoint", "pitch-deck-comment.pptx");
    assert_eq!(
        changes(&saved(), &again),
        [],
        "a comment is not a change to the slides"
    );
    // The first save of a deck another app wrote: PowerPoint rewrites the
    // design parts and the chart's markup, never its figures or text.
    let first = changes(&open("synthetic", "pitch-deck.pptx"), &saved());
    assert!(
        first
            .iter()
            .all(|(kind, unit, _)| *kind == ChangeKind::Format || *unit == Unit::Design),
        "{first:?}"
    );
}

#[test]
fn edits_made_in_powerpoint_are_found_as_made() {
    let cases: [(&str, &[Expected]); 7] = [
        (
            "pitch-deck-duplicated.pptx",
            &[(ChangeKind::Insert, Unit::Slide, "Added slide 7 · Traction")],
        ),
        (
            "pitch-deck-new-slide.pptx",
            &[(ChangeKind::Insert, Unit::Slide, "Added slide 8 · Pricing")],
        ),
        (
            "pitch-deck-reused.pptx",
            &[(
                ChangeKind::Insert,
                Unit::Slide,
                "Added slide 8 · Why Northgate",
            )],
        ),
        (
            "pitch-deck-moved.pptx",
            &[(ChangeKind::Move, Unit::Slide, "Moved slide 6 · Market")],
        ),
        (
            "pitch-deck-hidden.pptx",
            &[(ChangeKind::Modify, Unit::Settings, "Hid this slide")],
        ),
        (
            "pitch-deck-text-edited.pptx",
            &[(
                ChangeKind::Modify,
                Unit::Shape,
                "'$1.2M' changed to '$2.0M'",
            )],
        ),
        (
            "pitch-deck-notes.pptx",
            &[(
                ChangeKind::Modify,
                Unit::Notes,
                "Added speaker notes: 'Lead with the nine-day close.'",
            )],
        ),
    ];
    for (name, want) in cases {
        let got = changes(&saved(), &open("powerpoint", name));
        let want: Vec<(ChangeKind, Unit, String)> = want
            .iter()
            .map(|(k, u, s)| (*k, *u, (*s).to_owned()))
            .collect();
        assert_eq!(got, want, "{name}");
    }
}

#[test]
fn powerpoints_ids_keep_their_meaning() {
    // A slide keeps its id and creation id through saves; a duplicate or a
    // reused slide gets new ones, so it is never taken for its original.
    let saved = saved();
    let duplicated = open("powerpoint", "pitch-deck-duplicated.pptx");
    let traction = saved.slides[5].creation_id;
    assert!(traction.is_some());
    assert_eq!(duplicated.slides[5].creation_id, traction);
    assert_ne!(duplicated.slides[6].creation_id, traction);
    assert_ne!(duplicated.slides[6].sld_id, saved.slides[5].sld_id);
}

#[test]
fn powerpoints_changes_merge_into_a_powerpoint_deck() {
    // The version: PowerPoint's own edit of the traction text. The master
    // gains a duplicated slide, a new slide, a hidden slide, a move, notes.
    let base = saved();
    let ours = open("powerpoint", "pitch-deck-text-edited.pptx");
    for (n, name) in [
        "pitch-deck-duplicated.pptx",
        "pitch-deck-new-slide.pptx",
        "pitch-deck-reused.pptx",
        "pitch-deck-moved.pptx",
        "pitch-deck-hidden.pptx",
        "pitch-deck-notes.pptx",
    ]
    .iter()
    .enumerate()
    {
        let theirs = open("powerpoint", name);
        let merged = plan(&base, &ours, &theirs);
        assert!(
            merged.items.iter().all(|i| i.status == Status::Clean),
            "{name}: {:?}",
            merged.items.iter().map(|i| &i.status).collect::<Vec<_>>()
        );
        let decisions: Vec<(String, Decision)> = merged
            .items
            .iter()
            .map(|i| (i.change.id.clone(), Decision::BringIn))
            .collect();
        let out = apply(&merged, &decisions).expect("writes");
        let result = structure(&out).expect("holds together");
        // The master's change is there, and the version's own edit stays.
        let again = plan(&base, &result, &theirs);
        assert!(
            again
                .items
                .iter()
                .all(|i| matches!(i.status, Status::AlreadyThere { .. })),
            "{name}"
        );
        assert!(
            result.slides.iter().any(|s| s.text().contains("$2.0M ARR")),
            "{name}: the version's own edit stays"
        );
        smoke(&format!("powerpoint-merge-{n}"), &out);
    }
}

/// The open-deck path (decision 0019): a master change merged into PowerPoint's
/// own deck takes one slide's steps, and PowerPoint's deck with the same edit
/// made by hand counts as having landed: nothing PowerPoint renews on save
/// tells them apart.
#[test]
fn a_change_merged_into_a_powerpoint_deck_lands_as_powerpoint_saves_it() {
    for edited in [
        "pitch-deck-text-edited.pptx",
        "pitch-deck-notes.pptx",
        "pitch-deck-moved.pptx",
        "pitch-deck-new-slide.pptx",
    ] {
        let base = saved();
        let theirs = open("powerpoint", edited);
        let merged = plan(&base, &base, &theirs);
        let decisions: Vec<(String, Decision)> = merged
            .items
            .iter()
            .filter(|i| i.status == Status::Clean)
            .map(|i| (i.change.id.clone(), Decision::BringIn))
            .collect();
        let out = Deck::open(&apply(&merged, &decisions).expect("writes")).expect("opens");
        let ops = slide_ops(&base, &out).expect("steps");
        assert!(!ops.ops.is_empty(), "{edited}");
        let start: Vec<String> = base
            .slides
            .iter()
            .filter_map(lineage_core::pptx::slides::office_id)
            .collect();
        assert_eq!(
            simulate(&start, &ops.ops).expect("simulates"),
            ops.order,
            "{edited}"
        );
        let deletes = ops
            .ops
            .iter()
            .filter(|o| matches!(o, SlideOp::Delete { .. }))
            .count();
        assert!(deletes <= 1, "{edited}: {:?}", ops.ops);
        landed(&out, &theirs).unwrap_or_else(|e| panic!("{edited}: {e}"));
    }
}
