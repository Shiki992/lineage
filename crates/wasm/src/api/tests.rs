// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The boundary, driven the way the task pane drives it: JSON in, envelopes and
//! bytes out, a ledger and blob store kept by the host.

use std::collections::BTreeMap;

use lineage_testkit::resume::{self, Line};
use serde_json::Value;

use super::*;

/// 2026-10-15T09:30:00Z.
const START: f64 = 1_792_056_600_000.0;

/// What the task pane keeps: the ledger, stored blobs and the files.
struct Host {
    session: Session,
    ledger: Vec<u8>,
    stored: BTreeMap<String, Vec<u8>>,
    now: f64,
}

fn value(reply: &Reply) -> Value {
    let v: Value = serde_json::from_str(&reply.json).expect("json");
    assert_eq!(v["ok"], Value::Bool(true), "{}", reply.json);
    v["value"].clone()
}

fn error(reply: &Reply) -> Value {
    let v: Value = serde_json::from_str(&reply.json).expect("json");
    assert_eq!(v["ok"], Value::Bool(false), "{}", reply.json);
    let e = v["error"].clone();
    assert!(e["userMessage"].as_str().is_some_and(|m| !m.is_empty()));
    e
}

fn str_of(v: &Value) -> String {
    v.as_str().expect("string").to_owned()
}

fn edit(file: &[u8], lines: &[Line]) -> Vec<u8> {
    let source = Package::read(&resume::package("t", lines).to_bytes()).expect("reads the fixture");
    let mut package = Package::read(file).expect("reads");
    package.set_part(
        "word/document.xml",
        source.part("word/document.xml").expect("doc").to_vec(),
    );
    package.to_bytes().expect("bytes")
}

fn lines(bytes: &[u8]) -> Vec<String> {
    lineage_core::Docx::open(bytes).expect("opens").body_lines()
}

impl Host {
    fn new(seed: u8) -> Self {
        Self {
            session: Session::new([seed; 32]),
            ledger: Vec::new(),
            stored: BTreeMap::new(),
            now: START,
        }
    }

    /// Stores what a command produced and appends its lines, as the shell does.
    fn take(&mut self, mut reply: Reply) -> (Value, Option<Vec<u8>>, Option<Vec<u8>>) {
        let v = value(&reply);
        for hash in v["blobs"].as_array().expect("blobs") {
            let hash = str_of(hash);
            let blob = reply
                .blobs
                .iter()
                .find(|b| b.hash.as_str() == hash)
                .expect("listed blob travels with the reply");
            self.stored.insert(hash, blob.bytes.clone());
        }
        self.ledger
            .extend_from_slice(str_of(&v["ledgerLines"]).as_bytes());
        self.now += 60_000.0;
        (v, reply.file.take(), reply.new_file.take())
    }

    /// Fetches whatever the engine says it is missing, then calls again.
    fn fetching(&mut self, mut call: impl FnMut(&Session) -> Reply) -> (Reply, usize) {
        let mut rounds = 0;
        loop {
            let reply = call(&self.session);
            let v: Value = serde_json::from_str(&reply.json).expect("json");
            if v["ok"] == Value::Bool(true) || v["error"]["kind"] != "missingBlobs" {
                return (reply, rounds);
            }
            rounds += 1;
            assert!(rounds < 10, "fetching never finishes");
            for hash in v["error"]["hashes"].as_array().expect("hashes") {
                let hash = str_of(hash);
                let bytes = self.stored[&hash].clone();
                value(&self.session.add_blob(&hash, bytes));
            }
        }
    }
}

struct Started {
    host: Host,
    master_file: Vec<u8>,
    master: String,
    data: (String, Vec<u8>),
    platform: (String, Vec<u8>),
}

fn started() -> Started {
    let mut host = Host::new(1);
    let file = resume::package("Avery Quinn master resume", &resume::master_v1_lines()).to_bytes();
    let reply = host
        .session
        .start_tracking(&file, "Resume.docx", "Resume.docx", host.now);
    let (v, master_file, _) = host.take(reply);
    let master_file = master_file.expect("file with ids");
    assert!(v["familyId"].is_string());
    let family = value(&host.session.family(&host.ledger));
    let master = str_of(&family["nodes"][0]["id"]);
    let make = |host: &mut Host, name: &str| {
        let reply = host
            .session
            .make_tailored_version(&host.ledger, name, name, host.now);
        let (v, _, new_file) = host.take(reply);
        assert_eq!(v["newFileName"], Value::String(name.to_owned()));
        (str_of(&v["created"]), new_file.expect("new file"))
    };
    let data = make(&mut host, "Resume – data roles.docx");
    let platform = make(&mut host, "Resume – platform roles.docx");
    Started {
        host,
        master_file,
        master,
        data,
        platform,
    }
}

#[test]
fn the_personal_scenario_crosses_the_boundary() {
    let Started {
        mut host,
        master_file,
        master,
        data,
        platform,
    } = started();

    let panel = value(&host.session.master_panel(&host.ledger));
    assert_eq!(panel["follows"], "2 tailored versions follow this file");
    assert_eq!(
        panel["versions"][0]["storageRef"],
        "Resume – data roles.docx"
    );
    assert!(panel["callout"].is_null());

    // Add a job to the master in Word, look at what changed, save.
    let edited = edit(&master_file, &resume::master_v2_lines());
    let changed = value(&host.session.what_changed(&host.ledger, &master, &edited));
    assert!(
        changed["headline"]
            .as_str()
            .expect("headline")
            .ends_with("since v1")
    );
    let reply = host
        .session
        .save_checkpoint(&host.ledger, &master, &edited, "New job", host.now);
    let (v, _, _) = host.take(reply);
    assert!(v["ids"]["checkpoint"].is_string());

    let panel = value(&host.session.master_panel(&host.ledger));
    let callout = &panel["callout"];
    assert_eq!(callout["button"], "Offer them to both versions");
    let versions = serde_json::to_string(&callout["versions"]).expect("ids");
    let reply = host
        .session
        .offer_to_versions(&host.ledger, &versions, host.now);
    host.take(reply);

    // Bring the job into one version.
    let updates = value(&host.session.updates(&host.ledger, &data.0, &data.1));
    assert_eq!(updates["headline"], "2 updates from the master");
    let cards: Vec<Value> = updates["groups"][0]["view"]["changes"]
        .as_array()
        .expect("changes")
        .iter()
        .map(|c| serde_json::json!({"card": c["id"], "decision": "bringIn"}))
        .collect();
    let reply = host.session.decide(
        &host.ledger,
        &data.0,
        &data.1,
        &serde_json::to_string(&cards).expect("json"),
        host.now,
    );
    let (v, data_after, _) = host.take(reply);
    let data_after = data_after.expect("written");
    assert_eq!(lines(&data_after), lines(&edited));
    assert!(v["ids"]["checkpoint"].is_string());
    let written = value(&host.session.check_written(&data_after, &data_after));
    assert_eq!(written["ok"], true);
    assert!(written["lineageExpected"].as_u64().expect("count") > 0);

    // Decline it in the other.
    let updates = value(&host.session.updates(&host.ledger, &platform.0, &platform.1));
    let cards: Vec<Value> = updates["groups"][0]["view"]["changes"]
        .as_array()
        .expect("changes")
        .iter()
        .map(|c| serde_json::json!({"card": c["id"], "decision": "decline"}))
        .collect();
    let reply = host.session.decide(
        &host.ledger,
        &platform.0,
        &platform.1,
        &serde_json::to_string(&cards).expect("json"),
        host.now,
    );
    let (v, file, _) = host.take(reply);
    assert!(file.is_none(), "declining writes nothing");
    assert_eq!(v["hasFile"], false);
    let panel = value(&host.session.version_panel(&host.ledger, &platform.0));
    assert!(panel["updates"].is_null());

    // Send a copy and open it again.
    let reply = host.session.send_copy(
        &host.ledger,
        &data.0,
        &data_after,
        r#"{"recipient":"Northgate Analytics","purpose":"Data engineer","embedId":true}"#,
        host.now,
    );
    let (v, _, copy) = host.take(reply);
    let copy = copy.expect("a copy");
    assert_eq!(
        v["newFileName"],
        "Resume – data roles (Northgate Analytics).docx"
    );
    let sent = value(&host.session.sent_list(&host.ledger, None));
    let row = &sent["groups"][0]["rows"][0];
    assert_eq!(row["recipient"], "Northgate Analytics");
    let mut reply = host
        .session
        .open_sent_copy(&host.ledger, &str_of(&row["id"]));
    value(&reply);
    assert_eq!(
        reply.file.take().expect("bytes"),
        copy,
        "exactly what was sent"
    );
    let ids = value(&host.session.read_ids(&copy));
    assert_eq!(
        ids["family"],
        value(&host.session.family(&host.ledger))["familyId"]
    );
    let empty = value(&host.session.sent_list(&host.ledger, Some("nobody")));
    assert_eq!(empty["groups"], serde_json::json!([]));
}

#[test]
fn a_fresh_engine_fetches_what_it_needs_a_snapshot_at_a_time() {
    let Started {
        mut host,
        master_file,
        master,
        data,
        ..
    } = started();
    let edited = edit(&master_file, &resume::master_v2_lines());
    let reply = host
        .session
        .save_checkpoint(&host.ledger, &master, &edited, "New job", host.now);
    host.take(reply);

    // Another device: same storage, empty cache.
    host.session = Session::new([2; 32]);
    let referenced = value(&host.session.referenced_blobs(&host.ledger));
    let referenced: Vec<String> = referenced
        .as_array()
        .expect("list")
        .iter()
        .map(str_of)
        .collect();
    for hash in &referenced {
        value(&host.session.add_blob(hash, host.stored[hash].clone()));
    }
    assert_eq!(
        value(&host.session.referenced_blobs(&host.ledger)),
        serde_json::json!([]),
        "nothing left to prefetch"
    );
    let ledger = host.ledger.clone();
    let (reply, rounds) = host.fetching(|s| s.master_panel(&ledger));
    value(&reply);
    assert_eq!(rounds, 0, "the master panel needs only change lists");

    let file = data.1.clone();
    let node = data.0.clone();
    let (reply, rounds) = host.fetching(|s| s.updates(&ledger, &node, &file));
    value(&reply);
    assert!(rounds <= 2, "two snapshots, {rounds} rounds");
}

#[test]
fn malformed_requests_are_refused() {
    let Started { mut host, data, .. } = started();
    for now in [f64::NAN, f64::INFINITY, 1.5, -1.0, 1e300] {
        let e = error(
            &host
                .session
                .save_checkpoint(&host.ledger, &data.0, &data.1, "", now),
        );
        assert_eq!(e["kind"], "badInput", "{now}");
    }
    let e = error(&host.session.updates(&host.ledger, "../etc", &data.1));
    assert_eq!(e["kind"], "badInput");
    let e = error(&host.session.decide(
        &host.ledger,
        &data.0,
        &data.1,
        r#"[{"card":"x","decision":"bringIn","extra":1}]"#,
        START,
    ));
    assert_eq!(e["kind"], "badInput");
    let e = error(&host.session.send_copy(
        &host.ledger,
        &data.0,
        &data.1,
        r#"{"recipient":"   ","purpose":null,"embedId":true}"#,
        START,
    ));
    assert_eq!(e["kind"], "badInput");
    let e = error(
        &host
            .session
            .make_tailored_version(&host.ledger, "", "x", START),
    );
    assert_eq!(e["kind"], "badInput");
    let e = error(&host.session.add_blob(&"a".repeat(64), b"not that".to_vec()));
    assert_eq!(e["kind"], "badInput");
    let e = error(
        &host
            .session
            .write_ids(&data.1, r#"{"family":"x","checkpoint":"y"}"#),
    );
    assert_eq!(e["kind"], "badInput");
    let e = error(&host.session.family(b"not json\n"));
    assert_eq!(e["kind"], "ledger");
    let e = error(&host.session.decide(
        &host.ledger,
        &data.0,
        &data.1,
        r#"[{"card":"not-a-card","decision":"bringIn"}]"#,
        START,
    ));
    assert_eq!(e["kind"], "notAllowed");
    host.now += 1.0;
}

#[test]
fn the_wrong_file_is_refused_and_tracking_twice_is_not_allowed() {
    let Started {
        host,
        master_file,
        data,
        platform,
        ..
    } = started();
    let e = error(
        &host
            .session
            .what_changed(&host.ledger, &data.0, &platform.1),
    );
    assert_eq!(e["kind"], "wrongFile");
    let mut session = Session::new([3; 32]);
    let e = error(&session.start_tracking(&master_file, "Resume.docx", "Resume.docx", START));
    assert_eq!(e["kind"], "notAllowed");
    let e = error(&session.start_tracking(b"not a docx", "Resume.docx", "Resume.docx", START));
    assert_eq!(e["kind"], "engine");
    assert_eq!(
        e["userMessage"],
        "This file isn't a Word document Lineage can open."
    );
}

#[test]
fn ids_are_read_and_written_and_a_file_is_found_by_them() {
    let Started {
        host,
        master_file,
        data,
        ..
    } = started();
    let session = &host.session;
    let untracked = resume::package("t", &resume::master_v1_lines()).to_bytes();
    assert!(value(&session.read_ids(&untracked)).is_null());
    let ids = value(&session.read_ids(&data.1));
    let found = value(&session.open_file(&host.ledger, &str_of(&ids["checkpoint"])));
    assert_eq!(found["nodeId"], Value::String(data.0.clone()));
    assert_eq!(found["role"], "variant");
    assert_eq!(found["isLatest"], true);
    let master_ids = value(&session.read_ids(&master_file));
    let mut reply = session.write_ids(
        &untracked,
        &serde_json::to_string(&master_ids).expect("json"),
    );
    value(&reply);
    let with_ids = reply.file.take().expect("bytes");
    assert_eq!(value(&session.read_ids(&with_ids)), master_ids);
    let e = error(&session.open_file(&host.ledger, &"f".repeat(32)));
    assert_eq!(e["kind"], "wrongFile");
}

#[test]
fn a_file_that_comes_back_crosses_the_boundary() {
    let Started {
        mut host,
        master_file,
        master,
        ..
    } = started();
    let reply = host.session.send_copy(
        &host.ledger,
        &master,
        &master_file,
        r#"{"recipient":"Mentor","purpose":null,"embedId":true}"#,
        host.now,
    );
    let (_, _, copy) = host.take(reply);
    let back = edit(
        &copy.expect("copy"),
        &lineage_testkit::returned::light_edits(&resume::master_v1_lines()),
    );
    let family = str_of(&value(&host.session.family(&host.ledger))["familyId"]);
    let ledgers = serde_json::json!([{
        "family": family,
        "ledger": String::from_utf8(host.ledger.clone()).expect("utf-8"),
    }])
    .to_string();

    // Without its ids, a fresh pane asks for blobs from the right family, then finds it.
    let stripped = lineage_core::clean::clean_copy(&back, None)
        .expect("strips")
        .0;
    let mut fresh = Host::new(9);
    fresh.ledger = host.ledger.clone();
    fresh.stored = host.stored.clone();
    let first = error(&fresh.session.recognise(&stripped, &ledgers));
    assert_eq!(first["kind"], "missingBlobs");
    assert_eq!(first["family"], Value::String(family.clone()));
    let (reply, rounds) = fresh.fetching(|s| s.recognise(&stripped, &ledgers));
    assert!(
        (1..=2).contains(&rounds),
        "a manifest, then its parts: {rounds}"
    );
    let found = value(&reply);
    assert_eq!(found["verdict"], "certain");
    assert_eq!(found["byIds"], false);
    assert_eq!(found["candidates"][0]["title"], "Mentor");

    // A ledger given as another family is refused.
    let wrong = serde_json::json!([{"family": "0".repeat(32), "ledger": ""}]).to_string();
    assert_eq!(
        error(&host.session.recognise(&back, &wrong))["kind"],
        "ledger"
    );

    // With its ids: recorded, reviewed and kept into the master.
    let found = value(&host.session.recognise(&back, &ledgers));
    assert_eq!(found["byIds"], true);
    let candidate = &found["candidates"][0];
    let args = |how: &str| {
        serde_json::json!({
            "name": "Resume - comments.docx",
            "checkpoint": candidate["checkpoint"],
            "sent": candidate["sent"],
            "matchedBy": how,
        })
        .to_string()
    };
    let refused = host
        .session
        .record_returned(&host.ledger, &stripped, &args("ids"), host.now);
    assert_eq!(error(&refused)["kind"], "notAllowed");
    let reply = host
        .session
        .record_returned(&host.ledger, &back, &args("ids"), host.now);
    let (recorded, _, _) = host.take(reply);
    let returned = str_of(&recorded["created"]);
    assert_eq!(
        error(
            &host
                .session
                .record_returned(&host.ledger, &back, "{}", host.now)
        )["kind"],
        "badInput"
    );

    let review = value(&host.session.returned_review(
        &host.ledger,
        &master,
        &master_file,
        &returned,
    ));
    assert_eq!(review["headline"], "3 changes from Mentor");
    let cards: Vec<Value> = review["view"]["changes"]
        .as_array()
        .expect("changes")
        .iter()
        .map(|c| serde_json::json!({"card": c["id"], "decision": "bringIn"}))
        .collect();
    let reply = host.session.decide_returned(
        &host.ledger,
        &master,
        &master_file,
        &returned,
        &serde_json::to_string(&cards).expect("json"),
        false,
        host.now,
    );
    let (outcome, file, _) = host.take(reply);
    assert!(outcome["ids"]["checkpoint"].is_string());
    let written = file.expect("written");
    assert_eq!(
        lineage_core::Docx::open(&written)
            .expect("opens")
            .body_lines(),
        lines(
            &resume::package(
                "t",
                &lineage_testkit::returned::light_edits(&resume::master_v1_lines())
            )
            .to_bytes()
        )
    );
    let opened = host.session.open_returned(&host.ledger, &returned);
    value(&opened);
    let mut opened = opened;
    assert_eq!(opened.file.take().expect("bytes"), back);

    // The sent copy opened in Word names its node and recipient.
    let sent = str_of(&found["candidates"][0]["sent"]);
    let open = value(&host.session.open_file(&host.ledger, &sent));
    assert_eq!(open["nodeId"], Value::String(master));
    assert_eq!(open["copyOf"]["recipient"], "Mentor");
    assert_eq!(open["isLatest"], false);
}

#[test]
fn my_documents_crosses_the_boundary() {
    let Started {
        mut host,
        data,
        master,
        ..
    } = started();
    let family = str_of(&value(&host.session.family(&host.ledger))["familyId"]);
    let observed =
        serde_json::json!([{"family": family, "version": host.ledger.len().to_string()}])
            .to_string();
    assert_eq!(
        value(&host.session.stale_families(None, &observed)),
        serde_json::json!([family])
    );
    let ledgers = serde_json::json!([{
        "family": family,
        "ledger": String::from_utf8(host.ledger.clone()).expect("utf-8"),
    }])
    .to_string();
    let missing = host.session.refresh_library(None, &observed, "[]");
    assert_eq!(error(&missing)["kind"], "badInput");
    let refreshed = host.session.refresh_library(None, &observed, &ledgers);
    let v = value(&refreshed);
    assert_eq!(v["indexChanged"], true);
    assert_eq!(v["library"]["entries"][0]["tailored"], 2);
    let index = refreshed.file.expect("index");
    let again = host.session.refresh_library(Some(&index), &observed, "[]");
    assert_eq!(value(&again)["indexChanged"], false);
    assert!(again.file.is_none(), "an index that is right isn't written");

    let twice = serde_json::json!([
        {"family": family, "version": "1"},
        {"family": family, "version": "2"},
    ])
    .to_string();
    assert_eq!(
        error(&host.session.stale_families(None, &twice))["kind"],
        "badInput"
    );
    assert_eq!(
        error(&host.session.update_index(None, "nope", "1", &host.ledger))["kind"],
        "badInput"
    );
    assert_eq!(error(&host.session.document(b"garbage"))["kind"], "ledger");

    let doc = value(&host.session.document(&host.ledger));
    assert_eq!(doc["page"]["canChange"], true);
    let v1 = str_of(&doc["page"]["files"][0]["entries"][0]["id"]);
    let reply = host
        .session
        .mark_milestone(&host.ledger, &v1, Some(""), host.now);
    assert_eq!(
        value(&reply)["nothingChanged"],
        true,
        "an empty label is no mark"
    );
    let reply = host
        .session
        .restore_as_new_version(&host.ledger, &v1, "  ", "x", host.now);
    assert_eq!(error(&reply)["kind"], "badInput");
    let reply = host
        .session
        .relink(&host.ledger, &data.0, " ", &data.1, host.now);
    assert_eq!(error(&reply)["kind"], "badInput");
    let reply = host
        .session
        .relink(&host.ledger, &master, "Moved.docx", &data.1, host.now);
    assert_eq!(error(&reply)["kind"], "wrongFile");

    // A fresh engine compares only once it has the snapshots.
    let fresh = Session::new([3; 32]);
    let reply = fresh.compare(&host.ledger, &v1, &v1);
    assert_eq!(error(&reply)["kind"], "missingBlobs");
}
