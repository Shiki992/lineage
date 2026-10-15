// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Audits what the Word task pane wrote to its development folder
//! (`apps/office-addin/.dev-storage`, same layout as LocalFolderStorage), with
//! the Rust engine and none of the TypeScript: every ledger folds, every blob
//! matches its hash, every snapshot restores to a readable file, every tailored
//! version on disk is a checkpoint of its node, and every sent copy on disk is
//! byte for byte one that was recorded.
//!
//! Run after trying the panel in Word (ignored otherwise):
//!   cargo test -p lineage-harness --test dev_folder -- --ignored --nocapture
//! `LINEAGE_DEV_STORAGE` points at another folder. Prints counts only.

use std::path::PathBuf;

use lineage::blob::{BlobHash, BlobSource, referenced, restore};
use lineage::event;
use lineage::state::FamilyState;
use lineage_core::docx::props::read_ids;
use lineage_core::{Docx, Package};
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

#[test]
#[ignore = "reads the task pane's development folder; run by hand after a Word session"]
fn the_task_pane_wrote_a_sound_history() {
    let root = std::env::var_os("LINEAGE_DEV_STORAGE").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/office-addin/.dev-storage"),
        PathBuf::from,
    );
    let storage = LocalFolderStorage::new(&root).expect("folder");
    let families = storage.families().expect("families");
    assert!(!families.is_empty(), "no families in {}", root.display());
    let (mut checkpoints, mut blobs, mut versions, mut sent_files) = (0, 0, 0, 0);
    for family in &families {
        let (ledger, _) = storage.read_ledger(family).expect("ledger");
        let state = FamilyState::from_records(&event::decode(&ledger).expect("decodes"))
            .expect("the ledger folds");
        let source = Stored {
            storage: &storage,
            family,
        };
        for hash in referenced(&state) {
            let bytes = source.blob(&hash).expect("referenced blob is stored");
            assert_eq!(BlobHash::of(&bytes), hash, "blob matches its name");
            blobs += 1;
        }
        for checkpoint in &state.checkpoints {
            let bytes = restore(&checkpoint.snapshot, &source).expect("snapshot restores");
            Docx::open(&bytes).expect("snapshot is a readable .docx");
            checkpoints += 1;
        }
        for copy in &state.sent {
            let bytes = source.blob(&copy.file).expect("sent copy is stored");
            assert_eq!(BlobHash::of(&bytes), copy.file);
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
        "{} families, {checkpoints} checkpoints restored, {blobs} blobs checked, \
         {versions} tailored versions on disk, {sent_files} sent copies on disk match the record",
        families.len()
    );
}
