// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! BLUEPRINT's founder journey, pressed through the panel the way a person
//! would, on in-memory storage and on a real folder:
//!
//! 1. Start tracking a master pitch deck; make versions for a seed fund,
//!    Northgate Capital and Pellow Ventures.
//! 2. Send Pellow a copy. Northgate's version gets its own number on slide 6.
//! 3. The founder changes Slide 6 · Traction in the master: the panel says so,
//!    with the versions to update and the copy Pellow has.
//! 4. "Update 2 versions" (Pellow left unticked): the seed fund's version is
//!    updated; Northgate's is updated except the number both changed, which
//!    waits for a choice, then takes the master's.
//! 5. Open what Pellow saw: exactly the copy that left, with the old numbers.

use std::path::PathBuf;

use lineage::Timestamp;
use lineage::decks::RunResult;
use lineage_core::docx::props::{read_ids, write_ids};
use lineage_core::merge::Decision;
use lineage_core::{Deck, Package};
use lineage_harness::{LocalFolderStorage, MemoryStorage, Panel, Storage};
use lineage_testkit::decks::{edits, ids, pitch_deck, traction_box};
use lineage_testkit::pptx::{Deck as Built, Shape};

const MASTER: &str = "Pitch deck.pptx";
const SEED: &str = "Tailored versions/Pitch deck – Seed fund.pptx";
const NORTHGATE: &str = "Tailored versions/Pitch deck – Northgate.pptx";
const PELLOW: &str = "Tailored versions/Pitch deck – Pellow.pptx";
/// 2026-10-15T09:30:00Z.
const START: Timestamp = Timestamp(1_792_056_600_000);

/// The founder edits a deck in PowerPoint: it becomes `built`, keeping its ids.
fn edit_in_powerpoint<S: Storage>(storage: &mut S, file: &str, built: &Built) {
    let old = Package::read(&storage.read_file(file).expect("file")).expect("reads");
    let ids = read_ids(&old).expect("reads").expect("ids");
    let mut package = Package::read(&built.to_bytes()).expect("reads");
    write_ids(&mut package, &ids).expect("writes");
    storage
        .write_file(file, &package.to_bytes().expect("bytes"))
        .expect("writes");
}

fn traction<S: Storage>(storage: &S, file: &str) -> String {
    traction_of(&storage.read_file(file).expect("file"))
}

fn traction_of(bytes: &[u8]) -> String {
    let deck = Deck::open(bytes).expect("opens");
    deck.slides
        .iter()
        .find(|s| s.title().as_deref() == Some("Traction"))
        .expect("traction")
        .text()
}

fn scenario<S: Storage>(mut storage: S) -> Panel<S> {
    storage
        .write_file(MASTER, &pitch_deck().to_bytes())
        .expect("puts the master");
    let mut panel = Panel::new(storage, START);

    // 1. Start tracking, and three tailored versions.
    panel.start_tracking(MASTER).expect("starts tracking");
    for file in [SEED, NORTHGATE, PELLOW] {
        panel.make_tailored_version(file).expect("makes a version");
    }
    let deck = panel.deck_panel(None).expect("deck panel");
    assert_eq!(deck.master.chip, "Master deck");
    assert_eq!(deck.master.follows, "3 tailored versions follow this deck");
    assert!(deck.just_changed.is_none());
    assert_eq!(deck.button, None, "nothing to update yet");

    // 2. A copy for Pellow; Northgate's own number.
    let copy = panel
        .send_a_copy(PELLOW, "Pellow Ventures", Some("Seed round"), true)
        .expect("sends");
    assert!(copy.ends_with(".pptx"));
    let mut northgate = pitch_deck();
    for placed in &mut northgate.slide_mut(ids::TRACTION).shapes {
        if let Shape::TextBox { .. } = placed.shape {
            placed.shape = traction_box("$1.3M ARR");
        }
    }
    edit_in_powerpoint(&mut panel.storage, NORTHGATE, &northgate);
    panel
        .save_checkpoint(NORTHGATE, "Northgate's numbers")
        .expect("saves");

    // 3. The founder changes the numbers in the master.
    edit_in_powerpoint(
        &mut panel.storage,
        MASTER,
        &edits::traction_update(pitch_deck()),
    );
    let deck = panel.deck_panel(Some(MASTER)).expect("deck panel");
    let changed = deck.just_changed.as_ref().expect("just changed");
    assert_eq!(changed.headline, "You just changed Slide 6 · Traction");
    let summaries: Vec<&str> = changed.cards.iter().map(|c| c.summary.as_str()).collect();
    assert_eq!(
        summaries,
        [
            "The chart changed: '1200' changed to '2000'",
            "'$1.2M' changed to '$2.0M'"
        ]
    );
    assert_eq!(deck.button.as_deref(), Some("Update 3 versions"));
    let last = deck.last_sent.as_ref().expect("last sent");
    assert!(last.text.starts_with("Sent to Pellow Ventures on "));

    // 4. "Update 2 versions": the checkpoint first, Pellow left unticked.
    panel
        .save_checkpoint(MASTER, "New traction numbers")
        .expect("saves");
    let run = panel
        .update_versions(&[SEED, NORTHGATE], &[])
        .expect("updates");
    assert_eq!(run.headline, "Updated all 2 versions");
    assert_eq!(run.versions[0].text, "Brought in 2 changes");
    assert!(traction(&panel.storage, SEED).contains("$2.0M ARR"));
    assert_eq!(run.versions[1].result, RunResult::Updated);
    assert_eq!(run.versions[1].text, "Brought in 1 change · 1 to choose");
    let overlap = run.versions[1].left.overlaps[0].clone();
    assert_eq!(
        overlap
            .choices
            .iter()
            .map(|c| c.label.as_str())
            .collect::<Vec<_>>(),
        ["Keep mine", "Use the master's"]
    );
    assert!(traction(&panel.storage, NORTHGATE).contains("$1.3M ARR"));
    assert!(traction(&panel.storage, NORTHGATE).contains("Q4 2026: 2000"));
    let run = panel
        .update_versions(
            &[NORTHGATE],
            &[(NORTHGATE, &overlap.id, Decision::UseMasters)],
        )
        .expect("updates");
    assert_eq!(run.versions[0].text, "Brought in 1 change");
    assert!(traction(&panel.storage, NORTHGATE).contains("$2.0M ARR"));

    // The versions' panels and the master's agree.
    let deck = panel.deck_panel(None).expect("deck panel");
    let contexts: Vec<(&str, bool)> = deck
        .targets
        .iter()
        .map(|t| (t.context.as_str(), t.checked))
        .collect();
    assert_eq!(contexts[0], ("Up to date", false));
    assert_eq!(contexts[1], ("Up to date", false));
    assert!(contexts[2].1, "Pellow still lacks the new numbers");
    assert_eq!(deck.button.as_deref(), Some("Update 1 version"));

    // 5. What Pellow saw.
    let sent = panel.what_ive_sent(None).expect("sent list");
    let row = &sent.groups[0].rows[0];
    assert_eq!(row.recipient, "Pellow Ventures");
    assert_eq!(row.format, "PowerPoint");
    let saw = panel.open_what_they_received(&row.id).expect("opens");
    assert_eq!(saw, panel.storage.read_file(&copy).expect("copy"));
    assert!(traction_of(&saw).contains("$1.2M ARR"));
    panel
}

#[test]
fn the_founder_journey_in_memory() {
    scenario(MemoryStorage::default());
}

#[test]
fn the_founder_journey_on_disk() {
    let dir = std::env::temp_dir().join(format!("lineage-founder-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let panel = scenario(LocalFolderStorage::new(&dir).expect("folder"));
    // What is on disk opens, and reads as the panel says.
    let path: PathBuf = dir.join(SEED);
    let bytes = std::fs::read(&path).expect("seed on disk");
    assert!(traction_of(&bytes).contains("$2.0M ARR"));
    drop(panel);
    let _ = std::fs::remove_dir_all(&dir);
}
