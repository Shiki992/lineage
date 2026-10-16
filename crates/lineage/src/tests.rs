// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Family logic through its commands, with an in-memory host.

use lineage_core::Package;
use lineage_core::merge::Decision;
use lineage_testkit::resume::{self, Line};

use crate::blob::MemoryBlobs;
use crate::changes::variant_status;
use crate::commands::{self, Context, Outcome};
use crate::event::{self, ChangeState, How};
use crate::ids::{Id, SequentialIds, Timestamp};
use crate::state::FamilyState;

/// A host that keeps everything in memory, the way the harness does on disk.
pub(crate) struct Host {
    pub(crate) ids: SequentialIds,
    pub(crate) now: Timestamp,
    pub(crate) blobs: MemoryBlobs,
    ledger: Vec<event::Record>,
    state: Option<FamilyState>,
}

impl Host {
    pub(crate) fn new() -> Self {
        Self {
            ids: SequentialIds::new(1),
            now: Timestamp(1_792_056_600_000),
            blobs: MemoryBlobs::default(),
            ledger: Vec::new(),
            state: None,
        }
    }

    /// The ledger as stored.
    pub(crate) fn ledger_bytes(&self) -> Vec<u8> {
        event::encode(&self.ledger).expect("encodes")
    }

    pub(crate) fn context(&mut self) -> Context<'_> {
        Context {
            now: self.now,
            ids: &mut self.ids,
            author: None,
        }
    }

    pub(crate) fn tick(&mut self) {
        self.now = Timestamp(self.now.0 + 60_000);
    }

    pub(crate) fn state(&self) -> &FamilyState {
        self.state.as_ref().expect("tracking")
    }

    pub(crate) fn take(&mut self, outcome: Outcome) -> Outcome {
        for blob in &outcome.blobs {
            self.blobs.add(blob.clone());
        }
        self.ledger.extend(outcome.records.iter().cloned());
        // Fold from the stored ledger, as a fresh host would.
        let bytes = event::encode(&self.ledger).expect("encodes");
        self.state = Some(
            FamilyState::from_records(&event::decode(&bytes).expect("decodes")).expect("folds"),
        );
        self.tick();
        outcome
    }

    pub(crate) fn start(&mut self, bytes: &[u8]) -> Vec<u8> {
        let mut ctx = Context {
            now: self.now,
            ids: &mut self.ids,
            author: None,
        };
        let (_, outcome) =
            commands::start_tracking(&mut ctx, bytes, "Resume.docx", "Resume.docx", &self.blobs)
                .expect("starts");
        self.take(outcome).file.expect("file with ids")
    }

    pub(crate) fn master(&self) -> Id {
        self.state().master().expect("master").id.clone()
    }

    pub(crate) fn make_version(&mut self, name: &str) -> (Id, Vec<u8>) {
        let state = self.state().clone();
        let mut ctx = Context {
            now: self.now,
            ids: &mut self.ids,
            author: None,
        };
        let outcome = commands::make_tailored_version(&mut ctx, &state, name, name, &self.blobs)
            .expect("makes");
        let outcome = self.take(outcome);
        (
            outcome.created.expect("node"),
            outcome.new_file.expect("file").bytes,
        )
    }

    pub(crate) fn save(&mut self, node: &Id, bytes: &[u8], note: &str) -> Vec<u8> {
        let state = self.state().clone();
        let mut ctx = Context {
            now: self.now,
            ids: &mut self.ids,
            author: None,
        };
        let outcome = commands::save_checkpoint(&mut ctx, &state, node, bytes, note, &self.blobs)
            .expect("saves");
        self.take(outcome).file.unwrap_or_else(|| bytes.to_vec())
    }

    pub(crate) fn offer(&mut self, versions: &[Id]) {
        let state = self.state().clone();
        let mut ctx = Context {
            now: self.now,
            ids: &mut self.ids,
            author: None,
        };
        let outcome =
            commands::offer_to_versions(&mut ctx, &state, versions, &self.blobs).expect("offers");
        self.take(outcome);
    }

    pub(crate) fn decide(
        &mut self,
        node: &Id,
        bytes: &[u8],
        decisions: &[(Id, String, Decision)],
    ) -> Vec<u8> {
        let state = self.state().clone();
        let mut ctx = Context {
            now: self.now,
            ids: &mut self.ids,
            author: None,
        };
        let outcome = commands::decide(&mut ctx, &state, node, bytes, decisions, &self.blobs)
            .expect("decides");
        self.take(outcome).file.unwrap_or_else(|| bytes.to_vec())
    }

    /// Every offered change for a version, as (checkpoint, change id).
    pub(crate) fn offered(&self, node: &Id) -> Vec<(Id, String)> {
        variant_status(self.state(), node, &self.blobs)
            .expect("status")
            .offerable
            .into_iter()
            .filter(|p| p.offered)
            .map(|p| (p.checkpoint, p.change.id))
            .collect()
    }
}

/// "The person edits the file in Word": the body is replaced by a fixture's,
/// made independently by the generator, and the file keeps its own properties.
pub(crate) fn edit(file: &[u8], lines: &[Line]) -> Vec<u8> {
    let source = Package::read(&resume::package("t", lines).to_bytes()).expect("reads");
    let mut package = Package::read(file).expect("reads");
    package.set_part(
        "word/document.xml",
        source.part("word/document.xml").expect("doc").to_vec(),
    );
    package.to_bytes().expect("bytes")
}

pub(crate) fn lines(bytes: &[u8]) -> Vec<String> {
    lineage_core::Docx::open(bytes).expect("opens").body_lines()
}

#[test]
fn a_declined_change_is_not_offered_again() {
    let mut host = Host::new();
    let master_file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let (version, version_file) = host.make_version("Resume – data roles.docx");
    host.save(
        &master,
        &edit(&master_file, &resume::master_v2_lines()),
        "New job",
    );
    host.offer(std::slice::from_ref(&version));
    let offered = host.offered(&version);
    assert_eq!(offered.len(), 2);
    let decisions: Vec<(Id, String, Decision)> = offered
        .into_iter()
        .map(|(c, id)| (c, id, Decision::Decline))
        .collect();
    let after = host.decide(&version, &version_file, &decisions);
    assert_eq!(after, version_file, "declining writes nothing");
    host.offer(std::slice::from_ref(&version));
    assert!(host.offered(&version).is_empty(), "nothing offered again");
    let status = variant_status(host.state(), &version, &host.blobs).expect("status");
    assert_eq!(status.declined.len(), 2);
    assert_eq!(
        host.state().label(&status.base),
        "v2",
        "declined changes move the base on"
    );
}

#[test]
fn cards_are_decided_by_their_ids_alone() {
    let mut host = Host::new();
    let master_file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let (version, version_file) = host.make_version("Resume – data roles.docx");
    host.save(
        &master,
        &edit(&master_file, &resume::master_v2_lines()),
        "New job",
    );
    host.offer(std::slice::from_ref(&version));
    let cards: Vec<(String, Decision)> = host
        .offered(&version)
        .into_iter()
        .map(|(_, id)| (id, Decision::BringIn))
        .collect();
    let state = host.state().clone();
    let mut ctx = Context {
        now: host.now,
        ids: &mut host.ids,
        author: None,
    };
    let outcome = commands::decide_cards(
        &mut ctx,
        &state,
        &version,
        &version_file,
        &cards,
        &host.blobs,
    )
    .expect("decides");
    let after = host.take(outcome).file.expect("written");
    assert_eq!(
        lines(&after),
        lines(&resume::package("t", &resume::master_v2_lines()).to_bytes()),
        "accepting gives the master's text"
    );

    let state = host.state().clone();
    let mut ctx = Context {
        now: host.now,
        ids: &mut host.ids,
        author: None,
    };
    let unknown = vec![("not-a-card".to_owned(), Decision::BringIn)];
    let refused = commands::decide_cards(&mut ctx, &state, &version, &after, &unknown, &host.blobs);
    assert!(matches!(refused, Err(crate::Error::NotAllowed(_))));
}

/// What Word does to a file it saves without edits: new revision-save ids on
/// the paragraphs and a new modified date. The text is untouched.
fn resaved_by_word(file: &[u8]) -> Vec<u8> {
    let mut package = Package::read(file).expect("reads");
    let doc = String::from_utf8(package.part("word/document.xml").expect("doc").to_vec())
        .expect("utf-8")
        .replace(
            "<w:p>",
            r#"<w:p w:rsidR="00A1B2C3" w:rsidRDefault="00A1B2C3">"#,
        );
    package.set_part("word/document.xml", doc.into_bytes());
    if let Some(core) = package.part("docProps/core.xml") {
        let core = String::from_utf8(core.to_vec()).expect("utf-8").replace(
            "</cp:coreProperties>",
            "<!-- saved again --></cp:coreProperties>",
        );
        package.set_part("docProps/core.xml", core.into_bytes());
    }
    package.to_bytes().expect("bytes")
}

#[test]
fn a_file_word_saved_again_without_edits_has_not_changed() {
    // Found in Phase 2, in Word: sending a copy of a version just after
    // bringing changes in made a new checkpoint ("v3"), because Word had
    // re-saved the file and the bytes no longer matched.
    let mut host = Host::new();
    let master_file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let resaved = resaved_by_word(&master_file);
    assert_ne!(resaved, master_file);
    let state = host.state().clone();
    let mut ctx = Context {
        now: host.now,
        ids: &mut host.ids,
        author: None,
    };
    let outcome = commands::save_checkpoint(&mut ctx, &state, &master, &resaved, "", &host.blobs)
        .expect("saves");
    assert!(outcome.nothing_changed, "no new checkpoint");
    let outcome = commands::send_copy(
        &mut ctx,
        &state,
        &master,
        &resaved,
        "Mentor",
        None,
        true,
        &host.blobs,
    )
    .expect("sends");
    let saved = outcome
        .records
        .iter()
        .filter(|r| matches!(r.event, event::Event::CheckpointSaved { .. }))
        .count();
    assert_eq!(saved, 0, "the copy comes from v1, not a new checkpoint");
    // A real edit still counts.
    let edited = edit(&resaved, &resume::master_v2_lines());
    let outcome = commands::save_checkpoint(&mut ctx, &state, &master, &edited, "", &host.blobs)
        .expect("saves");
    assert!(!outcome.nothing_changed);
}

#[test]
fn referenced_blobs_are_the_manifests_and_change_lists() {
    let mut host = Host::new();
    let master_file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    host.make_version("Resume – data roles.docx");
    host.save(
        &master,
        &edit(&master_file, &resume::master_v2_lines()),
        "New job",
    );
    let state = host.state();
    let referenced = crate::blob::referenced(state);
    let expected: usize = state
        .checkpoints
        .iter()
        .map(|c| 1 + usize::from(c.changes.is_some()))
        .sum();
    assert_eq!(referenced.len(), expected, "no repeats here");
    for hash in &referenced {
        assert!(host.blobs.0.contains_key(hash), "stored");
    }
    let changes = state.checkpoints.iter().filter_map(|c| c.changes.as_ref());
    for hash in changes {
        assert!(referenced.contains(hash));
        assert!(crate::blob::manifest_parts(&host.blobs.0[hash]).is_none());
    }
}

#[test]
fn a_change_copied_in_by_hand_is_detected_as_merged() {
    let mut host = Host::new();
    let master_file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let (version, version_file) = host.make_version("Resume – data roles.docx");
    host.save(
        &master,
        &edit(&master_file, &resume::master_v2_lines()),
        "New job",
    );
    // The person types the job into the tailored version by hand, and saves.
    host.save(
        &version,
        &edit(&version_file, &resume::tailored_copied_job_lines()),
        "Tailored",
    );
    let state = host.state();
    let master_v2 = state.latest(&master).expect("v2").id.clone();
    let records: Vec<_> = state
        .change_states
        .iter()
        .filter(|((n, c, _), _)| *n == version && *c == master_v2)
        .collect();
    assert_eq!(records.len(), 2);
    assert!(
        records
            .iter()
            .all(|(_, r)| r.state == ChangeState::Merged && r.how == How::Detected)
    );
    let latest = state.latest(&version).expect("checkpoint").id.clone();
    assert!(
        state.links.contains(&(latest, master_v2)),
        "linked as a descendant of master v2"
    );
    host.offer(std::slice::from_ref(&version));
    assert!(host.offered(&version).is_empty());
}

#[test]
fn a_version_can_take_v3_without_v2_and_v2_goes_quiet() {
    let mut host = Host::new();
    let master_file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let (version, version_file) = host.make_version("Resume – data roles.docx");
    let version_file = host.save(
        &version,
        &edit(&version_file, &resume::tailored_data_lines()),
        "Tailored",
    );
    let master_file = host.save(
        &master,
        &edit(&master_file, &resume::master_v2_lines()),
        "New job",
    );
    host.save(
        &master,
        &edit(&master_file, &resume::master_v3_lines()),
        "Promoted",
    );
    host.offer(std::slice::from_ref(&version));
    let state = host.state().clone();
    let v3 = state.latest(&master).expect("v3").id.clone();
    let offered = host.offered(&version);
    assert_eq!(offered.len(), 4, "two from v2, two from v3");
    // Bring in only v3's changes.
    let decisions: Vec<(Id, String, Decision)> = offered
        .into_iter()
        .filter(|(c, _)| *c == v3)
        .map(|(c, id)| (c, id, Decision::BringIn))
        .collect();
    assert_eq!(decisions.len(), 2);
    let after = host.decide(&version, &version_file, &decisions);
    let text = lines(&after);
    assert!(
        text.contains(&resume::CORVIA_TITLE_V3.to_owned()),
        "v3's edit is in"
    );
    assert!(
        !text.contains(&resume::NEW_JOB_TITLE.to_owned()),
        "v2's job is not"
    );
    let status = variant_status(host.state(), &version, &host.blobs).expect("status");
    assert_eq!(host.state().label(&status.base), "v3");
    assert!(status.offerable.is_empty(), "nothing newer than the base");
    assert_eq!(
        status.quiet.len(),
        2,
        "v2's job is quiet: in history, not offered"
    );
    // The checkpoint's parents are the version's last and master v3 (decision 0003).
    let latest = host.state().latest(&version).expect("checkpoint");
    assert_eq!(latest.parents.last(), Some(&v3));
    assert!(
        latest
            .note
            .starts_with("Brought in 2 changes from master v3")
    );
}

#[test]
fn a_lineage_change_rejected_in_word_becomes_declined() {
    let mut host = Host::new();
    let master_file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let (version, version_file) = host.make_version("Resume – data roles.docx");
    host.save(
        &master,
        &edit(&master_file, &resume::master_v2_lines()),
        "New job",
    );
    host.offer(std::slice::from_ref(&version));
    let decisions: Vec<(Id, String, Decision)> = host
        .offered(&version)
        .into_iter()
        .map(|(c, id)| (c, id, Decision::BringIn))
        .collect();
    let after = host.decide(&version, &version_file, &decisions);
    assert!(lines(&after).contains(&resume::NEW_JOB_TITLE.to_owned()));
    // In Word the person rejects Lineage's tracked changes: the file is back to v1's text.
    host.save(
        &version,
        &edit(&after, &resume::master_v1_lines()),
        "Rejected in Word",
    );
    let status = variant_status(host.state(), &version, &host.blobs).expect("status");
    assert_eq!(status.declined.len(), 2);
    assert!(
        host.state()
            .change_states
            .values()
            .filter(|r| r.state == ChangeState::Declined)
            .all(|r| r.how == How::Detected)
    );
}

#[test]
fn the_ledger_refuses_gaps_and_unknown_references() {
    let mut host = Host::new();
    host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let mut records = host.ledger.clone();
    assert!(FamilyState::from_records(&records).is_ok());
    records[1].seq = 7;
    assert!(FamilyState::from_records(&records).is_err());
    let mut records = host.ledger.clone();
    if let event::Event::CheckpointSaved { parent_ids, .. } = &mut records[2].event {
        parent_ids.push(Id::parse(&"f".repeat(32)).expect("id"));
    }
    assert!(FamilyState::from_records(&records).is_err());
}

#[test]
fn saving_without_changes_is_a_no_op_and_wrong_files_are_refused() {
    let mut host = Host::new();
    let master_file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let state = host.state().clone();
    let mut ctx = Context {
        now: host.now,
        ids: &mut host.ids,
        author: None,
    };
    let outcome =
        commands::save_checkpoint(&mut ctx, &state, &master, &master_file, "", &host.blobs)
            .expect("saves");
    assert!(outcome.nothing_changed);
    let stranger = resume::package("t", &resume::master_v1_lines()).to_bytes();
    assert!(matches!(
        commands::save_checkpoint(&mut ctx, &state, &master, &stranger, "", &host.blobs),
        Err(crate::Error::WrongFile(_))
    ));
}
