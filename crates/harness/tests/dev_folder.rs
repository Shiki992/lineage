// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Audits what the Word or PowerPoint task pane wrote to its development folder
//! (`apps/office-addin/.dev-storage`, same layout as LocalFolderStorage), with
//! the Rust engine and none of the TypeScript: every ledger folds, every blob
//! matches its hash, every snapshot restores to a readable file (a deck's parts
//! and relationships holding together), every tailored version on disk is a
//! checkpoint of its node, every sent copy on disk is byte for byte one that
//! was recorded (a deck's without hidden slides), every file that came back is
//! kept exactly, and every review's checkpoint is of the file it went into
//! (and, with `LINEAGE_SMOKE_DIR`, Word files are written out for
//! `word-smoke.ps1 -Compare`).
//!
//! Run after trying the panel in Word or PowerPoint (ignored otherwise):
//!   cargo test -p lineage-harness --test dev_folder -- --ignored --nocapture
//! `LINEAGE_DEV_STORAGE` points at another folder. Prints counts only.

use std::path::PathBuf;

use lineage::blob::{BlobHash, BlobSource, referenced, restore};
use lineage::event;
use lineage::state::FamilyState;
use lineage_core::doc::Doc;
use lineage_core::docx::props::read_ids;
use lineage_core::{Deck, Docx, Package};
use lineage_harness::{LocalFolderStorage, Storage};

struct Stored<'a> {
    storage: &'a LocalFolderStorage,
    family: &'a lineage::Id,
}

impl BlobSource for Stored<'_> {
    fn blob(&self, hash: &BlobHash) -> Option<Vec<u8>> {
        self.storage.get_blob(self.family, hash)
    }
}

/// A file of the family's format that opens, and for a deck, whose parts and
/// relationships hold together.
fn readable(bytes: &[u8], format: &str, what: &str) {
    Doc::open_as(bytes, format).unwrap_or_else(|e| panic!("{what} is a readable .{format}: {e}"));
    if format == "pptx" {
        lineage_core::pptx::check::structure(bytes)
            .unwrap_or_else(|e| panic!("{what} holds together: {e}"));
    }
}

/// With `LINEAGE_SMOKE_DIR` set, writes the file and the text Word should show
/// with every revision accepted and with every revision rejected, for
/// `scripts/word-smoke.ps1 -Compare` (as the engine's own tests do).
fn smoke(name: &str, bytes: &[u8]) {
    let Some(dir) = std::env::var_os("LINEAGE_SMOKE_DIR").map(PathBuf::from) else {
        return;
    };
    std::fs::create_dir_all(&dir).expect("smoke folder");
    std::fs::write(dir.join(format!("{name}.docx")), bytes).expect("writes");
    for (view, suffix) in [
        (lineage_core::View::Accepted, "accepted"),
        (lineage_core::View::Rejected, "rejected"),
    ] {
        let lines: Vec<String> = Docx::open_view(bytes, view)
            .expect("opens")
            .body_lines()
            .into_iter()
            .map(|line| line.replace('\n', "\u{b}"))
            .collect();
        std::fs::write(dir.join(format!("{name}.{suffix}.txt")), lines.join("\n")).expect("writes");
    }
}

#[test]
#[ignore = "reads the task pane's development folder; run by hand after a Word or PowerPoint session"]
fn the_task_pane_wrote_a_sound_history() {
    let root = std::env::var_os("LINEAGE_DEV_STORAGE").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/office-addin/.dev-storage"),
        PathBuf::from,
    );
    let storage = LocalFolderStorage::new(&root).expect("folder");
    let families = storage.families().expect("families");
    assert!(!families.is_empty(), "no families in {}", root.display());
    let (mut checkpoints, mut blobs, mut versions, mut sent_files) = (0, 0, 0, 0);
    let (mut returned, mut reviews, mut decks) = (0, 0, 0);
    for family in &families {
        let (ledger, _) = storage.read_ledger(family).expect("ledger");
        let state = FamilyState::from_records(&event::decode(&ledger).expect("decodes"))
            .expect("the ledger folds");
        if !lineage::library::readable_format(&state.format) {
            // A workbook: this version of Lineage only shows its history.
            println!("skipped a {} family", state.format);
            continue;
        }
        let source = Stored {
            storage: &storage,
            family,
        };
        let format = state.format.as_str();
        if format == "pptx" {
            decks += 1;
        }
        for hash in referenced(&state) {
            let bytes = source.blob(&hash).expect("referenced blob is stored");
            assert_eq!(BlobHash::of(&bytes), hash, "blob matches its name");
            blobs += 1;
        }
        for checkpoint in &state.checkpoints {
            let bytes = restore(&checkpoint.snapshot, &source).expect("snapshot restores");
            readable(&bytes, format, "a snapshot");
            checkpoints += 1;
        }
        for copy in &state.sent {
            let bytes = source.blob(&copy.file).expect("sent copy is stored");
            assert_eq!(BlobHash::of(&bytes), copy.file);
            readable(&bytes, format, "a sent copy");
            if format == "pptx" {
                let deck = Deck::open(&bytes).expect("opens");
                assert!(
                    deck.slides.iter().all(|s| !s.hidden),
                    "a sent deck has no hidden slides"
                );
            }
        }
        for back in &state.returned {
            let bytes = source.blob(&back.file).expect("a returned file is stored");
            assert_eq!(BlobHash::of(&bytes), back.file, "kept exactly as it came");
            readable(&bytes, format, "a returned file");
            if let Some(sent) = &back.sent {
                assert_eq!(
                    state.sent_copy(sent).expect("its sent copy").from,
                    back.from,
                    "it came back from the checkpoint its copy left from"
                );
            }
            returned += 1;
            for review in &back.reviews {
                if let Some(result) = &review.result_checkpoint {
                    let saved = state.checkpoint(result).expect("the review's checkpoint");
                    assert_eq!(saved.node, review.into);
                    let bytes = restore(&saved.snapshot, &source).expect("restores");
                    if format == "docx" {
                        smoke(&format!("kept-{reviews}"), &bytes);
                    }
                }
                reviews += 1;
            }
        }
        for node in state.variants() {
            let path = root.join(&node.storage_ref);
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let ids = read_ids(&Package::read(&bytes).expect("reads"))
                .expect("properties")
                .expect("a tailored version carries its ids");
            assert_eq!(ids.family, family.as_str());
            assert!(
                node.checkpoints
                    .iter()
                    .any(|c| c.as_str() == ids.checkpoint),
                "the version on disk is one of its node's checkpoints"
            );
            readable(&bytes, format, "a tailored version");
            if format == "docx" {
                smoke(&format!("version-{versions}"), &bytes);
            }
            versions += 1;
        }
        let sent_dir = root.join("Sent copies");
        if let Ok(entries) = std::fs::read_dir(&sent_dir) {
            for entry in entries {
                let bytes = std::fs::read(entry.expect("entry").path()).expect("reads");
                let hash = BlobHash::of(&bytes);
                if state.sent.iter().any(|s| s.file == hash) {
                    sent_files += 1;
                }
            }
        }
    }
    println!(
        "{} families ({decks} decks), {checkpoints} checkpoints restored, {blobs} blobs checked, \
         {versions} tailored versions on disk, {sent_files} sent copies on disk match the record, \
         {returned} files came back, {reviews} reviews",
        families.len()
    );
}
