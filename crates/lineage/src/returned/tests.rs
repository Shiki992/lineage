// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Files that come back, through the commands, with an in-memory host.

use lineage_core::clean::clean_copy;
use lineage_core::docx::props::read_ids;
use lineage_core::merge::Decision;
use lineage_core::{Docx, Package, View};
use lineage_testkit::resume::{self, Line};
use lineage_testkit::returned::{light_edits, stranger_lines};

use super::*;
use crate::commands;
use crate::event::{self, Event};
use crate::tests::{Host, edit, lines};
use crate::views;

impl Host {
    fn send(&mut self, node: &Id, bytes: &[u8], recipient: &str, embed_id: bool) -> (Id, Vec<u8>) {
        let state = self.state().clone();
        let mut ctx = Context {
            now: self.now,
            ids: &mut self.ids,
            author: None,
        };
        let outcome = commands::send_copy(
            &mut ctx,
            &state,
            node,
            bytes,
            recipient,
            None,
            embed_id,
            &self.blobs,
        )
        .expect("sends");
        let outcome = self.take(outcome);
        (
            outcome.created.expect("sent copy"),
            outcome.new_file.expect("copy").bytes,
        )
    }

    fn recognise(&self, bytes: &[u8]) -> Recognition {
        let family = Family {
            state: self.state(),
            blobs: &self.blobs,
        };
        recognise(bytes, &[family]).expect("recognises")
    }

    fn record(&mut self, bytes: &[u8], found: &Recognition, how: MatchedBy) -> Id {
        let candidate = found.candidates[0].clone();
        let state = self.state().clone();
        let mut ctx = Context {
            now: self.now,
            ids: &mut self.ids,
            author: None,
        };
        let outcome = record(
            &mut ctx,
            &state,
            bytes,
            "Resume - comments AQ.docx",
            &candidate.checkpoint,
            candidate.sent.as_ref(),
            how,
            &self.blobs,
        )
        .expect("records");
        self.take(outcome).created.expect("returned file")
    }

    fn keep(
        &mut self,
        returned: &Id,
        node: &Id,
        bytes: &[u8],
        cards: &[(String, Decision)],
    ) -> Outcome {
        let state = self.state().clone();
        let mut ctx = Context {
            now: self.now,
            ids: &mut self.ids,
            author: None,
        };
        let outcome =
            decide(&mut ctx, &state, returned, node, bytes, cards, &self.blobs).expect("decides");
        self.take(outcome)
    }
}

/// A mentor's edits to the copy they were sent: the body replaced, everything
/// else (including the ids) as it was.
fn mentor(copy: &[u8], lines: &[Line]) -> Vec<u8> {
    edit(copy, lines)
}

/// The same file with its custom properties (and so the ids) gone.
fn stripped(file: &[u8]) -> Vec<u8> {
    let (bytes, _) = clean_copy(file, None).expect("cleans");
    assert!(
        read_ids(&Package::read(&bytes).expect("reads"))
            .expect("ids")
            .is_none()
    );
    bytes
}

fn tracked(bytes: &[u8]) -> Host {
    let mut host = Host::new();
    host.start(bytes);
    host
}

#[test]
fn a_sent_copy_carries_its_own_id_and_is_never_its_master() {
    let mut host = Host::new();
    let file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let (sent, copy) = host.send(&master, &file, "Mentor", true);
    let ids = read_ids(&Package::read(&copy).expect("reads"))
        .expect("ids")
        .expect("has ids");
    assert_eq!(ids.family, host.state().family_id.as_str());
    assert_eq!(ids.checkpoint, sent.as_str(), "the sent copy's own id");
    assert!(host.state().checkpoint(&sent).is_none());

    // Opened in Word and saved as a checkpoint of the master: refused.
    let state = host.state().clone();
    let mut ctx = Context {
        now: host.now,
        ids: &mut host.ids,
        author: None,
    };
    let refused = commands::save_checkpoint(&mut ctx, &state, &master, &copy, "", &host.blobs);
    assert!(
        matches!(&refused, Err(Error::WrongFile(d)) if d.contains("sent")),
        "{refused:?}"
    );
}

#[test]
fn a_copy_that_comes_back_is_recognised_by_its_ids_and_reviewed() {
    let mut host = Host::new();
    let v1 = resume::package("t", &resume::master_v1_lines()).to_bytes();
    let file = host.start(&v1);
    let master = host.master();
    let (sent, copy) = host.send(&master, &file, "Mentor", true);
    // The master moves on while the copy is away.
    let file = host.save(&master, &edit(&file, &resume::master_v2_lines()), "New job");
    let back = mentor(&copy, &light_edits(&resume::master_v1_lines()));

    let found = host.recognise(&back);
    assert_eq!(found.verdict, Verdict::Certain);
    assert!(found.by_ids);
    assert_eq!(found.candidates[0].sent.as_ref(), Some(&sent));
    let view = describe(&found, &[host.state()]);
    assert_eq!(view.headline, "This came back from Mentor");
    assert_eq!(view.candidates[0].score, "Recognised by its Lineage ID");
    assert_eq!(view.candidates[0].detail, "Resume · v1 · sent 15 Oct");

    let returned = host.record(&back, &found, MatchedBy::Ids);
    let r = host
        .state()
        .returned_file(&returned)
        .expect("recorded")
        .clone();
    assert_eq!((r.change_count, r.formatting_count), (3, 0));
    assert_eq!(r.confidence, 100);
    let row = &views::sent_list(host.state(), None).groups[0].rows[0];
    assert_eq!(row.chip.as_deref(), Some("Came back with 3 changes"));
    assert_eq!(row.button, "Review");
    assert_eq!(row.returned.as_ref(), Some(&returned));
    let panel = views::master_panel(host.state(), &host.blobs).expect("panel");
    assert_eq!(panel.returned.len(), 1);
    assert_eq!(
        panel.returned[0].text,
        "Mentor sent back your copy from 15 Oct"
    );
    assert_eq!(panel.returned[0].detail, "3 changes");

    // Dropping the same file again changes nothing.
    let again = host.record(&back, &found, MatchedBy::Ids);
    assert_eq!(again, returned);
    assert_eq!(host.state().returned.len(), 1);

    // Only the mentor's changes, judged against the master as it is now.
    let review = review(host.state(), &returned, &master, &file, &host.blobs).expect("reviews");
    assert_eq!(review.headline, "3 changes from Mentor");
    assert_eq!(
        review.from,
        "You sent this to Mentor on 15 Oct, from Resume v1."
    );
    assert_eq!(
        review.notes,
        ["Your file has changed since this copy left (it's now v2). Only their changes are shown."]
    );
    assert_eq!(review.theirs, "Mentor");
    assert!(review.view.overlaps.is_empty());
    let ids: Vec<String> = review.view.changes.iter().map(|c| c.id.clone()).collect();
    assert_eq!(ids.len(), 3);
    assert_eq!(review.view.changes[0].choices[0].label, "Keep");

    // Keep two, skip one, in one sitting.
    let cards = vec![
        (ids[0].clone(), Decision::BringIn),
        (ids[1].clone(), Decision::BringIn),
        (ids[2].clone(), Decision::Decline),
    ];
    let outcome = host.keep(&returned, &master, &file, &cards);
    let written = outcome.file.expect("written");
    let saved = host.state().latest(&master).expect("checkpoint").clone();
    assert_eq!(saved.note, "Kept 2 changes from Mentor");
    assert_eq!(host.state().label(&saved.id), "v3");
    // Accepting gives the master plus the two kept changes; rejecting gives
    // the master as it was.
    let accepted = Docx::open_view(&written, View::Accepted)
        .expect("opens")
        .body_lines();
    let rejected = Docx::open_view(&written, View::Rejected)
        .expect("opens")
        .body_lines();
    assert_eq!(rejected, lines(&file));
    assert!(
        accepted
            .iter()
            .any(|l| l.starts_with("Designed the shipment"))
    );
    assert!(accepted.iter().any(|l| l.starts_with("Mentored two new")));
    assert!(!accepted.iter().any(|l| l.ends_with("with honours.")));
    assert!(
        accepted
            .iter()
            .any(|l| l.starts_with("Senior data engineer, Halden"))
    );

    let r = host.state().returned_file(&returned).expect("recorded");
    assert!(r.finished());
    let row = &views::sent_list(host.state(), None).groups[0].rows[0];
    assert_eq!(row.chip.as_deref(), Some("Reviewed"));
    assert_eq!(row.button, "Open");
    assert!(
        views::master_panel(host.state(), &host.blobs)
            .expect("panel")
            .returned
            .is_empty()
    );

    // The kept changes are master changes like any other: offerable as v3.
    assert!(saved.changes.is_some());
    // And the file that came back can be opened exactly as it came.
    assert_eq!(
        open(host.state(), &returned, &host.blobs).expect("opens"),
        back
    );
}

#[test]
fn a_review_can_be_finished_later() {
    let mut host = Host::new();
    let file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let (_, copy) = host.send(&master, &file, "Mentor", true);
    let back = mentor(&copy, &light_edits(&resume::master_v1_lines()));
    let found = host.recognise(&back);
    let returned = host.record(&back, &found, MatchedBy::Ids);
    let first = review(host.state(), &returned, &master, &file, &host.blobs).expect("reviews");
    let id = first.view.changes[0].id.clone();
    let outcome = host.keep(
        &returned,
        &master,
        &file,
        &[(id.clone(), Decision::Decline)],
    );
    assert!(outcome.file.is_none(), "skipping writes nothing");
    assert!(outcome.records.iter().any(|r| matches!(
        &r.event,
        Event::ReturnReviewed { finished: false, skipped, .. } if skipped == &vec![id.clone()]
    )));
    let row = &views::sent_list(host.state(), None).groups[0].rows[0];
    assert_eq!(row.chip.as_deref(), Some("Partly reviewed"));
    let panel = views::master_panel(host.state(), &host.blobs).expect("panel");
    assert_eq!(panel.returned[0].detail, "3 changes · partly reviewed");

    let second = review(host.state(), &returned, &master, &file, &host.blobs).expect("reviews");
    assert_eq!(second.headline, "2 changes from Mentor");
    assert!(!second.view.ids().contains(&id.as_str()));

    // A decided card can't be decided again, and a decision must be a choice.
    let state = host.state().clone();
    let mut ctx = Context {
        now: host.now,
        ids: &mut host.ids,
        author: None,
    };
    let again = decide(
        &mut ctx,
        &state,
        &returned,
        &master,
        &file,
        &[(id, Decision::BringIn)],
        &host.blobs,
    );
    assert!(matches!(again, Err(Error::NotAllowed(_))));
    let other = second.view.changes[0].id.clone();
    let wrong = decide(
        &mut ctx,
        &state,
        &returned,
        &master,
        &file,
        &[(other, Decision::UseMasters)],
        &host.blobs,
    );
    assert!(matches!(wrong, Err(Error::NotAllowed(_))));
}

#[test]
fn when_both_changed_a_paragraph_the_person_picks_a_side() {
    let mut host = Host::new();
    let file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let (_, copy) = host.send(&master, &file, "Northgate Analytics", true);
    // The master renames the Corvia job (v3's edit) while the copy is away...
    let mut ours = resume::master_v1_lines();
    let corvia = ours
        .iter()
        .position(|l| l == &Line::H2(resume::CORVIA_TITLE.to_owned()))
        .expect("corvia");
    ours[corvia] = Line::H2(resume::CORVIA_TITLE_V3.to_owned());
    let file = host.save(&master, &edit(&file, &ours), "Promoted");
    // ...and they edit the same title.
    let back = mentor(&copy, &resume::tailored_same_para_lines());
    let found = host.recognise(&back);
    let returned = host.record(&back, &found, MatchedBy::Ids);
    let view = review(host.state(), &returned, &master, &file, &host.blobs).expect("reviews");
    assert_eq!(view.view.overlaps.len(), 1);
    let overlap = &view.view.overlaps[0];
    assert!(
        overlap
            .summary
            .starts_with("You and Northgate Analytics both changed 'Data engineer, Corvia"),
        "{}",
        overlap.summary
    );
    let labels: Vec<&str> = overlap.choices.iter().map(|c| c.label.as_str()).collect();
    assert_eq!(labels, ["Keep mine", "Use theirs"]);
    let outcome = host.keep(
        &returned,
        &master,
        &file,
        &[(overlap.id.clone(), Decision::UseMasters)],
    );
    let written = outcome.file.expect("written");
    let accepted = Docx::open_view(&written, View::Accepted)
        .expect("opens")
        .body_lines();
    assert!(
        accepted.contains(&"Data engineer (logistics), Corvia Freight, 2020 to 2023".to_owned())
    );
    assert_eq!(
        Docx::open_view(&written, View::Rejected)
            .expect("opens")
            .body_lines(),
        lines(&file)
    );
}

#[test]
fn without_ids_one_copy_is_found_by_its_content() {
    let mut host = Host::new();
    let file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let (sent, copy) = host.send(&master, &file, "Northgate Analytics", false);
    host.save(&master, &edit(&file, &resume::master_v2_lines()), "New job");
    let back = stripped(&mentor(&copy, &light_edits(&resume::master_v1_lines())));
    let found = host.recognise(&back);
    assert_eq!(found.verdict, Verdict::Certain, "{found:?}");
    assert!(!found.by_ids);
    assert_eq!(found.candidates[0].sent.as_ref(), Some(&sent));
    assert!(found.candidates[0].score >= CERTAIN);
    let view = describe(&found, &[host.state()]);
    assert_eq!(view.headline, "This came back from Northgate Analytics");
    assert!(view.candidates[0].score.ends_with("% of the text matches"));
    let returned = host.record(&back, &found, MatchedBy::Content);
    let r = host.state().returned_file(&returned).expect("recorded");
    assert_eq!(r.matched_by, MatchedBy::Content);
    assert_eq!(r.confidence, found.candidates[0].score);

    // MatchedBy::Ids is refused for a file without ids.
    let state = host.state().clone();
    let mut ctx = Context {
        now: host.now,
        ids: &mut host.ids,
        author: None,
    };
    let refused = record(
        &mut ctx,
        &state,
        &back,
        "x.docx",
        &found.candidates[0].checkpoint,
        Some(&sent),
        MatchedBy::Ids,
        &host.blobs,
    );
    assert!(matches!(refused, Err(Error::NotAllowed(_))));
}

#[test]
fn two_copies_of_one_checkpoint_are_a_choice() {
    let mut host = Host::new();
    let file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let (a, copy) = host.send(&master, &file, "Northgate Analytics", false);
    let (b, _) = host.send(&master, &file, "Pellow Ventures", false);
    let back = stripped(&mentor(&copy, &light_edits(&resume::master_v1_lines())));
    let found = host.recognise(&back);
    assert_eq!(found.verdict, Verdict::Choose);
    let sent: Vec<Option<&Id>> = found.candidates.iter().map(|c| c.sent.as_ref()).collect();
    assert!(
        sent.contains(&Some(&a)) && sent.contains(&Some(&b)),
        "{found:?}"
    );
    let view = describe(&found, &[host.state()]);
    assert_eq!(view.headline, "Which copy is this?");
    let titles: Vec<&str> = view.candidates.iter().map(|c| c.title.as_str()).collect();
    assert!(titles.contains(&"Northgate Analytics") && titles.contains(&"Pellow Ventures"));
    let returned = host.record(&back, &found, MatchedBy::Person);
    assert_eq!(
        host.state()
            .returned_file(&returned)
            .expect("recorded")
            .matched_by,
        MatchedBy::Person
    );
}

#[test]
fn a_strangers_file_is_not_recognised() {
    let mut host = Host::new();
    let file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    host.send(&master, &file, "Northgate Analytics", false);
    let stranger = resume::package("t", &stranger_lines()).to_bytes();
    let found = host.recognise(&stranger);
    assert_eq!(found.verdict, Verdict::Unknown, "{found:?}");
    let view = describe(&found, &[host.state()]);
    assert_eq!(view.headline, "Lineage doesn't recognise this file");
    assert!(view.candidates.is_empty());
}

#[test]
fn the_master_itself_sent_by_hand_is_a_copy_of_its_checkpoint() {
    // Someone attached their master file to an email instead of using "Send a
    // copy". It carries the checkpoint's id.
    let mut host = Host::new();
    let file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let v1 = host.state().latest(&master).expect("v1").id.clone();
    let file2 = host.save(&master, &edit(&file, &resume::master_v2_lines()), "New job");
    assert_ne!(file, file2);
    let back = edit(&file, &light_edits(&resume::master_v1_lines()));
    let found = host.recognise(&back);
    assert_eq!(found.verdict, Verdict::Certain);
    assert!(found.by_ids);
    assert_eq!(found.candidates[0].checkpoint, v1);
    assert_eq!(found.candidates[0].sent, None);
    let view = describe(&found, &[host.state()]);
    assert_eq!(view.headline, "This is a copy of Resume · v1");
    let returned = host.record(&back, &found, MatchedBy::Ids);
    let review = review(host.state(), &returned, &master, &file2, &host.blobs).expect("reviews");
    assert_eq!(review.headline, "3 changes from the returned copy");
    assert_eq!(review.from, "A copy of Resume v1.");
    assert_eq!(review.theirs, "The returned copy");
    let panel = views::master_panel(host.state(), &host.blobs).expect("panel");
    assert_eq!(panel.returned[0].text, "A copy of v1 came back");
}

#[test]
fn a_copy_of_a_version_can_go_into_the_master_instead() {
    let mut host = Host::new();
    let file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let (version, version_file) = host.make_version("Resume – data roles.docx");
    let version_file = host.save(
        &version,
        &edit(&version_file, &resume::tailored_data_lines()),
        "Tailored",
    );
    let (_, copy) = host.send(&version, &version_file, "Mentor", true);
    let back = mentor(&copy, &light_edits(&resume::tailored_data_lines()));
    let found = host.recognise(&back);
    let returned = host.record(&back, &found, MatchedBy::Ids);
    let into_master =
        review(host.state(), &returned, &master, &file, &host.blobs).expect("reviews");
    assert_eq!(
        into_master.notes,
        [
            "This copy was made from 'Resume – data roles'. The changes you keep will go into 'Resume'."
        ]
    );
    // It shows on the version's panel, where it came from.
    let panel = views::version_panel(host.state(), &version, &host.blobs).expect("panel");
    assert_eq!(panel.returned.len(), 1);
    assert!(
        views::master_panel(host.state(), &host.blobs)
            .expect("panel")
            .returned
            .is_empty()
    );
}

#[test]
fn a_returned_record_must_match_its_copy() {
    let host = tracked(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let v1 = host.state().latest(&master).expect("v1").id.clone();
    let mut state = host.state().clone();
    let bad = event::Record {
        v: event::RECORD_VERSION,
        seq: state.next_seq(),
        at: host.now,
        event: Event::FileReturned {
            returned_id: Id::parse(&"e".repeat(32)).expect("id"),
            sent_copy_id: Some(Id::parse(&"d".repeat(32)).expect("id")),
            from_checkpoint_id: v1,
            display_name: "x.docx".into(),
            file: BlobHash::of(b"x"),
            matched_by: MatchedBy::Content,
            confidence: 90,
            change_count: 1,
            formatting_count: 0,
        },
    };
    assert!(
        state.apply(&bad).is_err(),
        "an unknown sent copy is refused"
    );
}
