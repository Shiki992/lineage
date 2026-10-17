// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! More scenarios through the panel: text typed in by hand, taking v3 without
//! v2, overlaps, sending without the id, and two devices appending at once.

use lineage::Timestamp;
use lineage::blob::Blob;
use lineage_core::docx::props::custom_properties;
use lineage_core::merge::Decision;
use lineage_core::{Docx, Package};
use lineage_harness::{MemoryStorage, Panel, Storage, StorageError};
use lineage_testkit::resume::{self, Line};

const MASTER: &str = "Resume.docx";
const DATA: &str = "Resume – data roles.docx";
const START: Timestamp = Timestamp(1_792_056_600_000);

fn edit_in_word<S: Storage>(storage: &mut S, file: &str, lines: &[Line]) {
    let source = Package::read(&resume::package("t", lines).to_bytes()).expect("reads");
    let mut package = Package::read(&storage.read_file(file).expect("file")).expect("reads");
    package.set_part(
        "word/document.xml",
        source.part("word/document.xml").expect("doc").to_vec(),
    );
    storage
        .write_file(file, &package.to_bytes().expect("bytes"))
        .expect("writes");
}

fn tracked_with_one_version() -> Panel<MemoryStorage> {
    let mut storage = MemoryStorage::default();
    storage
        .write_file(
            MASTER,
            &resume::package("t", &resume::master_v1_lines()).to_bytes(),
        )
        .expect("puts");
    let mut panel = Panel::new(storage, START);
    panel.start_tracking(MASTER).expect("starts");
    panel.make_tailored_version(DATA).expect("makes");
    panel
}

fn lines(panel: &Panel<MemoryStorage>, file: &str) -> Vec<String> {
    Docx::open(&panel.storage.read_file(file).expect("file"))
        .expect("opens")
        .body_lines()
}

#[test]
fn text_typed_in_by_hand_is_not_offered() {
    let mut panel = tracked_with_one_version();
    edit_in_word(&mut panel.storage, MASTER, &resume::master_v2_lines());
    panel.save_checkpoint(MASTER, "New job").expect("saves");
    edit_in_word(
        &mut panel.storage,
        DATA,
        &resume::tailored_copied_job_lines(),
    );
    panel
        .save_checkpoint(DATA, "Tailored, with the new job typed in")
        .expect("saves");
    let master = panel.master_panel().expect("panel");
    assert!(master.callout.is_none(), "the version already has it");
    assert_eq!(master.versions[0].status, "Has v2");
}

#[test]
fn taking_v3_without_v2() {
    let mut panel = tracked_with_one_version();
    edit_in_word(&mut panel.storage, DATA, &resume::tailored_data_lines());
    panel.save_checkpoint(DATA, "Tailored").expect("saves");
    edit_in_word(&mut panel.storage, MASTER, &resume::master_v2_lines());
    panel.save_checkpoint(MASTER, "New job").expect("saves");
    edit_in_word(&mut panel.storage, MASTER, &resume::master_v3_lines());
    panel.save_checkpoint(MASTER, "Promoted").expect("saves");
    panel.offer_to_versions().expect("offers");
    let updates = panel.updates(DATA).expect("updates");
    assert_eq!(updates.headline, "4 updates from the master");
    let v3 = updates
        .groups
        .iter()
        .find(|g| g.label == "From master v3")
        .expect("v3 group");
    let cards: Vec<String> = v3.view.ids().into_iter().map(str::to_owned).collect();
    let refs: Vec<&str> = cards.iter().map(String::as_str).collect();
    panel.bring_in(DATA, &refs).expect("brings in v3");
    let text = lines(&panel, DATA);
    assert!(text.contains(&resume::CORVIA_TITLE_V3.to_owned()));
    assert!(!text.contains(&resume::NEW_JOB_TITLE.to_owned()));
    // v2's job goes quiet: no longer offered, and the version shows v3.
    assert_eq!(
        panel.updates(DATA).expect("updates").headline,
        "You're up to date with the master"
    );
    assert_eq!(
        panel.master_panel().expect("panel").versions[0].status,
        "Has v3"
    );
}

#[test]
fn using_the_masters_side_of_an_overlap() {
    let mut panel = tracked_with_one_version();
    edit_in_word(
        &mut panel.storage,
        DATA,
        &resume::tailored_same_para_lines(),
    );
    panel
        .save_checkpoint(DATA, "Logistics angle")
        .expect("saves");
    let mut v2 = resume::master_v1_lines();
    let corvia = v2
        .iter()
        .position(|l| *l == Line::H2(resume::CORVIA_TITLE.into()))
        .expect("corvia");
    v2[corvia] = Line::H2(resume::CORVIA_TITLE_V3.into());
    edit_in_word(&mut panel.storage, MASTER, &v2);
    panel.save_checkpoint(MASTER, "Promoted").expect("saves");
    panel.offer_to_versions().expect("offers");
    let updates = panel.updates(DATA).expect("updates");
    let overlap = &updates.groups[0].view.overlaps[0];
    let labels: Vec<&str> = overlap.choices.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, ["Keep mine", "Use the master's"]);
    let id = overlap.id.clone();
    // Bringing in without choosing a side is refused; choosing works.
    assert!(panel.bring_in(DATA, &[&id]).is_err());
    panel
        .decide(DATA, &[(&id, Decision::UseMasters)])
        .expect("uses the master's");
    assert!(lines(&panel, DATA).contains(&resume::CORVIA_TITLE_V3.to_owned()));
}

#[test]
fn sending_without_the_id() {
    let mut panel = tracked_with_one_version();
    let name = panel
        .send_a_copy(DATA, "A mentor", None, false)
        .expect("sends");
    let copy = panel.storage.read_file(&name).expect("copy");
    let props = custom_properties(&Package::read(&copy).expect("reads")).expect("props");
    assert!(props.is_empty(), "no Lineage ids, no properties at all");
    let sent = panel.what_ive_sent(Some("mentor")).expect("list");
    assert_eq!(sent.groups[0].rows.len(), 1);
    assert!(
        panel
            .what_ive_sent(Some("investor"))
            .expect("list")
            .groups
            .is_empty()
    );
}

#[test]
fn unsaved_changes_are_checkpointed_before_sending() {
    let mut panel = tracked_with_one_version();
    edit_in_word(&mut panel.storage, DATA, &resume::tailored_data_lines());
    panel
        .send_a_copy(DATA, "Northgate Analytics", None, true)
        .expect("sends");
    let state = panel.state().expect("state");
    let node = state
        .nodes
        .iter()
        .find(|n| n.storage_ref == DATA)
        .expect("node");
    let latest = state.latest(&node.id).expect("checkpoint");
    assert_eq!(latest.note, "Saved before sending to Northgate Analytics");
    assert_eq!(state.sent[0].from, latest.id);
}

/// Storage that lets another "device" append once, between our read and our append.
struct Racing {
    inner: MemoryStorage,
    race: bool,
}

impl Storage for Racing {
    fn read_file(&self, name: &str) -> Result<Vec<u8>, StorageError> {
        self.inner.read_file(name)
    }
    fn write_file(&mut self, name: &str, bytes: &[u8]) -> Result<(), StorageError> {
        self.inner.write_file(name, bytes)
    }
    fn file_names(&self) -> Result<Vec<String>, StorageError> {
        self.inner.file_names()
    }
    fn read_ledger(&self, family: &lineage::Id) -> Result<(Vec<u8>, u64), StorageError> {
        self.inner.read_ledger(family)
    }
    fn append_ledger(
        &mut self,
        family: &lineage::Id,
        expected: u64,
        lines: &[u8],
    ) -> Result<u64, StorageError> {
        if self.race {
            self.race = false;
            // As if another device's append had landed between our read and ours.
            return Err(StorageError::Conflict);
        }
        self.inner.append_ledger(family, expected, lines)
    }
    fn put_blob(&mut self, family: &lineage::Id, blob: &Blob) -> Result<(), StorageError> {
        self.inner.put_blob(family, blob)
    }
    fn get_blob(&self, family: &lineage::Id, hash: &lineage::blob::BlobHash) -> Option<Vec<u8>> {
        self.inner.get_blob(family, hash)
    }
    fn families(&self) -> Result<Vec<lineage::Id>, StorageError> {
        self.inner.families()
    }
    fn read_index(&self) -> Result<Option<(Vec<u8>, String)>, StorageError> {
        self.inner.read_index()
    }
    fn write_index(
        &mut self,
        bytes: &[u8],
        expected: Option<&str>,
    ) -> Result<String, StorageError> {
        self.inner.write_index(bytes, expected)
    }
}

#[test]
fn an_append_conflict_is_retried_on_fresh_state() {
    let mut storage = MemoryStorage::default();
    storage
        .write_file(
            MASTER,
            &resume::package("t", &resume::master_v1_lines()).to_bytes(),
        )
        .expect("puts");
    let mut panel = Panel::new(
        Racing {
            inner: storage,
            race: false,
        },
        START,
    );
    panel.start_tracking(MASTER).expect("starts");
    panel.storage.race = true;
    panel
        .make_tailored_version(DATA)
        .expect("succeeds on the second try");
    let state = panel.state().expect("state");
    assert_eq!(state.variants().count(), 1, "made once, not twice");
}

#[test]
fn blobs_must_match_their_names() {
    let mut storage = MemoryStorage::default();
    let family = lineage::Id::parse(&"1".repeat(32)).expect("id");
    let mut blob = Blob::new(b"hello".to_vec());
    blob.bytes.push(b'!');
    assert!(matches!(
        storage.put_blob(&family, &blob),
        Err(StorageError::BadBlob)
    ));
}

#[test]
fn local_files_stay_inside_the_folder() {
    let root = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("folder-safety");
    let _ = std::fs::remove_dir_all(&root);
    let mut storage = lineage_harness::LocalFolderStorage::new(&root).expect("folder");
    for name in ["../escape.docx", "a/b.docx", "..", "C:evil.docx", ""] {
        assert!(storage.write_file(name, b"x").is_err(), "{name}");
    }
    assert!(storage.write_file("ok.docx", b"x").is_ok());
}
