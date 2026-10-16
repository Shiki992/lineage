// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Golden replies: one scenario through the boundary, every kind of reply
//! written to `packages/shell/test/golden/`. The TypeScript schemas must accept
//! every file there and the panel tests draw from them, so a change to a DTO
//! that TypeScript doesn't know about fails here or there (decision 0010).
//!
//! Run with `LINEAGE_UPDATE_GOLDEN=1` to rewrite the files after a deliberate
//! change, and commit them.

use std::path::PathBuf;

use lineage_core::Package;
use lineage_testkit::docx::{para, run};
use lineage_testkit::resume::{self, Line};
use lineage_wasm::api::{Reply, Session};
use serde_json::Value;

/// 2026-10-15T09:30:00Z.
const START: f64 = 1_792_056_600_000.0;

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../packages/shell/test/golden")
}

struct Golden {
    files: Vec<(String, String)>,
}

impl Golden {
    fn keep(&mut self, name: &str, reply: &Reply) -> Value {
        let value: Value = serde_json::from_str(&reply.json).expect("json");
        let mut text = serde_json::to_string_pretty(&value).expect("pretty");
        text.push('\n');
        self.files.push((format!("{name}.json"), text));
        value
    }

    fn finish(self) {
        let dir = golden_dir();
        if std::env::var_os("LINEAGE_UPDATE_GOLDEN").is_some() {
            std::fs::create_dir_all(&dir).expect("golden folder");
            for entry in std::fs::read_dir(&dir).expect("lists") {
                let path = entry.expect("entry").path();
                if path.extension().is_some_and(|e| e == "json") {
                    std::fs::remove_file(path).expect("removes an old file");
                }
            }
            for (name, text) in &self.files {
                std::fs::write(dir.join(name), text).expect("writes");
            }
            return;
        }
        let mut stale = Vec::new();
        for (name, text) in &self.files {
            let on_disk = std::fs::read_to_string(dir.join(name))
                .unwrap_or_default()
                .replace("\r\n", "\n");
            if &on_disk != text {
                stale.push(name.clone());
            }
        }
        let mut committed: Vec<String> = std::fs::read_dir(&dir)
            .map(|d| {
                d.filter_map(|e| e.ok()?.file_name().into_string().ok())
                    .filter(|n| n.ends_with(".json"))
                    .collect()
            })
            .unwrap_or_default();
        committed.retain(|n| !self.files.iter().any(|(f, _)| f == n));
        assert!(
            stale.is_empty() && committed.is_empty(),
            "golden replies are out of date: {stale:?}, extra: {committed:?}. \
             Run `LINEAGE_UPDATE_GOLDEN=1 cargo test -p lineage-wasm --test golden`, \
             check the change and update packages/shell/src/schemas.ts to match."
        );
    }
}

struct Host {
    session: Session,
    ledger: Vec<u8>,
    stored: Vec<(String, Vec<u8>)>,
    now: f64,
}

impl Host {
    fn take(&mut self, mut reply: Reply) -> (Option<Vec<u8>>, Option<Vec<u8>>) {
        let v: Value = serde_json::from_str(&reply.json).expect("json");
        assert_eq!(v["ok"], true, "{}", reply.json);
        for blob in &reply.blobs {
            self.stored
                .push((blob.hash.as_str().to_owned(), blob.bytes.clone()));
        }
        self.ledger.extend_from_slice(
            v["value"]["ledgerLines"]
                .as_str()
                .expect("lines")
                .as_bytes(),
        );
        self.now += 60_000.0;
        (reply.file.take(), reply.new_file.take())
    }
}

fn edit(file: &[u8], lines: &[Line]) -> Vec<u8> {
    let source = Package::read(&resume::package("t", lines).to_bytes()).expect("reads");
    let mut package = Package::read(file).expect("reads");
    package.set_part(
        "word/document.xml",
        source.part("word/document.xml").expect("doc").to_vec(),
    );
    package.to_bytes().expect("bytes")
}

fn id(v: &Value) -> String {
    v.as_str().expect("id").to_owned()
}

/// Master v3 plus a new section at the end, which is brought in by hand.
fn master_v3_with_section() -> Vec<Line> {
    let mut lines = resume::master_v3_lines();
    lines.push(Line::Raw(para(
        "<w:sectPr/>",
        &run("References are available on request."),
    )));
    lines
}

#[test]
fn golden_replies() {
    let mut golden = Golden { files: Vec::new() };
    let mut host = Host {
        session: Session::new([42; 32]),
        ledger: Vec::new(),
        stored: Vec::new(),
        now: START,
    };

    let file = resume::package("Avery Quinn master resume", &resume::master_v1_lines()).to_bytes();
    let reply = host
        .session
        .start_tracking(&file, "Resume.docx", "Resume.docx", host.now);
    golden.keep("start-tracking", &reply);
    let (master_file, _) = host.take(reply);
    let master_file = master_file.expect("file");
    golden.keep("read-ids", &host.session.read_ids(&master_file));
    golden.keep("read-ids-untracked", &host.session.read_ids(&file));
    golden.keep("master-panel-new", &host.session.master_panel(&host.ledger));

    let mut versions = Vec::new();
    for name in ["Resume – data roles.docx", "Resume – platform roles.docx"] {
        let reply = host
            .session
            .make_tailored_version(&host.ledger, name, name, host.now);
        if versions.is_empty() {
            golden.keep("make-tailored-version", &reply);
        }
        let v: Value = serde_json::from_str(&reply.json).expect("json");
        let node = id(&v["value"]["created"]);
        let (_, new_file) = host.take(reply);
        versions.push((node, new_file.expect("new file")));
    }
    let family = golden.keep("family", &host.session.family(&host.ledger));
    let master = id(&family["value"]["nodes"][0]["id"]);
    let ids: Value = serde_json::from_str(&host.session.read_ids(&versions[0].1).json).expect("j");
    golden.keep(
        "open-file",
        &host
            .session
            .open_file(&host.ledger, &id(&ids["value"]["checkpoint"])),
    );

    // The data version edits the Corvia title, which master v3 also edits.
    let data_edit = edit(&versions[0].1, &resume::tailored_same_para_lines());
    let reply = host.session.save_checkpoint(
        &host.ledger,
        &versions[0].0,
        &data_edit,
        "Tailored for data roles",
        host.now,
    );
    let (data_file, _) = host.take(reply);
    let data_file = data_file.expect("file");

    // The master gains a job (v2), then edits Corvia and adds a section (v3).
    let v2 = edit(&master_file, &resume::master_v2_lines());
    let reply = host
        .session
        .save_checkpoint(&host.ledger, &master, &v2, "New job", host.now);
    golden.keep("save-checkpoint", &reply);
    let (v2, _) = host.take(reply);
    let v3 = edit(&v2.expect("file"), &master_v3_with_section());
    golden.keep(
        "what-changed",
        &host.session.what_changed(&host.ledger, &master, &v3),
    );
    let reply = host
        .session
        .save_checkpoint(&host.ledger, &master, &v3, "Corvia title", host.now);
    host.take(reply);

    let panel = golden.keep(
        "master-panel-callout",
        &host.session.master_panel(&host.ledger),
    );
    let to = serde_json::to_string(&panel["value"]["callout"]["versions"]).expect("ids");
    let reply = host.session.offer_to_versions(&host.ledger, &to, host.now);
    golden.keep("offer-to-versions", &reply);
    host.take(reply);
    golden.keep(
        "master-panel-offered",
        &host.session.master_panel(&host.ledger),
    );
    golden.keep(
        "version-panel-updates",
        &host.session.version_panel(&host.ledger, &versions[0].0),
    );

    let updates = golden.keep(
        "updates",
        &host
            .session
            .updates(&host.ledger, &versions[0].0, &data_file),
    );
    let mut cards = Vec::new();
    for group in updates["value"]["groups"].as_array().expect("groups") {
        let view = &group["view"];
        for card in view["changes"].as_array().expect("changes") {
            cards.push(serde_json::json!({"card": card["id"], "decision": "bringIn"}));
        }
        for card in view["formatting"].as_array().expect("formatting") {
            cards.push(serde_json::json!({"card": card["id"], "decision": "decline"}));
        }
        for card in view["overlaps"].as_array().expect("overlaps") {
            cards.push(serde_json::json!({"card": card["id"], "decision": "keepMine"}));
        }
        for card in view["byHand"].as_array().expect("by hand") {
            cards.push(serde_json::json!({"card": card["id"], "decision": "decline"}));
        }
    }
    let has = |list: &str| {
        updates["value"]["groups"]
            .as_array()
            .expect("groups")
            .iter()
            .any(|g| !g["view"][list].as_array().expect(list).is_empty())
    };
    assert!(
        has("changes") && has("overlaps") && has("byHand"),
        "every kind of card"
    );
    let reply = host.session.decide(
        &host.ledger,
        &versions[0].0,
        &data_file,
        &serde_json::to_string(&cards).expect("json"),
        host.now,
    );
    golden.keep("decide", &reply);
    let (data_after, _) = host.take(reply);
    let data_after = data_after.expect("written");
    golden.keep(
        "version-panel",
        &host.session.version_panel(&host.ledger, &versions[0].0),
    );
    golden.keep(
        "check-written",
        &host.session.check_written(&data_after, &data_after),
    );
    golden.keep(
        "check-written-failed",
        &host.session.check_written(&data_after, &data_file),
    );

    let reply = host.session.send_copy(
        &host.ledger,
        &versions[0].0,
        &data_after,
        r#"{"recipient":"Northgate Analytics","purpose":"Data engineer","embedId":true}"#,
        host.now,
    );
    golden.keep("send-copy", &reply);
    let (_, copy) = host.take(reply);
    let copy = copy.expect("the copy");
    golden.keep("sent-list", &host.session.sent_list(&host.ledger, None));
    golden.keep(
        "sent-list-empty",
        &host.session.sent_list(&host.ledger, Some("nobody")),
    );

    // A file comes back, edited: recognised, recorded, reviewed, kept.
    let family_id = id(&family["value"]["familyId"]);
    let sent_id = {
        let ids = serde_json::from_str::<Value>(&host.session.read_ids(&copy).json).expect("json");
        id(&ids["value"]["checkpoint"])
    };
    golden.keep(
        "open-file-copy",
        &host.session.open_file(&host.ledger, &sent_id),
    );
    let back = {
        let mut package = Package::read(&copy).expect("reads");
        let doc = String::from_utf8(package.part("word/document.xml").expect("doc").to_vec())
            .expect("utf-8")
            .replace("Built the shipment", "Designed the shipment");
        package.set_part("word/document.xml", doc.into_bytes());
        package.to_bytes().expect("bytes")
    };
    let ledgers = |host: &Host| {
        serde_json::json!([{
            "family": family_id,
            "ledger": String::from_utf8(host.ledger.clone()).expect("utf-8"),
        }])
        .to_string()
    };
    let found = golden.keep(
        "recognise-certain",
        &host.session.recognise(&back, &ledgers(&host)),
    );
    let candidate = &found["value"]["candidates"][0];
    let reply = host.session.record_returned(
        &host.ledger,
        &back,
        &serde_json::json!({
            "name": "Resume - comments.docx",
            "checkpoint": candidate["checkpoint"],
            "sent": candidate["sent"],
            "matchedBy": "ids",
        })
        .to_string(),
        host.now,
    );
    let recorded = golden.keep("record-returned", &reply);
    let returned_id = id(&recorded["value"]["created"]);
    host.take(reply);
    golden.keep(
        "version-panel-returned",
        &host.session.version_panel(&host.ledger, &versions[0].0),
    );
    golden.keep(
        "sent-list-returned",
        &host.session.sent_list(&host.ledger, None),
    );
    let review = golden.keep(
        "returned-review",
        &host
            .session
            .returned_review(&host.ledger, &versions[0].0, &data_after, &returned_id),
    );
    let cards: Vec<Value> = review["value"]["view"]["changes"]
        .as_array()
        .expect("changes")
        .iter()
        .map(|c| serde_json::json!({"card": c["id"], "decision": "bringIn"}))
        .collect();
    let reply = host.session.decide_returned(
        &host.ledger,
        &versions[0].0,
        &data_after,
        &returned_id,
        &serde_json::to_string(&cards).expect("json"),
        false,
        host.now,
    );
    golden.keep("decide-returned", &reply);
    let (kept, _) = host.take(reply);
    let kept = kept.expect("written");
    let reply = host.session.send_copy(
        &host.ledger,
        &versions[1].0,
        &versions[1].1,
        r#"{"recipient":"Harbour Bank","purpose":null,"embedId":false}"#,
        host.now,
    );
    host.take(reply);
    let reply = host.session.send_copy(
        &host.ledger,
        &versions[1].0,
        &versions[1].1,
        r#"{"recipient":"Pellow Ventures","purpose":null,"embedId":false}"#,
        host.now,
    );
    let (_, twin) = host.take(reply);
    let twin = twin.expect("copy");
    golden.keep(
        "recognise-choose",
        &host.session.recognise(&twin, &ledgers(&host)),
    );
    golden.keep(
        "recognise-unknown",
        &host.session.recognise(
            &resume::package("t", &lineage_testkit::returned::stranger_lines()).to_bytes(),
            &ledgers(&host),
        ),
    );

    // The same copy, saved again in LibreOffice: only their words would come in.
    let resaved = {
        let mut package = Package::read(&copy).expect("reads");
        let doc = String::from_utf8(package.part("word/document.xml").expect("doc").to_vec())
            .expect("utf-8")
            .replace("Built the shipment", "Built and ran the shipment");
        package.set_part("word/document.xml", doc.into_bytes());
        package.set_part(
            "docProps/app.xml",
            br#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>LibreOffice/26.8.1.1$Windows_X86_64</Application></Properties>"#
                .to_vec(),
        );
        package.to_bytes().expect("bytes")
    };
    let reply = host.session.record_returned(
        &host.ledger,
        &resaved,
        &serde_json::json!({
            "name": "Resume - from LibreOffice.docx",
            "checkpoint": candidate["checkpoint"],
            "sent": candidate["sent"],
            "matchedBy": "ids",
        })
        .to_string(),
        host.now,
    );
    let resaved_id =
        id(&serde_json::from_str::<Value>(&reply.json).expect("json")["value"]["created"]);
    host.take(reply);
    golden.keep(
        "returned-review-words-only",
        &host
            .session
            .returned_review(&host.ledger, &versions[0].0, &kept, &resaved_id),
    );

    // Errors.
    let fresh = Session::new([7; 32]);
    golden.keep(
        "error-missing-blobs-family",
        &fresh.recognise(&twin, &ledgers(&host)),
    );
    golden.keep("error-missing-blobs", &fresh.master_panel(&host.ledger));
    golden.keep(
        "error-wrong-file",
        &host
            .session
            .what_changed(&host.ledger, &versions[0].0, &versions[1].1),
    );
    golden.keep(
        "error-bad-input",
        &host
            .session
            .what_changed(&host.ledger, "not an id", &data_after),
    );
    let mut other = Session::new([8; 32]);
    golden.keep(
        "error-not-allowed",
        &other.start_tracking(&master_file, "Resume.docx", "Resume.docx", START),
    );
    golden.keep(
        "error-engine",
        &other.start_tracking(b"not a docx", "Resume.docx", "Resume.docx", START),
    );
    golden.keep("referenced-blobs", &fresh.referenced_blobs(&host.ledger));
    assert!(!host.stored.is_empty());
    golden.finish();
}
