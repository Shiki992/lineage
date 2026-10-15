// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The Phase 1 "done when" scenario, pressed through the panel the way a person
//! would, on in-memory storage and on a real folder:
//!
//! 1. Start tracking a master resume.
//! 2. Make two tailored versions.
//! 3. Add a job to the master and offer it.
//! 4. Bring it into one version and decline it in the other.
//! 5. Send a copy, then open the sent copy.

use std::path::PathBuf;

use lineage::Timestamp;
use lineage_core::docx::props::{CHECKPOINT_ID, FAMILY_ID, custom_properties};
use lineage_core::{Docx, Package};
use lineage_harness::{LocalFolderStorage, MemoryStorage, Panel, Storage};
use lineage_testkit::resume::{self, Line};

const MASTER: &str = "Resume.docx";
const DATA: &str = "Resume – data roles.docx";
const PLATFORM: &str = "Resume – platform roles.docx";
/// 2026-10-15T09:30:00Z.
const START: Timestamp = Timestamp(1_792_056_600_000);

/// The person edits a file in Word: its body becomes the fixture's.
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

fn lines_of(bytes: &[u8]) -> Vec<String> {
    Docx::open(bytes).expect("opens").body_lines()
}

fn scenario<S: Storage>(mut storage: S) -> Panel<S> {
    storage
        .write_file(
            MASTER,
            &resume::package("Avery Quinn master resume", &resume::master_v1_lines()).to_bytes(),
        )
        .expect("puts the master");
    let mut panel = Panel::new(storage, START);

    // 1. Start tracking a master resume.
    let master = panel.start_tracking(MASTER).expect("starts tracking");
    assert_eq!(master.chip, "Master");
    assert_eq!(master.follows, "No tailored versions yet");
    assert_eq!(master.latest.label, "v1");
    assert_eq!(
        master.actions,
        ["Make a tailored version", "Save checkpoint", "Send a copy"]
    );

    // 2. Make two tailored versions.
    panel.make_tailored_version(DATA).expect("makes data roles");
    panel
        .make_tailored_version(PLATFORM)
        .expect("makes platform roles");
    let master = panel.master_panel().expect("panel");
    assert_eq!(master.follows, "2 tailored versions follow this file");
    let rows: Vec<(&str, &str)> = master
        .versions
        .iter()
        .map(|v| (v.name.as_str(), v.status.as_str()))
        .collect();
    assert_eq!(rows, [(DATA, "Has v1"), (PLATFORM, "Has v1")]);
    assert!(master.callout.is_none());

    // 3. Add a job to the master (in Word), save a checkpoint, and offer it.
    edit_in_word(&mut panel.storage, MASTER, &resume::master_v2_lines());
    let changed = panel.show_what_changed(MASTER).expect("what changed");
    assert_eq!(changed.headline, "2 changes since v1");
    assert_eq!(
        changed.changes[0].summary,
        "Added under 'Experience': 'Senior data engineer, Halden Robotics, 2023 to now'"
    );
    panel
        .save_checkpoint(MASTER, "Added the Halden Robotics job")
        .expect("saves");
    let master = panel.master_panel().expect("panel");
    assert_eq!(master.latest.label, "v2");
    let callout = master.callout.expect("the call-out appears");
    assert_eq!(
        callout.text,
        "Your master has 2 changes your tailored versions don't have"
    );
    assert_eq!(callout.button, "Offer them to both versions");
    let master = panel.offer_to_versions().expect("offers");
    assert!(master.callout.is_none(), "offered: the call-out goes");

    // 4a. Bring it into the data version.
    let updates = panel.updates(DATA).expect("updates");
    assert_eq!(updates.headline, "2 updates from the master");
    assert_eq!(updates.groups[0].label, "From master v2");
    let cards: Vec<String> = updates.groups[0]
        .view
        .changes
        .iter()
        .map(|c| c.id.clone())
        .collect();
    assert_eq!(cards.len(), 2);
    let labels: Vec<&str> = updates.groups[0].view.changes[0]
        .choices
        .iter()
        .map(|c| c.label.as_str())
        .collect();
    assert_eq!(labels, ["Bring in", "Decline"]);
    let card_refs: Vec<&str> = cards.iter().map(String::as_str).collect();
    panel.bring_in(DATA, &card_refs).expect("brings in");
    let data_bytes = panel.storage.read_file(DATA).expect("data roles");
    let data_lines = lines_of(&data_bytes);
    assert!(data_lines.contains(&resume::NEW_JOB_TITLE.to_owned()));
    let document = String::from_utf8(
        Package::read(&data_bytes)
            .expect("reads")
            .part("word/document.xml")
            .expect("doc")
            .to_vec(),
    )
    .expect("utf-8");
    assert!(
        document.contains(r#"w:author="Lineage""#),
        "written as tracked changes by Lineage"
    );
    assert!(document.contains(r#"w:date="2026-10-15T09:"#));
    let state = panel.state().expect("state");
    let data_node = state
        .nodes
        .iter()
        .find(|n| n.storage_ref == DATA)
        .expect("node")
        .id
        .clone();
    let latest = state.latest(&data_node).expect("checkpoint");
    assert_eq!(latest.note, "Brought in 2 changes from master v2");
    let master_v2 = state
        .latest(&state.master().expect("master").id)
        .expect("v2")
        .id
        .clone();
    assert_eq!(latest.parents.len(), 2);
    assert_eq!(latest.parents[1], master_v2);
    assert_eq!(
        panel.version_panel(DATA).expect("panel").status,
        "Made from master v1 · has v2"
    );

    // 4b. Decline it in the platform version.
    let updates = panel.updates(PLATFORM).expect("updates");
    let cards: Vec<String> = updates.groups[0]
        .view
        .changes
        .iter()
        .map(|c| c.id.clone())
        .collect();
    let card_refs: Vec<&str> = cards.iter().map(String::as_str).collect();
    let before = panel.storage.read_file(PLATFORM).expect("platform");
    panel.decline(PLATFORM, &card_refs).expect("declines");
    assert_eq!(
        panel.storage.read_file(PLATFORM).expect("platform"),
        before,
        "declining changes nothing"
    );
    assert!(panel.offer_to_versions().is_err(), "nothing left to offer");
    assert_eq!(
        panel.updates(PLATFORM).expect("updates").headline,
        "You're up to date with the master"
    );

    // 5. Send a copy of the data version, then open the sent copy.
    let copy_name = panel
        .send_a_copy(
            DATA,
            "Northgate Analytics",
            Some("Data engineer role"),
            true,
        )
        .expect("sends");
    assert_eq!(copy_name, "Resume – data roles (Northgate Analytics).docx");
    let saved = panel.storage.read_file(&copy_name).expect("saved copy");
    let sent = panel.what_ive_sent(None).expect("what I've sent");
    assert_eq!(sent.note, "Each copy is kept exactly as sent.");
    assert_eq!(sent.groups.len(), 1);
    assert_eq!(sent.groups[0].month, "October 2026");
    let row = &sent.groups[0].rows[0];
    assert_eq!(row.recipient, "Northgate Analytics");
    assert_eq!(row.version, "Resume – data roles · v2");
    assert_eq!(row.format, "Word");
    assert_eq!(row.date, "15 Oct");
    let opened = panel
        .open_what_they_received(&row.id)
        .expect("opens what they received");
    assert_eq!(opened, saved, "exactly what was sent");

    // The copy is clean: the tracked changes are accepted, and only the two ids remain.
    assert_eq!(lines_of(&opened), data_lines);
    let package = Package::read(&opened).expect("reads");
    let document =
        String::from_utf8(package.part("word/document.xml").expect("doc").to_vec()).expect("utf-8");
    assert!(
        !document.contains("<w:ins") && !document.contains("<w:del") && !document.contains("rsid")
    );
    let names: Vec<String> = custom_properties(&package)
        .expect("props")
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, [FAMILY_ID, CHECKPOINT_ID]);
    assert!(!package.part_names().any(|n| n.contains("comments")));
    let docx = Docx::open(&opened).expect("opens");
    assert!(
        docx.stories
            .iter()
            .all(|s| s.blocks.iter().all(|b| match b {
                lineage_core::docx::Block::Paragraph(p) => !p.has_revisions,
                _ => true,
            }))
    );
    panel
}

#[test]
fn the_personal_resume_scenario_in_memory() {
    scenario(MemoryStorage::default());
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn the_personal_resume_scenario_on_disk() {
    let root = temp_dir("personal-resume");
    let panel = scenario(LocalFolderStorage::new(&root).expect("folder"));
    let family = panel.state().expect("state").family_id;
    let family_dir = root.join("Lineage").join(family.as_str());
    assert!(
        family_dir.join("ledger.jsonl").is_file(),
        "the ledger is in Lineage/<familyId>/"
    );
    let blobs: Vec<_> = std::fs::read_dir(family_dir.join("blobs"))
        .expect("blobs")
        .collect();
    assert!(!blobs.is_empty());
    for entry in blobs {
        let entry = entry.expect("entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        assert_eq!(name.len(), 64, "blobs are named by their SHA-256");
        let bytes = std::fs::read(entry.path()).expect("blob");
        assert_eq!(lineage::blob::BlobHash::of(&bytes).as_str(), name);
    }
    let files = panel.storage.file_names().expect("files");
    assert!(
        files.contains(&MASTER.to_owned())
            && files.contains(&DATA.to_owned())
            && files.contains(&PLATFORM.to_owned())
    );
    // A fresh panel over the same folder sees the same family.
    let mut again = Panel::new(LocalFolderStorage::new(&root).expect("folder"), START);
    let state = again.open_family_of(DATA).expect("reopens");
    assert_eq!(state.family_id, family);
    assert_eq!(state.sent.len(), 1);
}
