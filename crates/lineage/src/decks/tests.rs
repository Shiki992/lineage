// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_core::Deck;
use lineage_core::Package;
use lineage_core::docx::props::{read_ids, write_ids};
use lineage_core::merge::Decision;
use lineage_testkit::decks::{edits, ids, pitch_deck, traction_box};
use lineage_testkit::pptx::{Deck as Built, Shape};

use super::*;
use crate::commands::{self, Context};
use crate::tests::Host;

/// "The founder edits the deck in PowerPoint": the deck becomes `built`, and
/// keeps the file's Lineage ids.
fn deck_edit(file: &[u8], built: &Built) -> Vec<u8> {
    let ids = read_ids(&Package::read(file).expect("reads"))
        .expect("reads")
        .expect("ids");
    let mut package = Package::read(&built.to_bytes()).expect("reads");
    write_ids(&mut package, &ids).expect("writes");
    package.to_bytes().expect("bytes")
}

fn traction_text(bytes: &[u8]) -> String {
    let deck = Deck::open(bytes).expect("opens");
    let slide = deck
        .slides
        .iter()
        .find(|s| s.title().as_deref() == Some("Traction"))
        .expect("traction");
    slide.text()
}

fn with_box(text: &str) -> Built {
    let mut deck = pitch_deck();
    for placed in &mut deck.slide_mut(ids::TRACTION).shapes {
        if let Shape::TextBox { .. } = placed.shape {
            placed.shape = traction_box(text);
        }
    }
    deck
}

impl Host {
    fn start_deck(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut ctx = Context {
            now: self.now,
            ids: &mut self.ids,
            author: None,
        };
        let (_, outcome) = commands::start_tracking(
            &mut ctx,
            bytes,
            "Pitch deck.pptx",
            "Pitch deck.pptx",
            &self.blobs,
        )
        .expect("starts");
        self.take(outcome).file.expect("file with ids")
    }

    fn update(
        &mut self,
        inputs: &[(Id, Vec<u8>)],
        choices: &[(Id, String, Decision)],
    ) -> UpdateRun {
        let state = self.state().clone();
        let blobs = self.blobs.clone();
        let mut ctx = self.context();
        let run = update_versions(&mut ctx, &state, inputs, choices, &blobs).expect("updates");
        self.take(run.outcome.clone());
        run
    }
}

#[test]
fn the_founder_updates_two_versions_and_chooses_once() {
    let mut host = Host::new();
    let master_file = host.start_deck(&pitch_deck().to_bytes());
    assert_eq!(host.state().format, "pptx");
    let master = host.master();
    let (seed, seed_file) = host.make_version("Pitch deck – Seed fund.pptx");
    let (northgate, northgate_file) = host.make_version("Pitch deck – Northgate.pptx");
    let (pellow, pellow_file) = host.make_version("Pitch deck – Pellow.pptx");
    // Northgate's version says its own number on slide 6.
    let northgate_file = host.save(
        &northgate,
        &deck_edit(&northgate_file, &with_box("$1.3M ARR")),
        "Northgate's numbers",
    );
    // Pellow gets a copy with the old numbers.
    let (sent, copy) = host.send(&pellow, &pellow_file, "Pellow Ventures", true);
    assert!(copy.ends_with_pptx());

    // The founder changes the traction numbers in the master.
    let edited = deck_edit(&master_file, &edits::traction_update(pitch_deck()));
    let panel = deck_panel(host.state(), Some(&edited), &host.blobs).expect("panel");
    let changed = panel.just_changed.as_ref().expect("just changed");
    assert_eq!(changed.headline, "You just changed Slide 6 · Traction");
    assert!(changed.unsaved);
    assert_eq!(panel.master.chip, "Master deck");
    assert_eq!(panel.master.follows, "3 tailored versions follow this deck");
    assert_eq!(panel.button.as_deref(), Some("Update 3 versions"));
    let pellow_row = panel
        .targets
        .iter()
        .find(|t| t.id == pellow)
        .expect("pellow");
    assert!(pellow_row.context.starts_with("Last sent "));
    assert!(pellow_row.context.ends_with(", before this change"));
    let last = panel.last_sent.as_ref().expect("last sent");
    assert!(last.text.starts_with("Sent to Pellow Ventures on "));
    assert_eq!(last.button, "Open what they saw");

    // "Update 2 versions": the checkpoint first, then Seed fund and Northgate.
    host.save(&master, &edited, "New traction numbers");
    let run = host.update(
        &[
            (seed.clone(), seed_file.clone()),
            (northgate.clone(), northgate_file.clone()),
        ],
        &[],
    );
    assert_eq!(run.headline, "Updated all 2 versions");
    let seed_run = &run.versions[0];
    assert_eq!(seed_run.result, RunResult::Updated);
    assert_eq!(seed_run.text, "Brought in 2 changes");
    let new_seed = seed_run.file.clone().expect("written");
    assert!(traction_text(&new_seed).contains("$2.0M ARR"));
    assert!(traction_text(&new_seed).contains("Q4 2026: 2000"));

    let north_run = &run.versions[1];
    assert_eq!(north_run.result, RunResult::Updated);
    assert_eq!(north_run.text, "Brought in 1 change · 1 to choose");
    assert_eq!(north_run.left.overlaps.len(), 1);
    let overlap = north_run.left.overlaps[0].clone();
    assert_eq!(overlap.summary, "Both changed '$1.2M ARR'");
    let north_file = north_run.file.clone().expect("written");
    assert!(
        traction_text(&north_file).contains("$1.3M ARR"),
        "theirs is kept until chosen"
    );
    assert!(traction_text(&north_file).contains("Q4 2026: 2000"));

    // The founder chooses the master's number for Northgate.
    let run = host.update(
        &[(northgate.clone(), north_file)],
        &[(northgate.clone(), overlap.id.clone(), Decision::UseMasters)],
    );
    assert_eq!(run.versions[0].text, "Brought in 1 change");
    assert!(traction_text(&run.versions[0].file.clone().expect("written")).contains("$2.0M ARR"));

    // Pellow was left unticked: the changes wait for it, not offered.
    let status =
        crate::changes::variant_status(host.state(), &pellow, &host.blobs).expect("status");
    assert!(!status.offerable.is_empty());
    assert!(status.offerable.iter().all(|p| !p.offered));
    // What Pellow saw: the exact copy, with the old numbers.
    let saw = commands::open_sent_copy(host.state(), &sent, &host.blobs).expect("opens");
    assert_eq!(saw, copy);
    assert!(traction_text(&saw).contains("$1.2M ARR"));
    let deck = Deck::open(&saw).expect("opens");
    assert_eq!(deck.slides.len(), 8);

    // A second run finds nothing more to do.
    let panel = deck_panel(host.state(), None, &host.blobs).expect("panel");
    let seed_row = panel.targets.iter().find(|t| t.id == seed).expect("seed");
    assert_eq!(seed_row.context, "Up to date");
    assert!(!seed_row.checked);
}

#[test]
fn edits_made_outside_lineage_are_saved_before_the_update() {
    let mut host = Host::new();
    let master_file = host.start_deck(&pitch_deck().to_bytes());
    let master = host.master();
    let (seed, seed_file) = host.make_version("Pitch deck – Seed fund.pptx");
    // The founder edits the version in PowerPoint and never opens the panel.
    let edited_seed = deck_edit(&seed_file, &edits::made_bold(pitch_deck()));
    host.save(
        &master,
        &deck_edit(&master_file, &edits::traction_update(pitch_deck())),
        "New traction numbers",
    );
    let run = host.update(&[(seed.clone(), edited_seed)], &[]);
    let line = &run.versions[0];
    assert!(line.saved_edits);
    assert_eq!(line.result, RunResult::Updated);
    let notes: Vec<String> = host
        .state()
        .node(&seed)
        .expect("seed")
        .checkpoints
        .iter()
        .map(|c| host.state().checkpoint(c).expect("c").note.clone())
        .collect();
    assert_eq!(
        notes,
        [
            "Made from master v1",
            "Your edits before the update",
            "Brought in 2 changes from master v2"
        ]
    );
    // Both the founder's bold and the master's numbers are in the file.
    let file = line.file.clone().expect("written");
    let deck = Deck::open(&file).expect("opens");
    let bold = Deck::open(&edits::made_bold(pitch_deck()).to_bytes()).expect("opens");
    assert_eq!(deck.slides[3].full, bold.slides[3].full);
    assert!(traction_text(&file).contains("$2.0M ARR"));
}

#[test]
fn a_word_family_has_no_deck_panel() {
    let family = crate::testfamily::everything();
    assert!(matches!(
        deck_panel(family.host.state(), None, &family.host.blobs),
        Err(Error::NotAllowed(_))
    ));
}

trait EndsWithPptx {
    fn ends_with_pptx(&self) -> bool;
}

impl EndsWithPptx for Vec<u8> {
    fn ends_with_pptx(&self) -> bool {
        Deck::open(self).is_ok()
    }
}
