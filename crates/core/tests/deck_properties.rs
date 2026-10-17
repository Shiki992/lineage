// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Property tests for decks, on random edits of the pitch deck:
//!
//! - merge(base, x, base) offers nothing.
//! - merge(base, base, x): bringing every change in that Lineage can write
//!   leaves nothing on offer but changes to bring in by hand.
//! - Whatever the person decides on two edited decks, the written deck holds
//!   together (every relationship, content type and id) and re-reads.
//! - diff(x, x) is empty.

use lineage_core::Deck;
use lineage_core::merge::{Decision, Status};
use lineage_core::pptx::check::structure;
use lineage_core::pptx::diff::diff;
use lineage_core::pptx::merge::plan;
use lineage_core::pptx::write::apply;
use lineage_testkit::decks::{pitch_deck, traction_data};
use lineage_testkit::pptx::{Deck as Built, Layout, Shape, Slide, p};
use proptest::prelude::*;

const WORDS: &[&str] = &[
    "lenders", "ledger", "nightly", "breaks", "close", "audit", "pilots", "revenue", "seed",
    "runway", "hires", "market", "growth", "churn", "margin",
];

#[derive(Clone, Debug)]
enum Edit {
    Retitle(usize, Vec<&'static str>),
    Bullets(usize, Vec<&'static str>),
    Add(usize, Vec<&'static str>),
    Remove(usize),
    Move(usize, usize),
    Hide(usize),
    Notes(usize, Vec<&'static str>),
    Chart(i64),
}

fn words() -> impl Strategy<Value = Vec<&'static str>> {
    prop::collection::vec(prop::sample::select(WORDS), 1..5)
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        (0..12_usize, words()).prop_map(|(i, w)| Edit::Retitle(i, w)),
        (0..12_usize, words()).prop_map(|(i, w)| Edit::Bullets(i, w)),
        (0..12_usize, words()).prop_map(|(i, w)| Edit::Add(i, w)),
        (0..12_usize).prop_map(Edit::Remove),
        (0..12_usize, 0..12_usize).prop_map(|(a, b)| Edit::Move(a, b)),
        (0..12_usize).prop_map(Edit::Hide),
        (0..12_usize, words()).prop_map(|(i, w)| Edit::Notes(i, w)),
        (1..5000_i64).prop_map(Edit::Chart),
    ]
}

/// Applies edits. `salt` makes each side's new slides look like PowerPoint's:
/// fresh ids and their own random creation ids.
fn edited(edits: &[Edit], salt: u32) -> Built {
    let mut deck = pitch_deck();
    for edit in edits {
        let n = deck.slides.len();
        match edit {
            Edit::Retitle(i, w) => {
                let slide = &mut deck.slides[i % n];
                if let Some(title) = slide
                    .shapes
                    .iter_mut()
                    .find(|s| matches!(s.shape, Shape::Title(_)))
                {
                    title.shape = Shape::Title(vec![p(&w.join(" "))]);
                }
            }
            Edit::Bullets(i, w) => {
                let slide = &mut deck.slides[i % n];
                if let Some(body) = slide
                    .shapes
                    .iter_mut()
                    .find(|s| matches!(s.shape, Shape::Body { .. }))
                {
                    body.shape = Shape::Body {
                        idx: 1,
                        paras: vec![p(&w.join(" "))],
                    };
                }
            }
            Edit::Add(i, w) => {
                let id = deck.slides.iter().map(|s| s.id).max().unwrap_or(255) + 1;
                let file = deck.slides.iter().map(|s| s.file).max().unwrap_or(0) + 1;
                let mut slide = Slide::new(
                    id,
                    Layout::TitleAndContent,
                    vec![
                        Shape::Title(vec![p(&w.join(" "))]),
                        Shape::Body {
                            idx: 1,
                            paras: vec![p(&w.join(", "))],
                        },
                    ],
                );
                slide.file = file;
                slide.creation_id = Some(salt.wrapping_mul(7_777_777).wrapping_add(id));
                deck.slides.insert(i % (n + 1), slide);
            }
            Edit::Remove(i) => {
                if n > 1 {
                    deck.slides.remove(i % n);
                }
            }
            Edit::Move(a, b) => {
                let slide = deck.slides.remove(a % n);
                deck.slides.insert(b % n, slide);
            }
            Edit::Hide(i) => deck.slides[i % n].hidden = !deck.slides[i % n].hidden,
            Edit::Notes(i, w) => deck.slides[i % n].notes = Some(vec![w.join(" ")]),
            Edit::Chart(value) => {
                for slide in &mut deck.slides {
                    for placed in &mut slide.shapes {
                        if let Shape::Chart { data, .. } = &mut placed.shape {
                            *data = traction_data([310, 560, 840, *value]);
                        }
                    }
                }
            }
        }
    }
    deck
}

fn open(deck: &Built) -> Deck {
    Deck::open(&deck.to_bytes()).expect("opens")
}

fn cases(default: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(64), ..ProptestConfig::default() })]

    #[test]
    fn a_deck_has_no_changes_from_itself(edits in prop::collection::vec(edit(), 0..6)) {
        let deck = open(&edited(&edits, 1));
        prop_assert!(diff(&deck, &deck).changes.is_empty());
    }

    #[test]
    fn a_master_that_is_the_base_offers_nothing(edits in prop::collection::vec(edit(), 0..6)) {
        let base = open(&pitch_deck());
        let ours = open(&edited(&edits, 2));
        prop_assert!(plan(&base, &ours, &base).items.is_empty());
    }

    #[test]
    fn bringing_everything_in_leaves_nothing_on_offer(edits in prop::collection::vec(edit(), 1..6)) {
        let base = open(&pitch_deck());
        let theirs = open(&edited(&edits, 1));
        let first = plan(&base, &base, &theirs);
        let decisions: Vec<(String, Decision)> = first
            .items
            .iter()
            .filter(|i| i.status == Status::Clean)
            .map(|i| (i.change.id.clone(), Decision::BringIn))
            .collect();
        let out = apply(&first, &decisions).expect("writes");
        let result = structure(&out).expect("holds together");
        let again = plan(&base, &result, &theirs);
        for item in &again.items {
            prop_assert!(
                matches!(item.status, Status::AlreadyThere { .. } | Status::ByHand(_)),
                "{:?} {:?}", item.status, item.change.kind
            );
        }
    }

    #[test]
    fn whatever_is_decided_the_deck_holds_together(
        mine in prop::collection::vec(edit(), 1..5),
        master in prop::collection::vec(edit(), 1..5),
        use_masters in any::<bool>(),
    ) {
        let base = open(&pitch_deck());
        let ours = open(&edited(&mine, 2));
        let theirs = open(&edited(&master, 1));
        let merged = plan(&base, &ours, &theirs);
        let decisions: Vec<(String, Decision)> = merged
            .items
            .iter()
            .filter_map(|i| {
                let choices = i.choices();
                let pick = if use_masters && choices.contains(&Decision::UseMasters) {
                    Decision::UseMasters
                } else {
                    *choices.first()?
                };
                Some((i.change.id.clone(), pick))
            })
            .collect();
        let out = apply(&merged, &decisions).expect("writes");
        structure(&out).expect("holds together");
    }
}
