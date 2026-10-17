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

/// The task pane's blob cache: what was fetched, and what a run asked for that
/// wasn't there.
struct Fetched<'a> {
    have: &'a crate::blob::MemoryBlobs,
    missed: std::cell::RefCell<Vec<crate::blob::BlobHash>>,
}

impl crate::blob::BlobSource for Fetched<'_> {
    fn blob(&self, hash: &crate::blob::BlobHash) -> Option<Vec<u8>> {
        let found = self.have.blob(hash);
        if found.is_none() {
            self.missed.borrow_mut().push(hash.clone());
        }
        found
    }
}

/// How many runs "Update N versions" takes in a pane that has fetched
/// nothing yet, for `n` versions that each have their own edits.
fn runs_for(n: usize) -> usize {
    let mut host = Host::new();
    let master_file = host.start_deck(&pitch_deck().to_bytes());
    let master = host.master();
    let mut inputs = Vec::new();
    for i in 0..n {
        let name = format!("Investor {i}");
        let (node, file) = host.make_version(&format!("Pitch deck – {name}.pptx"));
        let file = host.save(&node, &deck_edit(&file, &with_box(&name)), &name);
        inputs.push((node, file));
    }
    let edited = deck_edit(&master_file, &edits::traction_update(pitch_deck()));
    host.save(&master, &edited, "New traction numbers");

    // As the host does: run, fetch what was missed (a manifest with its
    // parts), run again.
    let state = host.state().clone();
    let mut cache = crate::blob::MemoryBlobs::default();
    let mut runs = 0;
    loop {
        runs += 1;
        assert!(runs < 20, "never finished");
        let source = Fetched {
            have: &cache,
            missed: std::cell::RefCell::new(Vec::new()),
        };
        let mut ctx = host.context();
        match update_versions(&mut ctx, &state, &inputs, &[], &source) {
            Ok(run) => {
                assert_eq!(run.versions.len(), n);
                assert!(run.versions.iter().all(|v| v.result == RunResult::Updated));
                return runs;
            }
            Err(Error::MissingBlob(_)) => {
                for hash in source.missed.take() {
                    let bytes = host.blobs.blob(&hash).expect("stored");
                    for part in crate::blob::manifest_parts(&bytes).unwrap_or_default() {
                        let part_bytes = host.blobs.blob(&part).expect("stored");
                        cache.0.insert(part, part_bytes);
                    }
                    cache.0.insert(hash, bytes);
                }
            }
            Err(e) => panic!("{e}"),
        }
    }
}

#[test]
fn an_update_asks_for_every_versions_history_at_once() {
    // Found in the PowerPoint run: the first version's missing blob stopped
    // the run, so each version cost the pane more rounds of fetching, and the
    // host gives up after six.
    assert_eq!(runs_for(4), runs_for(1));
}

#[test]
fn a_version_that_is_open_is_left_and_said_so() {
    // Found in the PowerPoint run: PowerPoint for Windows locks a deck it has
    // open, so its file can't be replaced; the other versions still are.
    let mut host = Host::new();
    let master_file = host.start_deck(&pitch_deck().to_bytes());
    let master = host.master();
    let (seed, seed_file) = host.make_version("Pitch deck – Seed fund.pptx");
    let (northgate, _) = host.make_version("Pitch deck – Northgate.pptx");
    let edited = deck_edit(&master_file, &edits::traction_update(pitch_deck()));
    host.save(&master, &edited, "New traction numbers");

    let state = host.state().clone();
    let blobs = host.blobs.clone();
    let mut ctx = host.context();
    let run = update_versions_leaving_open(
        &mut ctx,
        &state,
        &[(seed.clone(), seed_file)],
        std::slice::from_ref(&northgate),
        &[],
        &blobs,
    )
    .expect("updates");
    assert_eq!(run.headline, "Updated 1 of 2 versions");
    assert_eq!(run.versions[0].result, RunResult::Updated);
    let open = &run.versions[1];
    assert_eq!(open.node, northgate);
    assert_eq!(open.name, "Pitch deck – Northgate.pptx");
    assert_eq!(open.result, RunResult::NotUpdated);
    assert!(open.text.starts_with("Not updated: it's open"));
    assert!(open.file.is_none());
    // Nothing is recorded for it: its changes still wait.
    host.take(run.outcome);
    let status =
        crate::changes::variant_status(host.state(), &northgate, &host.blobs).expect("status");
    assert!(!status.offerable.is_empty());
    assert!(status.offerable.iter().all(|p| !p.offered));
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

// ----- Bringing changes into the open deck (decision 0019) ---------------------------

impl Host {
    fn prepare(&mut self, node: &Id, bytes: &[u8], choices: &[(Id, String, Decision)]) -> OpenPlan {
        let state = self.state().clone();
        let blobs = self.blobs.clone();
        let mut ctx = self.context();
        prepare_open(&mut ctx, &state, node, bytes, choices, &blobs).expect("prepares")
    }

    fn finish(
        &mut self,
        node: &Id,
        original: &[u8],
        held: &[u8],
        choices: &[(Id, String, Decision)],
    ) -> crate::Result<Outcome> {
        let state = self.state().clone();
        let blobs = self.blobs.clone();
        let mut ctx = self.context();
        finish_open(&mut ctx, &state, node, original, held, choices, &blobs)
    }
}

/// A master with the traction update saved, and one tailored version made
/// before it.
fn founder_with_update() -> (Host, Id, Vec<u8>) {
    let mut host = Host::new();
    let master_file = host.start_deck(&pitch_deck().to_bytes());
    let master = host.master();
    let (seed, seed_file) = host.make_version("Pitch deck – Seed fund.pptx");
    let edited = deck_edit(&master_file, &edits::traction_update(pitch_deck()));
    host.save(&master, &edited, "New traction numbers");
    (host, seed, seed_file)
}

fn names(bytes: &[u8]) -> Vec<String> {
    Deck::open(bytes)
        .expect("opens")
        .slides
        .iter()
        .filter_map(lineage_core::pptx::slides::office_id)
        .collect()
}

fn office(id: u32) -> String {
    format!("{id}#{}", lineage_testkit::pptx::creation_id_for(id))
}

#[test]
fn the_open_deck_takes_the_master_change_and_records_what_powerpoint_holds() {
    use lineage_core::pptx::slides::SlideOp;

    let (mut host, seed, seed_file) = founder_with_update();
    let plan = host.prepare(&seed, &seed_file, &[]);
    assert_eq!(plan.run.result, RunResult::Updated);
    assert_eq!(plan.run.text, "Brought in 2 changes");
    let merged = plan.merged.clone().expect("merged");
    assert!(traction_text(&merged).contains("$2.0M ARR"));
    assert_eq!(
        plan.ops.ops,
        [
            SlideOp::Delete {
                slide: office(ids::TRACTION)
            },
            SlideOp::Insert {
                slide: office(ids::TRACTION),
                after: Some(office(ids::PRODUCT)),
            },
        ]
    );
    assert_eq!(plan.ops.order, names(&merged));
    // Nothing is recorded until PowerPoint holds the deck.
    let before = host.ledger_bytes();

    // PowerPoint holds the same deck, written its own way.
    let held = deck_edit(&seed_file, &edits::traction_update(pitch_deck()));
    assert_ne!(held, merged);
    let outcome = host
        .finish(&seed, &seed_file, &held, &[])
        .expect("finishes");
    assert_eq!(host.ledger_bytes(), before);
    let ids = outcome.ids.clone().expect("ids for the open deck");
    let created = outcome.created.clone().expect("checkpoint");
    assert_eq!(ids.checkpoint, created.as_str());
    host.take(outcome);

    let latest = host.state().latest(&seed).expect("latest").clone();
    assert_eq!(latest.id, created);
    assert!(
        latest.note.starts_with("Brought in 2 changes from master"),
        "{}",
        latest.note
    );
    let recorded = crate::blob::restore(&latest.snapshot, &host.blobs).expect("restores");
    let (a, b) = (
        Deck::open(&recorded).expect("opens"),
        Deck::open(&held).expect("opens"),
    );
    assert_eq!(a.slides.len(), b.slides.len());
    assert!(
        a.slides
            .iter()
            .zip(&b.slides)
            .all(|(x, y)| x.full == y.full),
        "the checkpoint is the deck PowerPoint holds"
    );
    // The open deck with its new ids is that checkpoint, and nothing is left.
    let mut package = Package::read(&held).expect("reads");
    write_ids(&mut package, &ids).expect("writes");
    let now = package.to_bytes().expect("bytes");
    let status = crate::changes::variant_status(host.state(), &seed, &host.blobs).expect("status");
    assert!(status.offerable.is_empty(), "{:?}", status.offerable);
    let again = host.prepare(&seed, &now, &[]);
    assert_eq!(again.run.text, "Already up to date");
    assert!(again.merged.is_none() && again.ops.is_empty());
}

#[test]
fn a_deck_that_did_not_land_records_nothing() {
    let (mut host, seed, seed_file) = founder_with_update();
    // PowerPoint still holds the old slide, or only part of the change.
    for held in [
        seed_file.clone(),
        deck_edit(&seed_file, &with_box("$2.0M ARR")),
    ] {
        let failed = host.finish(&seed, &seed_file, &held, &[]);
        assert!(
            matches!(
                failed,
                Err(Error::Engine(lineage_core::Error::NotLanded(_)))
            ),
            "{failed:?}"
        );
    }
}

#[test]
fn an_overlap_waits_for_a_choice_on_the_open_deck() {
    let mut host = Host::new();
    let master_file = host.start_deck(&pitch_deck().to_bytes());
    let master = host.master();
    let (north, north_file) = host.make_version("Pitch deck – Northgate.pptx");
    let north_file = host.save(
        &north,
        &deck_edit(&north_file, &with_box("$1.3M ARR")),
        "Northgate numbers",
    );
    let edited = deck_edit(&master_file, &edits::traction_update(pitch_deck()));
    host.save(&master, &edited, "New traction numbers");

    let plan = host.prepare(&north, &north_file, &[]);
    assert_eq!(plan.run.text, "Brought in 1 change · 1 to choose");
    let overlap = plan.run.left.overlaps[0].id.clone();
    assert!(traction_text(&plan.merged.expect("merged")).contains("$1.3M ARR"));

    let keep = host.prepare(
        &north,
        &north_file,
        &[(north.clone(), overlap.clone(), Decision::KeepMine)],
    );
    assert_eq!(keep.run.text, "Brought in 1 change");
    assert!(traction_text(&keep.merged.expect("merged")).contains("$1.3M ARR"));

    let choice = [(north.clone(), overlap, Decision::UseMasters)];
    let masters = host.prepare(&north, &north_file, &choice);
    assert_eq!(masters.run.text, "Brought in 2 changes");
    let merged = masters.merged.expect("merged");
    assert!(traction_text(&merged).contains("$2.0M ARR"));
    let outcome = host
        .finish(&north, &north_file, &merged, &choice)
        .expect("finishes");
    host.take(outcome);
}

#[test]
fn edits_not_yet_saved_are_kept_as_a_checkpoint_first() {
    let (mut host, seed, seed_file) = founder_with_update();
    let typed = deck_edit(&seed_file, &edits::notes_edited(pitch_deck()));
    let plan = host.prepare(&seed, &typed, &[]);
    assert!(plan.run.saved_edits);
    let merged = plan.merged.expect("merged");
    let outcome = host.finish(&seed, &typed, &merged, &[]).expect("finishes");
    host.take(outcome);
    let notes: Vec<String> = host
        .state()
        .node(&seed)
        .expect("seed")
        .checkpoints
        .iter()
        .filter_map(|c| host.state().checkpoint(c).map(|c| c.note.clone()))
        .collect();
    assert_eq!(notes.len(), 3, "{notes:?}");
    assert_eq!(notes[1], "Your edits before the update");
}

#[test]
fn a_deck_whose_slides_powerpoint_cannot_name_stays_for_the_master_deck() {
    let unnamed = |mut deck: Built| {
        for slide in &mut deck.slides {
            slide.creation_id = None;
        }
        deck
    };
    let mut host = Host::new();
    let master_file = host.start_deck(&unnamed(pitch_deck()).to_bytes());
    let master = host.master();
    let (seed, seed_file) = host.make_version("Pitch deck – Seed fund.pptx");
    let update = unnamed(edits::traction_update(pitch_deck()));
    host.save(&master, &deck_edit(&master_file, &update), "New numbers");
    let plan = host.prepare(&seed, &seed_file, &[]);
    assert_eq!(plan.run.result, RunResult::NotUpdated);
    assert!(plan.run.text.contains("Close it"), "{}", plan.run.text);
    assert!(plan.merged.is_none() && plan.ops.is_empty());
}
