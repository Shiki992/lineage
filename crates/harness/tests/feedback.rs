// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The Phase 3 feedback journey (BLUEPRINT §3, "Asking for feedback"), pressed
//! through the panel the way a person would, in memory and on a real folder:
//!
//! 1. Start tracking a master resume and send a clean copy to a mentor.
//! 2. The master moves on while the copy is away.
//! 3. The mentor edits the copy, renames it and sends it back.
//! 4. "A file came back": it is recognised by its ids, and recorded.
//! 5. What I've sent says "Came back with 4 changes"; the master panel lists it.
//! 6. Review one change at a time: keep two, skip one, and pick their side of
//!    a paragraph both changed. Kept changes land as tracked changes by Lineage.
//! 7. A checkpoint is saved, and What I've sent says "Reviewed".
//!
//! Then the other ways a file comes back: ids stripped and exported from
//! Google Docs (found by content), two copies of one checkpoint (a choice),
//! and a stranger's file (not recognised).

use std::path::PathBuf;

use lineage::Timestamp;
use lineage::event::MatchedBy;
use lineage::returned::Verdict;
use lineage_core::merge::Decision;
use lineage_core::{Docx, Package, View};
use lineage_harness::{LocalFolderStorage, MemoryStorage, Panel, Storage};
use lineage_testkit::resume::{self, Line};
use lineage_testkit::returned::{self as kit, Saved, light_edits};

const MASTER: &str = "Resume.docx";
const RETURNED: &str = "Resume - comments from Sam.docx";
/// 2026-10-15T09:30:00Z.
const START: Timestamp = Timestamp(1_792_056_600_000);

const OURS_SUMMARY: &str = "Data engineer with seven years of experience building reliable pipelines for retail and logistics teams.";
const THEIR_SUMMARY: &str = "Data engineer with six years of experience building dependable pipelines for retail, logistics and insurance teams.";

fn with_summary(mut lines: Vec<Line>, summary: &str) -> Vec<Line> {
    let at = lines
        .iter()
        .position(|l| l == &Line::H1("Summary".to_owned()))
        .expect("summary heading")
        + 1;
    lines[at] = Line::Plain(summary.to_owned());
    lines
}

/// Someone edits a file in Word: its body becomes these lines; everything else
/// (the ids too) stays. Written under `to`.
fn edit_in_word<S: Storage>(storage: &mut S, from: &str, to: &str, lines: &[Line]) {
    let source = Package::read(&resume::package("t", lines).to_bytes()).expect("reads");
    let mut package = Package::read(&storage.read_file(from).expect("file")).expect("reads");
    package.set_part(
        "word/document.xml",
        source.part("word/document.xml").expect("doc").to_vec(),
    );
    storage
        .write_file(to, &package.to_bytes().expect("bytes"))
        .expect("writes");
}

fn lines_of(bytes: &[u8], view: View) -> Vec<String> {
    Docx::open_view(bytes, view).expect("opens").body_lines()
}

fn expected(lines: &[Line]) -> Vec<String> {
    lines_of(&resume::package("t", lines).to_bytes(), View::Accepted)
}

/// Every tracked change in the main document is by Lineage.
fn authors(bytes: &[u8]) -> Vec<String> {
    let package = Package::read(bytes).expect("reads");
    let xml =
        String::from_utf8(package.part("word/document.xml").expect("doc").to_vec()).expect("utf-8");
    let mut out: Vec<String> = xml
        .split("w:author=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
        .map(str::to_owned)
        .collect();
    out.sort();
    out.dedup();
    out
}

fn journey<S: Storage>(mut storage: S) -> Panel<S> {
    storage
        .write_file(
            MASTER,
            &resume::package("Avery Quinn master resume", &resume::master_v1_lines()).to_bytes(),
        )
        .expect("puts the master");
    let mut panel = Panel::new(storage, START);

    // 1. Start tracking, and send a clean copy to a mentor.
    panel.start_tracking(MASTER).expect("starts tracking");
    let copy = panel
        .send_a_copy(MASTER, "Mentor", Some("Feedback"), true)
        .expect("sends");
    assert_eq!(copy, "Resume (Mentor).docx");

    // 2. The master moves on: a new job, and the summary reworded.
    let ours = with_summary(resume::master_v2_lines(), OURS_SUMMARY);
    edit_in_word(&mut panel.storage, MASTER, MASTER, &ours);
    panel.save_checkpoint(MASTER, "New job").expect("saves v2");

    // 3. The mentor edits their copy, renames it and sends it back.
    let theirs = with_summary(light_edits(&resume::master_v1_lines()), THEIR_SUMMARY);
    edit_in_word(&mut panel.storage, &copy, RETURNED, &theirs);

    // 4. "A file came back": recognised by its ids.
    let (found, view) = panel.a_file_came_back(RETURNED).expect("recognises");
    assert_eq!(found.verdict, Verdict::Certain);
    assert!(found.by_ids);
    assert_eq!(view.headline, "This came back from Mentor");
    assert_eq!(view.candidates[0].detail, "Resume · v1 · sent 15 Oct");
    assert_eq!(view.candidates[0].score, "Recognised by its Lineage ID");
    let returned = panel
        .record_returned(RETURNED, &found.candidates[0], MatchedBy::Ids)
        .expect("records");

    // 5. What I've sent, and the master panel.
    let sent = panel.what_ive_sent(None).expect("what I've sent");
    let row = &sent.groups[0].rows[0];
    assert_eq!(row.chip.as_deref(), Some("Came back with 4 changes"));
    assert_eq!(row.button, "Review");
    assert_eq!(row.returned.as_ref(), Some(&returned));
    let master = panel.master_panel().expect("panel");
    assert_eq!(master.returned.len(), 1);
    assert_eq!(
        master.returned[0].text,
        "Mentor sent back your copy from 15 Oct"
    );
    assert_eq!(master.returned[0].detail, "4 changes");

    // 6. Review: only the mentor's changes, against what they were sent.
    let review = panel.returned_review(MASTER, &returned).expect("review");
    assert_eq!(review.headline, "4 changes from Mentor");
    assert_eq!(
        review.from,
        "You sent this to Mentor on 15 Oct, from Resume v1."
    );
    assert_eq!(
        review.notes,
        ["Your file has changed since this copy left (it's now v2). Only their changes are shown."]
    );
    let cards = &review.view.changes;
    assert_eq!(cards.len(), 3, "the clean changes");
    assert_eq!(review.view.overlaps.len(), 1, "both changed the summary");
    let overlap = &review.view.overlaps[0];
    assert!(
        overlap
            .summary
            .starts_with("You and Mentor both changed 'Data engineer with six years"),
        "{}",
        overlap.summary
    );
    let pick = |prefix: &str| {
        cards
            .iter()
            .find(|c| {
                c.text
                    .iter()
                    .map(|s| s.text.as_str())
                    .collect::<String>()
                    .contains(prefix)
            })
            .unwrap_or_else(|| panic!("a card with {prefix}"))
            .id
            .clone()
    };
    let designed = pick("Designed");
    let mentored = pick("Mentored two new");
    let honours = pick("honours");
    let before = panel.storage.read_file(MASTER).expect("master");
    let outcome = panel
        .keep_and_skip(
            MASTER,
            &returned,
            &[
                (designed.as_str(), Decision::BringIn),
                (mentored.as_str(), Decision::BringIn),
                (honours.as_str(), Decision::Decline),
                (overlap.id.as_str(), Decision::UseMasters),
            ],
        )
        .expect("keeps and skips");
    assert!(outcome.file.is_some());

    // The kept changes are tracked changes by Lineage: accepting gives the
    // master with them, rejecting gives the master as it was.
    let after = panel.storage.read_file(MASTER).expect("master");
    assert_eq!(authors(&after), ["Lineage"]);
    let mut wanted = with_summary(resume::master_v2_lines(), THEIR_SUMMARY);
    let built = wanted
        .iter()
        .position(|l| {
            l == &Line::Bullet("Built the shipment data warehouse used by forty analysts.".into())
        })
        .expect("bullet");
    wanted[built] =
        Line::Bullet("Designed the shipment data warehouse used by forty analysts.".into());
    wanted.insert(
        built + 1,
        Line::Bullet("Mentored two new team members and wrote their onboarding guide.".into()),
    );
    assert_eq!(lines_of(&after, View::Accepted), expected(&wanted));
    assert_eq!(
        lines_of(&after, View::Rejected),
        lines_of(&before, View::Accepted)
    );

    // 7. A checkpoint, and the copy is reviewed.
    let master = panel.master_panel().expect("panel");
    assert_eq!(master.latest.label, "v3");
    assert_eq!(master.latest.note, "Kept 3 changes from Mentor");
    assert!(master.returned.is_empty());
    let row = &panel.what_ive_sent(None).expect("sent").groups[0].rows[0];
    assert_eq!(row.chip.as_deref(), Some("Reviewed"));
    assert_eq!(row.button, "Open");
    let review = panel.returned_review(MASTER, &returned).expect("review");
    assert_eq!(review.headline, "Nothing left to review");

    // What came back is kept exactly as it came.
    assert_eq!(
        panel.open_what_came_back(&returned).expect("opens"),
        panel.storage.read_file(RETURNED).expect("file")
    );
    // Dropping it in again finds the same record.
    let again = panel
        .record_returned(RETURNED, &found.candidates[0], MatchedBy::Ids)
        .expect("records");
    assert_eq!(again, returned);
    panel
}

#[test]
fn the_feedback_journey_in_memory() {
    journey(MemoryStorage::default());
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[test]
fn the_feedback_journey_on_disk() {
    let root = temp_dir("feedback");
    let panel = journey(LocalFolderStorage::new(&root).expect("folder"));
    let state = panel.state().expect("state");
    assert_eq!(state.returned.len(), 1);
    let file = &state.returned[0].file;
    let blob = root
        .join("Lineage")
        .join(state.family_id.as_str())
        .join("blobs")
        .join(file.as_str());
    assert!(
        blob.is_file(),
        "the returned file is kept in the family's blobs"
    );
}

fn tracked_master() -> Panel<MemoryStorage> {
    let mut storage = MemoryStorage::default();
    storage
        .write_file(
            MASTER,
            &resume::package("t", &resume::master_v1_lines()).to_bytes(),
        )
        .expect("puts");
    let mut panel = Panel::new(storage, START);
    panel.start_tracking(MASTER).expect("starts");
    panel
}

#[test]
fn a_google_docs_export_without_ids_is_found_by_its_content() {
    let mut panel = tracked_master();
    panel
        .send_a_copy(MASTER, "Northgate Analytics", None, false)
        .expect("sends");
    let export = kit::package(
        "t",
        &light_edits(&resume::master_v1_lines()),
        Saved::GoogleExport,
    );
    panel
        .storage
        .write_file("Avery Quinn resume.docx", &export.to_bytes())
        .expect("puts");
    let (found, view) = panel
        .a_file_came_back("Avery Quinn resume.docx")
        .expect("recognises");
    assert_eq!(found.verdict, Verdict::Certain);
    assert!(!found.by_ids);
    assert_eq!(view.headline, "This came back from Northgate Analytics");
    assert!(view.candidates[0].score.ends_with("% of the text matches"));
    let returned = panel
        .record_returned(
            "Avery Quinn resume.docx",
            &found.candidates[0],
            MatchedBy::Content,
        )
        .expect("records");
    let review = panel.returned_review(MASTER, &returned).expect("review");
    assert_eq!(review.view.changes.len(), 3, "the words they changed");

    // Saved by another app: Keep brings their words in the master's formatting.
    assert!(review.formatting.words_only);
    let ids: Vec<&str> = review.view.changes.iter().map(|c| c.id.as_str()).collect();
    let keep: Vec<(&str, Decision)> = ids.iter().map(|id| (*id, Decision::BringIn)).collect();
    panel
        .keep_and_skip(MASTER, &returned, &keep)
        .expect("keeps");
    let written = panel.storage.read_file(MASTER).expect("master");
    let docx = Docx::open(&written).expect("opens");
    let xml = docx.part_text(&docx.main_part).expect("text");
    assert!(!xml.contains("Arial") && !xml.contains("rPrChange") && !xml.contains("pPrChange"));
    assert_eq!(
        Docx::open_view(&written, View::Accepted)
            .expect("opens")
            .body_lines(),
        Docx::open(&resume::package("t", &light_edits(&resume::master_v1_lines())).to_bytes())
            .expect("opens")
            .body_lines()
    );
}

#[test]
fn two_copies_of_one_checkpoint_without_ids_are_a_choice() {
    let mut panel = tracked_master();
    panel
        .send_a_copy(MASTER, "Northgate Analytics", None, false)
        .expect("sends");
    panel
        .send_a_copy(MASTER, "Harbour Bank", None, false)
        .expect("sends");
    let back = resume::package("t", &light_edits(&resume::master_v1_lines()));
    panel
        .storage
        .write_file("back.docx", &back.to_bytes())
        .expect("puts");
    let (found, view) = panel.a_file_came_back("back.docx").expect("recognises");
    assert_eq!(found.verdict, Verdict::Choose);
    assert_eq!(view.headline, "Which copy is this?");
    let mut titles: Vec<&str> = view.candidates.iter().map(|c| c.title.as_str()).collect();
    titles.sort_unstable();
    assert_eq!(titles, ["Harbour Bank", "Northgate Analytics"]);
    // The person picks Harbour Bank.
    let pick = view
        .candidates
        .iter()
        .position(|c| c.title == "Harbour Bank")
        .expect("listed");
    let returned = panel
        .record_returned("back.docx", &found.candidates[pick], MatchedBy::Person)
        .expect("records");
    let state = panel.state().expect("state");
    assert_eq!(
        state.returned_file(&returned).expect("recorded").matched_by,
        MatchedBy::Person
    );
}

#[test]
fn a_strangers_file_is_not_recognised() {
    let mut panel = tracked_master();
    panel
        .send_a_copy(MASTER, "Northgate Analytics", None, false)
        .expect("sends");
    panel
        .storage
        .write_file(
            "Jordan Ellis.docx",
            &resume::package("t", &kit::stranger_lines()).to_bytes(),
        )
        .expect("puts");
    let (found, view) = panel.a_file_came_back("Jordan Ellis.docx").expect("reads");
    assert_eq!(found.verdict, Verdict::Unknown);
    assert_eq!(view.headline, "Lineage doesn't recognise this file");
}
