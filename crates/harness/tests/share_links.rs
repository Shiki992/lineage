// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Sending a copy as a link (decision 0032), pressed through the panel the way
//! a person would, in memory and on a real folder:
//!
//! 1. Start tracking a master resume, send a clean copy to a mentor, and share
//!    it as an edit link.
//! 2. What I've sent shows the link, and nothing changed yet.
//! 3. The mentor edits the shared copy in place. Lineage spots it by its hash,
//!    recognises it by its ids, and the person keeps two changes and skips one.
//! 4. The mentor edits it again: only the new change is asked about.
//! 5. Stop sharing: the link is kept as stopped, with nothing left to watch.

use std::path::PathBuf;

use lineage::Timestamp;
use lineage::event::{LinkAccess, MatchedBy};
use lineage_core::Package;
use lineage_core::merge::Decision;
use lineage_harness::{LocalFolderStorage, MemoryStorage, Panel, Storage};
use lineage_testkit::resume::{self, Line};
use lineage_testkit::returned::light_edits;

const MASTER: &str = "Resume.docx";
/// 2026-10-15T09:30:00Z.
const START: Timestamp = Timestamp(1_792_056_600_000);

/// The mentor edits the shared file where it is: its body becomes these
/// lines; everything else, the ids too, stays.
fn edit_shared<S: Storage>(storage: &mut S, file: &str, lines: &[Line]) {
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

fn journey<S: Storage>(mut storage: S) -> Panel<S> {
    storage
        .write_file(
            MASTER,
            &resume::package("Avery Quinn master resume", &resume::master_v1_lines()).to_bytes(),
        )
        .expect("puts the master");
    let mut panel = Panel::new(storage, START);

    // 1. Track, send, share as an edit link.
    panel.start_tracking(MASTER).expect("starts tracking");
    let copy = panel
        .send_a_copy(MASTER, "Mentor", Some("Feedback"), true)
        .expect("sends");
    let sent = panel.what_ive_sent(None).expect("sent").groups[0].rows[0]
        .id
        .clone();
    panel
        .share_a_link(&sent, LinkAccess::Edit, &copy)
        .expect("shares");

    // 2. What I've sent shows it; nothing changed yet.
    let row = panel.what_ive_sent(None).expect("sent").groups[0].rows[0].clone();
    let link = row.link.expect("a link");
    assert_eq!(link.line, "Shared as a link · Can edit");
    assert!(link.live);
    assert_eq!(row.shared.expect("watched").file_ref, copy);
    assert!(panel.shared_edits().expect("edits").is_empty());

    // 3. The mentor edits the shared copy; it comes back by its ids.
    let mut edits = light_edits(&resume::master_v1_lines());
    edit_shared(&mut panel.storage, &copy, &edits);
    assert_eq!(
        panel.shared_edits().expect("edits"),
        std::slice::from_ref(&sent)
    );
    let (found, view) = panel.a_file_came_back(&copy).expect("recognises");
    assert!(found.by_ids);
    assert_eq!(view.headline, "This came back from Mentor");
    let first = panel
        .record_returned(&copy, &found.candidates[0], MatchedBy::Ids)
        .expect("records");
    assert!(panel.shared_edits().expect("edits").is_empty());
    let review = panel.returned_review(MASTER, &first).expect("review");
    let ids: Vec<String> = review.view.changes.iter().map(|c| c.id.clone()).collect();
    assert_eq!(ids.len(), 3);
    panel
        .keep_and_skip(
            MASTER,
            &first,
            &[
                (ids[0].as_str(), Decision::BringIn),
                (ids[1].as_str(), Decision::BringIn),
                (ids[2].as_str(), Decision::Decline),
            ],
        )
        .expect("keeps two, skips one");

    // 4. Edited again: only the new line is asked about.
    edits.push(Line::Plain("Speaks Portuguese and Spanish.".to_owned()));
    edit_shared(&mut panel.storage, &copy, &edits);
    assert_eq!(
        panel.shared_edits().expect("edits"),
        std::slice::from_ref(&sent)
    );
    let (found, _) = panel.a_file_came_back(&copy).expect("recognises");
    let second = panel
        .record_returned(&copy, &found.candidates[0], MatchedBy::Ids)
        .expect("records");
    let review = panel.returned_review(MASTER, &second).expect("review");
    assert_eq!(review.headline, "1 change from Mentor");
    assert!(review.view.changes.iter().all(|c| !ids.contains(&c.id)));

    // 5. Stop sharing.
    panel.stop_sharing(&sent).expect("stops");
    let row = panel.what_ive_sent(None).expect("sent").groups[0].rows[0].clone();
    assert!(!row.link.expect("listed").live);
    assert!(row.shared.is_none());
    assert!(panel.stop_sharing(&sent).is_err(), "nothing to stop");
    panel
}

#[test]
fn a_shared_copy_comes_back_twice_in_memory() {
    journey(MemoryStorage::default());
}

#[test]
fn a_shared_copy_comes_back_twice_on_disk() {
    let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("share-links");
    let _ = std::fs::remove_dir_all(&root);
    let panel = journey(LocalFolderStorage::new(&root).expect("folder"));
    // The history on disk reads back whole, links and all.
    let state = panel.state().expect("reads back");
    assert!(state.sent[0].link.as_ref().is_some_and(|l| !l.live()));
    assert_eq!(state.returned.len(), 2);
}
