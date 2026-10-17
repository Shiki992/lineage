// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! "My documents": the library, a document's page, compare, restore,
//! milestones and relinking, through the commands with an in-memory host.

use lineage_core::Package;
use lineage_core::docx::props::read_ids;
use lineage_testkit::resume;

use crate::changes::variant_status;
use crate::commands;
use crate::error::Error;
use crate::event::{self, Event, Role};
use crate::ids::Id;
use crate::library::{EntryFacts, build_library};
use crate::read::read_lenient;
use crate::testfamily::everything;
use crate::tests::lines;
use crate::views::document_page;

#[test]
fn the_library_counts_what_the_family_holds() {
    let family = everything();
    let ledger = family.host.ledger_bytes();
    let id = family.host.state().family_id.to_string();
    let library = build_library(&[
        (id.as_str(), &ledger),
        ("not-an-id", b"{}"),
        (&"e".repeat(32), b""),
    ]);
    assert_eq!(library.entries.len(), 1);
    assert_eq!(library.empty, None);
    let entry = &library.entries[0];
    assert_eq!(entry.facts.name, "Resume");
    assert_eq!(entry.facts.master_file.as_deref(), Some("Resume.docx"));
    assert_eq!(
        (entry.facts.tailored, entry.facts.sent, entry.facts.returned),
        (2, 2, 1),
        "the archived version isn't counted"
    );
    assert_eq!(entry.kind, "Word document");
    assert_eq!(
        entry.summary,
        format!(
            "2 tailored versions · {} checkpoints · 2 sent · 1 came back",
            entry.facts.checkpoints
        )
    );
    assert!(entry.last_activity.starts_with("Last change 15 Oct 2026"));
    assert!(entry.can_open);
    assert_eq!(entry.facts, EntryFacts::of_state(family.host.state()));

    let empty = build_library(&[]);
    assert!(empty.empty.expect("empty state").contains("Start tracking"));
}

#[test]
fn a_damaged_ledger_is_listed_from_what_can_be_read() {
    let family = everything();
    let mut ledger = family.host.ledger_bytes();
    ledger.truncate(ledger.len() - 30);
    let id = family.host.state().family_id.to_string();
    let library = build_library(&[(id.as_str(), &ledger)]);
    let entry = &library.entries[0];
    assert!(!entry.can_open);
    assert!(
        entry
            .facts
            .damage
            .as_deref()
            .expect("damage")
            .starts_with("Lineage could read this history up to 15 Oct. 1 later entry is damaged")
    );
    assert_eq!(entry.facts.name, "Resume");

    let read = read_lenient(&ledger);
    let page = document_page(read.state.as_ref().expect("state"), read.note());
    assert!(!page.can_change);
    assert_eq!(page.note, read.note());
}

#[test]
fn the_page_lists_each_file_newest_first_with_milestones_and_sent_copies() {
    let family = everything();
    let page = document_page(family.host.state(), None);
    assert!(page.can_change);
    assert_eq!(page.note, None);
    let names: Vec<&str> = page.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Resume.docx",
            "Resume – data roles.docx",
            "Resume – platform roles.docx",
            "Resume – old.docx"
        ]
    );
    let master = &page.files[0];
    assert_eq!(master.chip, "Master");
    let labels: Vec<&str> = master.entries.iter().map(|e| e.label.as_str()).collect();
    assert_eq!(labels, ["v3", "v2", "v1"]);
    assert_eq!(
        master.entries[1].milestone.as_deref(),
        Some("Sent to ten companies")
    );
    assert_eq!(master.entries[1].sent.as_deref(), Some("Sent to Mentor"));
    assert_eq!(page.files[1].status, "Made from master v1");
    assert!(page.files[3].archived);
    assert_eq!(page.sent.groups[0].rows.len(), 2);
}

#[test]
fn a_workbook_family_shows_its_history_but_offers_nothing_to_open() {
    let mut state = everything().host.state().clone();
    state.format = "pptx".to_owned();
    let page = document_page(&state, None);
    assert!(page.can_change, "decks can be opened since Phase 4");
    assert_eq!(page.note, None);
    assert_eq!(page.kind, "PowerPoint deck");
    state.format = "xlsx".to_owned();
    let page = document_page(&state, None);
    assert!(!page.can_change);
    assert!(
        page.note
            .expect("note")
            .starts_with("Comes with Excel support")
    );
    assert_eq!(page.kind, "Excel workbook");
    let blobs = crate::blob::MemoryBlobs::default();
    let (a, b) = (&state.checkpoints[0].id, &state.checkpoints[1].id);
    assert!(matches!(
        commands::compare(&state, a, b, &blobs),
        Err(Error::NotAllowed(_))
    ));
}

#[test]
fn compare_shows_what_changed_between_any_two_checkpoints() {
    let family = everything();
    let state = family.host.state();
    let masters = state.master_checkpoints();
    let (v1, v2) = (masters[0].id.clone(), masters[1].id.clone());
    let view = commands::compare(state, &v1, &v2, &family.host.blobs).expect("compares");
    assert_eq!(view.headline, "2 changes from Resume v1 to Resume v2");
    assert!(view.changes.iter().all(|c| !c.text.is_empty()));
    let back = commands::compare(state, &v2, &v1, &family.host.blobs).expect("compares");
    assert_eq!(back.changes.len(), view.changes.len());
    let same = commands::compare(state, &v1, &v1, &family.host.blobs).expect("compares");
    assert_eq!(same.headline, "No changes from Resume v1 to Resume v1");

    let data = state.latest(&family.data).expect("data").id.clone();
    let across = commands::compare(state, &v2, &data, &family.host.blobs).expect("compares");
    assert!(
        across
            .headline
            .ends_with("from Resume v2 to Resume – data roles v2"),
        "{}",
        across.headline
    );
}

#[test]
fn restoring_makes_a_new_version_and_changes_nothing_else() {
    let mut family = everything();
    let before = family.host.state().clone();
    let ledger_before = family.host.ledger_bytes();
    let v1 = before.master_checkpoints()[0].id.clone();
    let mut ctx = commands::Context {
        now: family.host.now,
        ids: &mut family.host.ids,
        author: None,
    };
    let outcome = commands::restore_as_new_version(
        &mut ctx,
        &before,
        &v1,
        "Resume – first draft.docx",
        "Tailored versions/Resume – first draft.docx",
        &family.host.blobs,
    )
    .expect("restores");
    assert!(outcome.file.is_none(), "the open file is never written");
    assert!(outcome.records.iter().all(|r| match &r.event {
        Event::NodeAdded { .. }
        | Event::ChangeStateRecorded { .. }
        | Event::CheckpointLinked { .. } => true,
        Event::CheckpointSaved { node_id, .. } => Some(node_id) == outcome.created.as_ref(),
        _ => false,
    }));
    let new_file = outcome.new_file.clone().expect("a new file");
    let node = outcome.created.clone().expect("node");
    family.host.take(outcome);
    let ledger_after = family.host.ledger_bytes();
    assert!(ledger_after.starts_with(&ledger_before), "only appended");

    let state = family.host.state();
    let made = state.node(&node).expect("node");
    assert_eq!(made.role, Role::Variant);
    assert_eq!(made.made_from.as_ref(), Some(&v1));
    let first = state.latest(&node).expect("checkpoint");
    assert_eq!(first.parents, vec![v1.clone()]);
    assert_eq!(first.note, "Restored from Resume v1");
    let ids = read_ids(&Package::read(&new_file.bytes).expect("reads"))
        .expect("ids")
        .expect("has ids");
    assert_eq!(ids.checkpoint, first.id.as_str());
    assert_eq!(
        lines(&new_file.bytes),
        lines(
            &crate::blob::restore(&before.master_checkpoints()[0].snapshot, &family.host.blobs)
                .expect("restores")
        )
    );
    // It follows the master: v2's changes are offered to it.
    let status = variant_status(state, &node, &family.host.blobs).expect("status");
    assert!(!status.offerable.is_empty());
}

#[test]
fn restoring_a_versions_checkpoint_follows_the_master_it_came_from() {
    let mut family = everything();
    let state = family.host.state().clone();
    let data_latest = state.latest(&family.data).expect("latest").id.clone();
    let mut ctx = commands::Context {
        now: family.host.now,
        ids: &mut family.host.ids,
        author: None,
    };
    let outcome = commands::restore_as_new_version(
        &mut ctx,
        &state,
        &data_latest,
        "Resume – data again.docx",
        "Tailored versions/Resume – data again.docx",
        &family.host.blobs,
    )
    .expect("restores");
    let node = outcome.created.clone().expect("node");
    family.host.take(outcome);
    let state = family.host.state();
    let v2 = state.master_checkpoints()[1].id.clone();
    assert_eq!(
        state.node(&node).expect("node").made_from.as_ref(),
        Some(&v2)
    );
    let page = document_page(state, None);
    let restored = page.files.iter().find(|f| f.node == node).expect("file");
    assert_eq!(restored.status, "Restored from Resume – data roles v2");
    // Its changes from v2 are already there, found by content.
    let status = variant_status(state, &node, &family.host.blobs).expect("status");
    assert!(
        status
            .offerable
            .iter()
            .all(|p| p.checkpoint != v2 || !p.offered)
    );
}

#[test]
fn a_milestone_is_marked_changed_and_taken_off() {
    let mut family = everything();
    let v1 = family.host.state().master_checkpoints()[0].id.clone();
    family.host.milestone(&v1, Some("  First draft  "));
    assert_eq!(
        family
            .host
            .state()
            .checkpoint(&v1)
            .expect("v1")
            .milestone
            .as_deref(),
        Some("First draft")
    );
    let state = family.host.state().clone();
    let ctx = family.host.context();
    let same = commands::mark_milestone(&ctx, &state, &v1, Some("First draft")).expect("ok");
    assert!(same.nothing_changed && same.records.is_empty());
    let long = "x".repeat(61);
    assert!(commands::mark_milestone(&ctx, &state, &v1, Some(&long)).is_err());
    let unknown = Id::parse(&"9".repeat(32)).expect("id");
    assert!(commands::mark_milestone(&ctx, &state, &unknown, Some("x")).is_err());
    family.host.milestone(&v1, None);
    assert_eq!(
        family.host.state().checkpoint(&v1).expect("v1").milestone,
        None
    );
    // A label the ledger refuses on reading, too.
    let bad = format!(
        r#"{{"v":1,"seq":{},"at":0,"event":{{"type":"MilestoneMarked","checkpoint_id":"{v1}","label":" padded "}}}}"#,
        family.host.state().next_seq()
    );
    let mut ledger = family.host.ledger_bytes();
    ledger.extend_from_slice(bad.as_bytes());
    let records = event::decode(&ledger).expect("decodes");
    assert!(crate::state::FamilyState::from_records(&records).is_err());
}

#[test]
fn a_file_is_relinked_only_to_a_file_that_is_its_own() {
    let mut family = everything();
    let state = family.host.state().clone();
    let ctx = family.host.context();
    let moved = commands::relink(
        &ctx,
        &state,
        &family.data,
        "Tailored versions/Data (moved).docx",
        &family.data_file,
    )
    .expect("relinks");
    assert_eq!(moved.records.len(), 1);
    let wrong = commands::relink(&ctx, &state, &family.data, "x.docx", &family.master_file);
    assert!(matches!(wrong, Err(Error::WrongFile(_))));
    let fresh = resume::package("t", &resume::master_v1_lines()).to_bytes();
    assert!(commands::relink(&ctx, &state, &family.data, "x.docx", &fresh).is_err());
    let here = state.node(&family.data).expect("data").storage_ref.clone();
    let same =
        commands::relink(&ctx, &state, &family.data, &here, &family.data_file).expect("same");
    assert!(same.nothing_changed);
    family.host.take(moved);
    assert_eq!(
        family
            .host
            .state()
            .node(&family.data)
            .expect("data")
            .storage_ref,
        "Tailored versions/Data (moved).docx"
    );
}
