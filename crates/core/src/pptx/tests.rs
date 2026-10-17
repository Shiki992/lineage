// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_testkit::decks::{self, edits, ids, pitch_deck};

use super::*;

fn open(deck: &lineage_testkit::pptx::Deck) -> Deck {
    Deck::open(&deck.to_bytes()).expect("opens")
}

#[test]
fn every_fixture_opens_and_round_trips_unchanged() {
    for (name, deck) in decks::all() {
        let bytes = deck.to_bytes();
        let opened = Deck::open(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(opened.slides.len(), deck.slides.len(), "{name}");
        // Written back untouched, every part and entry is exactly as it was.
        let package = Package::read(&bytes).expect("reads");
        let again = Package::read(&package.to_bytes().expect("writes")).expect("rereads");
        for part in package.part_names() {
            assert!(again.is_unchanged(part), "{name}: {part}");
            assert_eq!(package.part(part), again.part(part), "{name}: {part}");
        }
    }
}

#[test]
fn slides_have_ids_titles_layouts_and_notes() {
    let deck = open(&pitch_deck());
    let titles: Vec<String> = deck.slides.iter().filter_map(Slide::title).collect();
    assert_eq!(
        titles,
        [
            "Lumen Ledger",
            "Problem",
            "Solution",
            "Market",
            "Product",
            "Traction",
            "Team",
            "The ask"
        ]
    );
    let traction = deck.slide(ids::TRACTION).expect("traction");
    assert_eq!(
        traction.creation_id,
        Some(lineage_testkit::pptx::creation_id_for(ids::TRACTION))
    );
    assert_eq!(
        traction.layout.as_ref().map(|l| l.name.as_str()),
        Some("Title Only")
    );
    assert_eq!(
        traction.notes.as_ref().map(Notes::text).as_deref(),
        Some("Numbers as of 30 September.")
    );
    let kinds: Vec<ShapeKind> = traction.shapes.iter().map(|s| s.kind).collect();
    assert_eq!(kinds, [ShapeKind::Text, ShapeKind::Chart, ShapeKind::Text]);
    assert_eq!(
        traction.shapes[2].text(),
        "$1.2M ARR\nNet revenue retention 128%"
    );
    let product = deck.slide(ids::PRODUCT).expect("product");
    assert_eq!(product.shapes[1].kind, ShapeKind::Picture);
    assert!(product.shapes[1].geometry.is_some());
    let solution = deck.slide(ids::SOLUTION).expect("solution");
    assert_eq!(solution.shapes[1].paras[1].level, 1);
}

#[test]
fn the_same_deck_twice_has_the_same_keys() {
    let a = open(&pitch_deck());
    let b = open(&pitch_deck());
    for (x, y) in a.slides.iter().zip(&b.slides) {
        assert_eq!(x.full, y.full);
    }
    assert_eq!(a.design, b.design);
}

#[test]
fn a_chart_edit_changes_only_the_chart() {
    let base = open(&pitch_deck());
    let edited = open(&edits::chart_edited(pitch_deck()));
    let (a, b) = (
        base.slide(ids::TRACTION).expect("t"),
        edited.slide(ids::TRACTION).expect("t"),
    );
    assert_ne!(a.full, b.full);
    assert_ne!(
        a.shapes[1].content, b.shapes[1].content,
        "the chart is one object"
    );
    assert_eq!(a.shapes[0].full, b.shapes[0].full);
    assert_eq!(a.shapes[2].full, b.shapes[2].full);
    for (x, y) in base.slides.iter().zip(&edited.slides) {
        if x.sld_id != ids::TRACTION {
            assert_eq!(x.full, y.full, "slide {}", x.sld_id);
        }
    }
}

#[test]
fn moving_a_picture_keeps_its_content() {
    let base = open(&pitch_deck());
    let moved = open(&edits::picture_moved(pitch_deck()));
    let (a, b) = (
        &base.slide(ids::PRODUCT).expect("p").shapes[1],
        &moved.slide(ids::PRODUCT).expect("p").shapes[1],
    );
    assert_eq!(a.content, b.content);
    assert_ne!(a.full, b.full);
    assert_ne!(a.geometry, b.geometry);
}

#[test]
fn bold_is_formatting_and_text_is_content() {
    let base = open(&pitch_deck());
    let bold = open(&edits::made_bold(pitch_deck()));
    let (a, b) = (
        &base.slide(ids::MARKET).expect("m").shapes[1],
        &bold.slide(ids::MARKET).expect("m").shapes[1],
    );
    assert_eq!(a.content, b.content);
    assert_ne!(a.full, b.full);
    let traction = open(&edits::traction_update(pitch_deck()));
    assert_ne!(
        base.slide(ids::TRACTION).expect("t").shapes[2].content,
        traction.slide(ids::TRACTION).expect("t").shapes[2].content
    );
}

#[test]
fn spell_check_and_language_marks_are_not_changes() {
    let base = pitch_deck().to_bytes();
    let package = Package::read(&base).expect("reads");
    let part = "ppt/slides/slide4.xml";
    let text = String::from_utf8(package.part(part).expect("slide").to_vec()).expect("utf-8");
    let noisy = text.replacen(
        r#"<a:rPr lang="en-US"/>"#,
        r#"<a:rPr lang="en-GB" dirty="0" err="1"/>"#,
        2,
    );
    assert_ne!(noisy, text);
    let mut changed = package.clone();
    changed.set_part(part, noisy.into_bytes());
    let a = Deck::from_package(package).expect("opens");
    let b = Deck::from_package(changed).expect("opens");
    assert_eq!(a.slides[3].full, b.slides[3].full);
}

#[test]
fn hidden_slides_sections_and_missing_creation_ids_are_read() {
    let deck = open(&edits::hidden_appendix(pitch_deck()));
    assert!(deck.slides.last().expect("appendix").hidden);
    assert!(deck.lines().iter().any(|l| l.ends_with("(hidden)")));
    let sections = open(&edits::sections(pitch_deck()));
    assert_eq!(sections.sections.len(), 2);
    assert_eq!(sections.sections[1].slides[1], ids::TRACTION);
    let bare = decks::all()
        .into_iter()
        .find(|(n, _)| *n == "pitch-deck-no-creation-ids.pptx")
        .expect("fixture")
        .1;
    assert!(open(&bare).slides.iter().all(|s| s.creation_id.is_none()));
}

#[test]
fn animations_name_their_shapes() {
    let deck = open(&edits::animation(pitch_deck()));
    let traction = deck.slide(ids::TRACTION).expect("t");
    assert_ne!(traction.timing, 0);
    assert_eq!(traction.animated, [4]);
}

#[test]
fn tables_and_groups_have_text() {
    let deck = open(&edits::table(pitch_deck()));
    let table = deck.slide(264).expect("table slide");
    assert_eq!(table.shapes[1].kind, ShapeKind::Table);
    assert_eq!(table.shapes[1].paras[1].text, "Engineering · 60%");
    let deck = open(&edits::group(pitch_deck()));
    let group = deck
        .slide(ids::SOLUTION)
        .expect("s")
        .shapes
        .last()
        .expect("group");
    assert_eq!(group.kind, ShapeKind::Group);
    assert_eq!(group.text(), "Nightly\nSelf-checking");
    assert_eq!(group.members.len(), 2);
}

#[test]
fn word_documents_and_strangers_are_told_apart() {
    let docx = lineage_testkit::fixtures::all()
        .into_iter()
        .next()
        .expect("a docx")
        .1
        .to_bytes();
    assert_eq!(kind_of(&docx), Ok(Kind::Word));
    assert_eq!(kind_of(&pitch_deck().to_bytes()), Ok(Kind::PowerPoint));
    assert!(matches!(Deck::open(&docx), Err(Error::NotADeck(_))));
    assert!(matches!(Deck::open(b"not a zip"), Err(Error::NotAZip(_))));
}

#[test]
fn the_reader_never_panics_on_damaged_decks() {
    let bytes = pitch_deck().to_bytes();
    let package = Package::read(&bytes).expect("reads");
    // Damage each XML part in turn: truncated, and with its root renamed.
    for part in package
        .part_names()
        .filter(|n| n.ends_with(".xml") || n.ends_with(".rels"))
    {
        let text = package.part(part).expect("part");
        for damaged in [
            text[..text.len() / 2].to_vec(),
            String::from_utf8_lossy(text)
                .replacen("<p:", "<q:", 2)
                .into_bytes(),
        ] {
            let mut broken = package.clone();
            broken.set_part(part, damaged);
            let _ = Deck::from_package(broken);
        }
    }
    // And random byte flips in the zip itself.
    for i in (0..bytes.len()).step_by(997) {
        let mut flipped = bytes.clone();
        flipped[i] ^= 0x5A;
        let _ = Deck::open(&flipped);
    }
}
