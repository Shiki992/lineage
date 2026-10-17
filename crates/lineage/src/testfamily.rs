// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! One family with a bit of everything, for the library, index, tree and
//! document page tests: a master with two tailored versions, master changes
//! brought into one and declined in the other, two copies sent, one of which
//! came back and was partly kept, an archived version and a milestone.

use lineage_core::merge::Decision;
use lineage_testkit::resume;
use lineage_testkit::returned::light_edits;

use crate::commands::{self, Outcome};
use crate::event::{Event, MatchedBy, RECORD_VERSION, Record};
use crate::ids::Id;
use crate::returned::review;
use crate::tests::{Host, edit};

pub(crate) struct Family {
    pub(crate) host: Host,
    pub(crate) master: Id,
    pub(crate) data: Id,
    pub(crate) platform: Id,
    pub(crate) archived: Id,
    pub(crate) master_file: Vec<u8>,
    pub(crate) data_file: Vec<u8>,
    pub(crate) mentor: Id,
    pub(crate) northgate: Id,
    pub(crate) returned: Id,
}

impl Host {
    /// Appends a record no command makes yet (archiving a version).
    pub(crate) fn push(&mut self, event: Event) {
        let record = Record {
            v: RECORD_VERSION,
            seq: self.state().next_seq(),
            at: self.now,
            event,
        };
        self.take(Outcome {
            records: vec![record],
            ..Outcome::default()
        });
    }

    pub(crate) fn milestone(&mut self, checkpoint: &Id, label: Option<&str>) {
        let state = self.state().clone();
        let ctx = self.context();
        let outcome = commands::mark_milestone(&ctx, &state, checkpoint, label).expect("marks");
        self.take(outcome);
    }
}

pub(crate) fn everything() -> Family {
    let mut host = Host::new();
    let master_file = host.start(&resume::package("t", &resume::master_v1_lines()).to_bytes());
    let master = host.master();
    let (data, data_file) = host.make_version("Resume – data roles.docx");
    let (platform, platform_file) = host.make_version("Resume – platform roles.docx");
    let (archived, _) = host.make_version("Resume – old.docx");
    let master_file = host.save(
        &master,
        &edit(&master_file, &resume::master_v2_lines()),
        "New job",
    );
    host.offer(&[data.clone(), platform.clone()]);
    let bring: Vec<_> = host
        .offered(&data)
        .into_iter()
        .map(|(c, id)| (c, id, Decision::BringIn))
        .collect();
    let data_file = host.decide(&data, &data_file, &bring);
    let decline: Vec<_> = host
        .offered(&platform)
        .into_iter()
        .map(|(c, id)| (c, id, Decision::Decline))
        .collect();
    host.decide(&platform, &platform_file, &decline);
    host.push(Event::NodeArchived {
        node_id: archived.clone(),
    });
    let v2 = host.state().latest(&master).expect("v2").id.clone();
    host.milestone(&v2, Some("Sent to ten companies"));

    let (mentor, copy) = host.send(&master, &master_file, "Mentor", true);
    let (northgate, _) = host.send(&data, &data_file, "Northgate Analytics", true);
    let back = edit(&copy, &light_edits(&resume::master_v2_lines()));
    let found = host.recognise(&back);
    let returned = host.record(&back, &found, MatchedBy::Ids);
    let first =
        review(host.state(), &returned, &master, &master_file, &host.blobs).expect("review");
    let cards: Vec<_> = first
        .view
        .changes
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let d = if i == 0 {
                Decision::BringIn
            } else {
                Decision::Decline
            };
            (c.id.clone(), d)
        })
        .take(2)
        .collect();
    let kept = host.keep(&returned, &master, &master_file, &cards);
    let master_file = kept.file.unwrap_or(master_file);
    Family {
        host,
        master,
        data,
        platform,
        archived,
        master_file,
        data_file,
        mentor,
        northgate,
        returned,
    }
}
