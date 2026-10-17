// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_testkit::decks::{edits, ids, pitch_deck};
use lineage_testkit::pptx::{Deck as Built, Placed, Shape, p};

use super::plan as merge_plan;
use super::*;
use crate::merge::{Decision, OverlapKind, Status};
use crate::pptx::check::structure;
use crate::pptx::write::apply;

fn open(deck: &Built) -> Deck {
    Deck::open(&deck.to_bytes()).expect("opens")
}

struct Three {
    base: Deck,
    ours: Deck,
    theirs: Deck,
}

fn three(base: &Built, ours: &Built, theirs: &Built) -> Three {
    Three {
        base: open(base),
        ours: open(ours),
        theirs: open(theirs),
    }
}

fn statuses(plan: &Plan<'_>) -> Vec<Status> {
    plan.items.iter().map(|i| i.status.clone()).collect()
}

/// Applies `decision` to every item that offers it, else the first choice.
fn decide_all(plan: &Plan<'_>, decision: Decision) -> Vec<(String, Decision)> {
    plan.items
        .iter()
        .filter_map(|item| {
            let choices = item.choices();
            let pick = if choices.contains(&decision) {
                decision
            } else {
                *choices.first()?
            };
            Some((item.change.id.clone(), pick))
        })
        .collect()
}

/// Applies the decisions, and keeps the result for the smoke tests.
#[track_caller]
fn written(plan: &Plan<'_>, decisions: &[(String, Decision)]) -> Vec<u8> {
    let out = apply(plan, decisions).expect("writes");
    static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let line = std::panic::Location::caller().line();
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    crate::testutil::smoke_deck(&format!("deck-merge-{line}-{n}"), &out);
    out
}

fn titles(deck: &Deck) -> Vec<String> {
    deck.slides
        .iter()
        .map(|s| s.title().unwrap_or_default())
        .collect()
}

fn traction_text(deck: &Deck) -> String {
    let slide = deck
        .slides
        .iter()
        .find(|s| s.title().as_deref() == Some("Traction"))
        .expect("traction");
    slide
        .shapes
        .iter()
        .find(|s| s.kind == ShapeKind::Text && s.text().contains("ARR"))
        .expect("box")
        .text()
}

use crate::pptx::model::ShapeKind;

#[test]
fn new_numbers_reach_a_tailored_version() {
    let base = pitch_deck();
    let t = three(
        &base,
        &edits::northgate(pitch_deck()),
        &edits::traction_update(pitch_deck()),
    );
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    assert_eq!(statuses(&plan), [Status::Clean, Status::Clean]);
    let decisions = decide_all(&plan, Decision::BringIn);
    let out = written(&plan, &decisions);
    let result = structure(&out).expect("holds together");
    // Theirs' numbers, ours' subtitle and extra slide.
    assert_eq!(traction_text(&result), traction_text(&t.theirs));
    assert_eq!(titles(&result), titles(&t.ours));
    assert_eq!(
        result.slides[0].shapes[1].text(),
        "Prepared for Northgate Capital"
    );
    let chart = |deck: &Deck| {
        deck.slides
            .iter()
            .flat_map(|s| &s.shapes)
            .find(|s| s.kind == ShapeKind::Chart)
            .map(|s| s.text())
    };
    assert_eq!(chart(&result), chart(&t.theirs));
    // Nothing else moved: every other slide is byte-identical in content keys.
    for (a, b) in t.ours.slides.iter().zip(&result.slides) {
        if a.title().as_deref() != Some("Traction") {
            assert_eq!(a.full, b.full, "{:?}", a.title());
        }
    }
    // The old chart and its workbook are gone, the new ones are there once.
    let charts = result
        .package
        .part_names()
        .filter(|n| n.starts_with("ppt/charts/chart"))
        .count();
    let books = result
        .package
        .part_names()
        .filter(|n| n.starts_with("ppt/embeddings/"))
        .count();
    assert_eq!((charts, books), (1, 1));
    // The rest of the version's parts are byte-identical.
    for name in t.ours.package.part_names() {
        let lower = name.to_ascii_lowercase();
        if lower.contains("slide6")
            || lower.contains("chart")
            || lower.contains("embeddings")
            || lower == "[content_types].xml"
        {
            continue;
        }
        assert_eq!(
            t.ours.package.part(name),
            result.package.part(name),
            "{name}"
        );
    }
}

#[test]
fn both_changed_the_numbers_and_the_person_chooses() {
    let mut mine = pitch_deck();
    for placed in &mut mine.slide_mut(ids::TRACTION).shapes {
        if let Shape::TextBox { .. } = placed.shape {
            placed.shape = lineage_testkit::decks::traction_box("$1.3M ARR");
        }
    }
    let t = three(&pitch_deck(), &mine, &edits::traction_update(pitch_deck()));
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    let text_item = plan
        .items
        .iter()
        .find(|i| i.change.shape_kind == Some(ShapeKind::Text))
        .expect("text");
    assert_eq!(text_item.status, Status::Overlap(OverlapKind::BothEdited));
    assert_eq!(
        text_item.choices(),
        [Decision::KeepMine, Decision::UseMasters]
    );
    // Bringing in an overlap without a side is refused.
    assert!(apply(&plan, &[(text_item.change.id.clone(), Decision::BringIn)]).is_err());

    let keep = decide_all(&plan, Decision::KeepMine);
    let kept = structure(&written(&plan, &keep)).expect("holds");
    assert!(traction_text(&kept).contains("$1.3M ARR"));
    let decisions: Vec<(String, Decision)> = plan
        .items
        .iter()
        .map(|i| {
            let d = if i.choices().contains(&Decision::UseMasters) {
                Decision::UseMasters
            } else {
                Decision::BringIn
            };
            (i.change.id.clone(), d)
        })
        .collect();
    let used = structure(&written(&plan, &decisions)).expect("holds");
    assert_eq!(traction_text(&used), traction_text(&t.theirs));
}

#[test]
fn a_new_slide_a_removed_slide_and_a_move() {
    // Theirs adds Why Northgate (8th), ours reorders nothing.
    let t = three(
        &pitch_deck(),
        &pitch_deck(),
        &edits::northgate(pitch_deck()),
    );
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    let out = written(&plan, &decide_all(&plan, Decision::BringIn));
    let result = structure(&out).expect("holds");
    assert_eq!(titles(&result), titles(&t.theirs));
    assert!(result.slides.iter().all(|s| s.layout.is_some()));

    let t = three(
        &pitch_deck(),
        &edits::northgate(pitch_deck()),
        &edits::team_removed(pitch_deck()),
    );
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    let out = written(&plan, &decide_all(&plan, Decision::BringIn));
    let result = structure(&out).expect("holds");
    assert!(!titles(&result).contains(&"Team".to_owned()));
    assert_eq!(result.slides.len(), t.ours.slides.len() - 1);
    assert!(!result.package.contains("ppt/slides/slide7.xml"));

    let t = three(
        &pitch_deck(),
        &edits::northgate(pitch_deck()),
        &edits::reordered(pitch_deck()),
    );
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    let out = written(&plan, &decide_all(&plan, Decision::BringIn));
    let result = structure(&out).expect("holds");
    let order = titles(&result);
    let market = order.iter().position(|t| t == "Market").expect("market");
    let traction = order
        .iter()
        .position(|t| t == "Traction")
        .expect("traction");
    assert_eq!(market, traction + 1);
}

#[test]
fn a_duplicated_slide_brings_its_own_chart() {
    let t = three(
        &pitch_deck(),
        &pitch_deck(),
        &edits::duplicated(pitch_deck()),
    );
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    let out = written(&plan, &decide_all(&plan, Decision::BringIn));
    let result = structure(&out).expect("holds");
    assert_eq!(result.slides.len(), 9);
    let charts = result
        .package
        .part_names()
        .filter(|n| n.starts_with("ppt/charts/chart"))
        .count();
    assert_eq!(charts, 2, "each slide has its own chart");
}

#[test]
fn both_added_a_slide_in_the_same_place() {
    let t = three(
        &pitch_deck(),
        &edits::added_a(pitch_deck()),
        &edits::added_b(pitch_deck()),
    );
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    assert_eq!(statuses(&plan), [Status::Overlap(OverlapKind::BothAdded)]);
    let id = plan.items[0].change.id.clone();
    for (decision, want) in [
        (
            Decision::KeepBoth {
                masters_first: false,
            },
            vec!["Pricing", "Competition"],
        ),
        (
            Decision::KeepBoth {
                masters_first: true,
            },
            vec!["Competition", "Pricing"],
        ),
        (Decision::UseMasters, vec!["Competition"]),
    ] {
        let out = written(&plan, &[(id.clone(), decision)]);
        let result = structure(&out).expect("holds");
        let order = titles(&result);
        let got: Vec<&str> = order
            .iter()
            .filter(|t| *t == "Pricing" || *t == "Competition")
            .map(String::as_str)
            .collect();
        assert_eq!(got, want, "{decision:?}");
        assert_eq!(order.last().map(String::as_str), Some("The ask"));
    }
}

#[test]
fn notes_hidden_slides_layouts_and_moved_pictures() {
    for theirs in [
        edits::notes_edited(pitch_deck()),
        edits::picture_moved(pitch_deck()),
        edits::made_bold(pitch_deck()),
        edits::layout_changed(pitch_deck()),
        edits::links(pitch_deck()),
        edits::table(pitch_deck()),
        edits::group(pitch_deck()),
    ] {
        let t = three(&pitch_deck(), &edits::northgate(pitch_deck()), &theirs);
        let plan = merge_plan(&t.base, &t.ours, &t.theirs);
        assert!(
            plan.items.iter().all(|i| i.status == Status::Clean),
            "{:?}",
            statuses(&plan)
        );
        let out = written(&plan, &decide_all(&plan, Decision::BringIn));
        let result = structure(&out).expect("holds");
        // What the master changed is now in the version.
        let again = merge_plan(&t.base, &result, &t.theirs);
        assert!(
            again
                .items
                .iter()
                .all(|i| matches!(i.status, Status::AlreadyThere { .. })),
            "{:?}",
            statuses(&again)
        );
    }
    let mut shown = pitch_deck();
    shown.slides.push(edits::appendix(false));
    let mut hidden = pitch_deck();
    hidden.slides.push(edits::appendix(true));
    let t = three(&shown, &shown, &hidden);
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    let out = written(&plan, &decide_all(&plan, Decision::BringIn));
    assert!(
        structure(&out)
            .expect("holds")
            .slides
            .last()
            .expect("appendix")
            .hidden
    );
    // And shown again.
    let t = three(&hidden, &hidden, &shown);
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    let out = written(&plan, &decide_all(&plan, Decision::BringIn));
    assert!(
        !structure(&out)
            .expect("holds")
            .slides
            .last()
            .expect("appendix")
            .hidden
    );
}

#[test]
fn sections_take_new_slides_and_lose_removed_ones() {
    let base = edits::sections(pitch_deck());
    let t = three(
        &base,
        &base,
        &edits::sections(edits::northgate(pitch_deck())),
    );
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    let out = written(&plan, &decide_all(&plan, Decision::BringIn));
    let result = structure(&out).expect("holds");
    assert!(result.sections[1].slides.contains(&264));
    let t = three(
        &base,
        &base,
        &edits::sections(edits::team_removed(pitch_deck())),
    );
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    let out = written(&plan, &decide_all(&plan, Decision::BringIn));
    let result = structure(&out).expect("holds");
    assert!(!result.sections[1].slides.contains(&ids::TEAM));
}

#[test]
fn removing_a_slide_with_comments_removes_them() {
    let ours = edits::comments(pitch_deck());
    let mut theirs = pitch_deck();
    theirs.slides.retain(|s| s.id != ids::ASK);
    let t = three(&pitch_deck(), &ours, &theirs);
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    let out = written(&plan, &decide_all(&plan, Decision::BringIn));
    let result = structure(&out).expect("holds");
    let comments = result
        .package
        .part_names()
        .filter(|n| n.starts_with("ppt/comments/"))
        .count();
    assert_eq!(comments, 1, "the traction slide keeps its comments");
}

#[test]
fn already_there_by_hand_and_deleted_versus_edited() {
    let t = three(
        &pitch_deck(),
        &edits::traction_update(pitch_deck()),
        &edits::traction_update(pitch_deck()),
    );
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    assert!(
        plan.items
            .iter()
            .all(|i| matches!(i.status, Status::AlreadyThere { .. }))
    );

    let mut edited_team = pitch_deck();
    edited_team.slide_mut(ids::TEAM).shapes[0] = Placed::from(Shape::Title(vec![p("Our team")]));
    let t = three(
        &pitch_deck(),
        &edited_team,
        &edits::team_removed(pitch_deck()),
    );
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    assert_eq!(
        statuses(&plan),
        [Status::Overlap(OverlapKind::DeletedVsEdited)]
    );
    assert!(plan.items[0].masters_writable);

    let t = three(&pitch_deck(), &edits::team_removed(pitch_deck()), &{
        let mut d = pitch_deck();
        d.slide_mut(ids::TEAM).shapes[0] = Placed::from(Shape::Title(vec![p("Our team")]));
        d
    });
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    assert_eq!(
        statuses(&plan),
        [Status::Overlap(OverlapKind::EditedVsDeleted)]
    );
    assert_eq!(plan.items[0].choices(), [Decision::KeepMine]);

    // Removing an animated shape, and animations themselves, are by hand.
    let animated = edits::animation(pitch_deck());
    let mut without = edits::animation(pitch_deck());
    without.slide_mut(ids::TRACTION).shapes.pop();
    let t = three(&animated, &animated, &without);
    let plan = merge_plan(&t.base, &t.ours, &t.theirs);
    assert!(
        plan.items
            .iter()
            .all(|i| matches!(i.status, Status::ByHand(_))),
        "{:?}",
        statuses(&plan)
    );
}

#[test]
fn a_layout_the_version_lacks_is_by_hand() {
    // The master's slide uses a layout the version renamed.
    let theirs = edits::northgate(pitch_deck());
    let ours_bytes = pitch_deck().to_bytes();
    let mut package = crate::Package::read(&ours_bytes).expect("reads");
    let layout = "ppt/slideLayouts/slideLayout2.xml";
    let text = String::from_utf8(package.part(layout).expect("layout").to_vec()).expect("utf-8");
    package.set_part(
        layout,
        text.replace(r#"name="Title and Content""#, r#"name="Body""#)
            .replace(r#"type="obj""#, r#"type="cust""#)
            .into_bytes(),
    );
    let ours = Deck::from_package(package).expect("opens");
    let base = open(&pitch_deck());
    let theirs = open(&theirs);
    let plan = merge_plan(&base, &ours, &theirs);
    let added = plan
        .items
        .iter()
        .find(|i| i.change.unit == super::super::diff::Unit::Slide)
        .expect("slide");
    assert_eq!(added.status, Status::ByHand(crate::diff::ByHand::Layout));
}

#[test]
fn nothing_offered_when_the_master_is_the_base() {
    let t = three(
        &pitch_deck(),
        &edits::northgate(pitch_deck()),
        &pitch_deck(),
    );
    assert!(merge_plan(&t.base, &t.ours, &t.theirs).items.is_empty());
}
