// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! "My documents" (Phase 3B), pressed through the panel the way a person would,
//! in memory and on a real folder:
//!
//! 1. Three families exist: a resume with tailored versions, sent copies and
//!    files that came back; a cover letter; a pitch deck from PowerPoint.
//! 2. The library index is out of date: My documents reads the ledgers that
//!    moved on, shows all three, and rewrites the index.
//! 3. Open the resume: every file's checkpoints, the milestone, the copies
//!    sent, and the family tree.
//! 4. Compare two versions; open a sent copy exactly as sent.
//! 5. A version's file was moved: relink it to the moved file, and only that.
//! 6. Restore an old checkpoint as a new version; nothing that existed changes.
//! 7. A damaged ledger is shown from what can be read.
//! 8. A family with 300 checkpoints and 50 copies sent stays quick.

use std::path::PathBuf;
use std::time::Instant;

use lineage::graph::{EdgeKind, LaneKind};
use lineage_harness::accounts::{self, PLATFORM, PLATFORM_MOVED, START};
use lineage_harness::{LocalFolderStorage, MemoryStorage, Panel, Storage, Tamper};

fn journey<S: Storage + Tamper>(mut panel: Panel<S>) {
    let made = accounts::three_families(&mut panel).expect("account");

    // 2. The index is stale; the ledgers win.
    let (old_index, _) = panel.storage.read_index().expect("reads").expect("index");
    let library = panel.my_documents().expect("library");
    let names: Vec<&str> = library
        .entries
        .iter()
        .map(|e| e.facts.name.as_str())
        .collect();
    assert_eq!(
        names,
        ["Cover letter", "Resume", "Pitch deck"],
        "newest activity first"
    );
    let (new_index, _) = panel.storage.read_index().expect("reads").expect("index");
    assert_ne!(old_index, new_index, "the index was rewritten");
    assert_eq!(panel.my_documents().expect("again"), library);
    let resume = &library.entries[1];
    assert_eq!(resume.facts.family_id, made.resume);
    assert_eq!(
        resume.facts.tailored, 2,
        "the archived version isn't counted"
    );
    assert_eq!((resume.facts.sent, resume.facts.returned), (4, 2));
    assert_eq!(resume.kind, "Word document");
    let deck = &library.entries[2];
    assert_eq!(deck.kind, "PowerPoint deck");
    assert!(deck.can_open, "decks open since Phase 4");
    assert_eq!((deck.facts.tailored, deck.facts.sent), (1, 2));

    // 3. The resume's page and tree.
    let (page, graph) = panel.document(&made.resume).expect("page");
    assert!(page.can_change);
    let master = &page.files[0];
    let labels: Vec<&str> = master.entries.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels, ["v4", "v3", "v2", "v1"]);
    assert_eq!(
        master.entries[2].milestone.as_deref(),
        Some("Sent to ten companies")
    );
    assert_eq!(
        page.sent.groups.iter().map(|g| g.rows.len()).sum::<usize>(),
        4
    );
    assert!(page.files.iter().any(|f| f.archived));
    let kinds: Vec<LaneKind> = graph.lanes.iter().map(|l| l.kind).collect();
    assert_eq!(
        kinds,
        [
            LaneKind::Master,
            LaneKind::Version,
            LaneKind::Version,
            LaneKind::Version,
            LaneKind::Sent,
            LaneKind::Returned,
        ]
    );
    assert!(graph.edges.iter().any(|e| e.kind == EdgeKind::BroughtIn));
    assert!(graph.edges.iter().any(|e| e.kind == EdgeKind::KeptInto));

    // 4. Compare, and a copy exactly as sent.
    let (v1, v3) = (&master.entries[3].id, &master.entries[1].id);
    let changed = panel.compare(&made.resume, v1, v3).expect("compares");
    assert!(
        changed.headline.ends_with("from Resume v1 to Resume v3"),
        "{}",
        changed.headline
    );
    assert!(!changed.changes.is_empty());
    let pellow = page.sent.groups[0]
        .rows
        .iter()
        .find(|r| r.recipient == "Pellow Ventures")
        .expect("Pellow");
    let sent = panel.open_as_sent(&made.resume, &pellow.id).expect("opens");
    assert_eq!(
        sent,
        panel
            .storage
            .read_file("Resume (Pellow Ventures).docx")
            .expect("file")
    );

    // 5. The platform version's file moved.
    assert!(panel.storage.read_file(PLATFORM).is_err());
    let platform = page
        .files
        .iter()
        .find(|f| f.storage_ref == PLATFORM)
        .expect("platform")
        .node
        .clone();
    let data = page.files[1].node.clone();
    assert!(
        panel.relink(&made.resume, &data, PLATFORM_MOVED).is_err(),
        "another version's file is refused"
    );
    panel
        .relink(&made.resume, &platform, PLATFORM_MOVED)
        .expect("relinks");
    let (page, _) = panel.document(&made.resume).expect("page");
    assert!(page.files.iter().any(|f| f.storage_ref == PLATFORM_MOVED));

    // 6. Restore v1 as a new version.
    let files_before: Vec<(String, Vec<u8>)> = panel
        .storage
        .file_names()
        .expect("names")
        .into_iter()
        .map(|n| {
            let b = panel.storage.read_file(&n).expect("reads");
            (n, b)
        })
        .collect();
    let restored = panel
        .restore_as_new_version(
            &made.resume,
            v1,
            "Tailored versions/Resume – first draft.docx",
        )
        .expect("restores");
    for (name, bytes) in &files_before {
        assert_eq!(
            &panel.storage.read_file(name).expect("still there"),
            bytes,
            "{name}"
        );
    }
    let (page, graph) = panel.document(&made.resume).expect("page");
    let first = page
        .files
        .iter()
        .find(|f| f.node == restored)
        .expect("restored");
    assert_eq!(first.status, "Restored from Resume v1");
    assert!(
        graph
            .edges
            .iter()
            .any(|e| e.kind == EdgeKind::MadeFrom && e.to == first.entries[0].id)
    );
    let after = panel.my_documents().expect("library");
    let entry = after
        .entries
        .iter()
        .find(|e| e.facts.family_id == made.resume)
        .expect("resume");
    assert_eq!(entry.facts.tailored, 3);
}

#[test]
fn my_documents_in_memory() {
    journey(Panel::new(MemoryStorage::default(), START));
}

#[test]
fn my_documents_on_disk() {
    let dir = scratch("my-documents");
    journey(Panel::new(
        LocalFolderStorage::new(&dir).expect("folder"),
        START,
    ));
    assert!(dir.join("Lineage").join("index.json").exists());
    assert!(
        dir.join("Tailored versions")
            .join("Resume – first draft.docx")
            .exists()
    );
    std::fs::remove_dir_all(&dir).expect("cleans up");
}

#[test]
fn a_damaged_ledger_shows_what_can_be_read() {
    let mut panel = Panel::new(MemoryStorage::default(), START);
    let family = accounts::damaged(&mut panel).expect("account");
    let library = panel.my_documents().expect("library");
    let entry = &library.entries[0];
    assert!(!entry.can_open);
    assert!(
        entry
            .facts
            .damage
            .as_deref()
            .expect("damage")
            .contains("later entry is damaged")
    );
    let (page, _) = panel.document(&family).expect("page");
    assert!(!page.can_change);
    let v1 = page.files[0].entries.last().expect("v1").id.clone();
    assert!(
        panel.mark_milestone(&family, &v1, Some("x")).is_err(),
        "nothing is added"
    );
}

#[test]
fn a_large_family_stays_quick() {
    let mut panel = Panel::new(MemoryStorage::default(), START);
    let family = accounts::large(&mut panel, 300, 50).expect("account");
    let started = Instant::now();
    let library = panel.my_documents().expect("library");
    let (page, graph) = panel.document(&family).expect("page");
    let elapsed = started.elapsed();
    assert_eq!(library.entries[0].facts.checkpoints, 300);
    assert_eq!(library.entries[0].facts.sent, 50);
    assert_eq!(
        page.files.iter().map(|f| f.entries.len()).sum::<usize>(),
        300
    );
    assert_eq!(graph.nodes.len(), 350);
    // Generous for an unoptimised test build; the release engine is measured
    // in the browser (STATUS).
    assert!(elapsed.as_millis() < 2_000, "{elapsed:?}");
    let a = &page.files[0].entries[0].id;
    let b = page.files[0].entries.last().map(|e| &e.id).expect("v1");
    assert!(panel.compare(&family, b, a).is_ok());
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lineage-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// The workbook account: two workbook families in My documents, a comparison
/// in the five categories, a sent workbook opened exactly as sent, a restore,
/// and a family with macros kept as `.xlsm`.
fn workbook_account<S: Storage + Tamper>(mut panel: Panel<S>) {
    let made = accounts::workbooks(&mut panel).expect("account");
    let library = panel.my_documents().expect("library");
    let mut names: Vec<&str> = library
        .entries
        .iter()
        .map(|e| e.facts.name.as_str())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["Model with macros", "Startup model"]);

    let (page, _) = panel.document(&made.model).expect("page");
    assert_eq!(page.format, "xlsx");
    assert_eq!(page.files.len(), 3, "the master and two versions");
    let master = &page.files[0];
    let (v1, v2) = (
        master.entries.last().expect("v1").id.clone(),
        master.entries.first().expect("latest").id.clone(),
    );
    let view = panel.compare(&made.model, &v1, &v2).expect("compares");
    let wv = view.workbook.expect("a workbook view");
    assert_eq!(wv.headline, "1 input, 83 knock-on changes");
    assert_eq!(wv.groups[0].title, "Inputs");

    let row = &page.sent.groups[0].rows[0];
    assert_eq!(row.recipient, "Harbor Lane Capital");
    assert_eq!(row.format, "Excel");
    let bytes = panel.open_as_sent(&made.model, &row.id).expect("opens");
    let on_disk = panel
        .storage
        .read_file("Startup model – Harbor Lane (Harbor Lane Capital).xlsx")
        .expect("the copy that left");
    assert_eq!(bytes, on_disk, "exactly as sent");

    let restored = "Tailored versions/Startup model – first price.xlsx";
    panel
        .restore_as_new_version(&made.model, &v1, restored)
        .expect("restores");
    let wb = lineage_core::xlsx::Workbook::open(&panel.storage.read_file(restored).expect("file"))
        .expect("opens");
    assert!(!wb.macro_enabled);

    let file = panel
        .storage
        .read_file(accounts::MACROS_VERSION)
        .expect("the version with macros");
    let wb = lineage_core::xlsx::Workbook::open(&file).expect("opens");
    assert!(
        wb.macro_enabled,
        "a version of a workbook with macros keeps them"
    );
    let _ = made.macros;
}

#[test]
fn the_workbook_account_in_memory() {
    workbook_account(Panel::new(MemoryStorage::default(), START));
}

#[test]
fn the_workbook_account_on_disk() {
    let dir = std::env::temp_dir().join(format!("lineage-workbook-account-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    workbook_account(Panel::new(
        LocalFolderStorage::new(&dir).expect("folder"),
        START,
    ));
    let _ = std::fs::remove_dir_all(&dir);
}
