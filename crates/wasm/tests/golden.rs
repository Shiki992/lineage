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
    // Shared as an edit link, listed, then stopped (decision 0032).
    let reply = host.session.record_link(
        &host.ledger,
        &sent_id,
        r#"{"id":"perm-1","access":"edit","url":"https://1drv.ms/w/c/a1b2c3d4e5f60718/shared","fileRef":"Sent copies/Resume - data roles (Northgate Analytics).docx","scope":"anonymous"}"#,
        host.now,
    );
    golden.keep("record-link", &reply);
    host.take(reply);
    let shared = golden.keep(
        "sent-list-shared",
        &host.session.sent_list(&host.ledger, None),
    );
    let row = &shared["value"]["groups"][0]["rows"][0];
    assert_eq!(row["link"]["line"], "Shared as a link · Can edit");
    assert_eq!(row["shared"]["known"].as_array().map(Vec::len), Some(1));
    let reply = host.session.stop_link(&host.ledger, &sent_id, host.now);
    golden.keep("stop-link", &reply);
    host.take(reply);
    golden.keep(
        "error-link-not-shared",
        &host.session.stop_link(&host.ledger, &sent_id, host.now),
    );

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

    // My documents: the library and its index, a document's page and tree.
    let observed = serde_json::json!([
        {"family": family_id, "version": host.ledger.len().to_string()},
    ])
    .to_string();
    golden.keep(
        "stale-families",
        &host.session.stale_families(None, &observed),
    );
    let refreshed = host
        .session
        .refresh_library(None, &observed, &ledgers(&host));
    golden.keep("refresh-library", &refreshed);
    let index = refreshed.file.clone().expect("a new index");
    golden.keep(
        "library-from-index",
        &host.session.library_from_index(&index),
    );
    golden.keep(
        "library-from-index-untrusted",
        &host.session.library_from_index(b"{}"),
    );
    let updated = host.session.update_index(
        Some(&index),
        &family_id,
        &host.ledger.len().to_string(),
        &host.ledger,
    );
    assert_eq!(updated.file.as_deref(), Some(index.as_slice()));
    let document = golden.keep("document", &host.session.document(&host.ledger));
    let master_entries = &document["value"]["page"]["files"][0]["entries"];
    let newest = id(&master_entries[0]["id"]);
    let oldest = id(&master_entries[2]["id"]);
    let mut damaged = host.ledger.clone();
    damaged.truncate(damaged.len() - 25);
    golden.keep("document-damaged", &host.session.document(&damaged));
    golden.keep(
        "compare",
        &host.session.compare(&host.ledger, &oldest, &newest),
    );
    let reply = host.session.restore_as_new_version(
        &host.ledger,
        &oldest,
        "Resume – first draft.docx",
        "Tailored versions/Resume – first draft.docx",
        host.now,
    );
    golden.keep("restore-as-new-version", &reply);
    let (_, restored) = host.take(reply);
    assert!(restored.is_some());
    let reply = host.session.mark_milestone(
        &host.ledger,
        &newest,
        Some("Sent to ten companies"),
        host.now,
    );
    golden.keep("mark-milestone", &reply);
    host.take(reply);
    let reply = host.session.relink(
        &host.ledger,
        &versions[0].0,
        "Tailored versions/Resume – data roles (moved).docx",
        &kept,
        host.now,
    );
    golden.keep("relink", &reply);
    host.take(reply);

    deck_replies(&mut golden);
    workbook_replies(&mut golden);
    object_replies(&mut golden);

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

/// The founder's deck: the pitch deck panel, "Update N versions" and a sent deck.
fn deck_replies(golden: &mut Golden) {
    use lineage_core::docx::props::{read_ids, write_ids};
    use lineage_testkit::decks::{edits, ids, pitch_deck, traction_box};
    use lineage_testkit::pptx::Shape;

    let with_ids = |file: &[u8], built: &lineage_testkit::pptx::Deck| -> Vec<u8> {
        let ids = read_ids(&Package::read(file).expect("reads"))
            .expect("reads")
            .expect("ids");
        let mut package = Package::read(&built.to_bytes()).expect("reads");
        write_ids(&mut package, &ids).expect("writes");
        package.to_bytes().expect("bytes")
    };
    let mut host = Host {
        session: Session::new([43; 32]),
        ledger: Vec::new(),
        stored: Vec::new(),
        now: START,
    };
    let reply = host.session.start_tracking(
        &pitch_deck().to_bytes(),
        "Pitch deck.pptx",
        "Pitch deck.pptx",
        host.now,
    );
    let (master_file, _) = host.take(reply);
    let master_file = master_file.expect("ids written");
    let state: Value = serde_json::from_str(&host.session.family(&host.ledger).json).expect("json");
    let master = id(&state["value"]["nodes"][0]["id"]);
    let mut versions = Vec::new();
    for name in ["Pitch deck – Seed fund.pptx", "Pitch deck – Northgate.pptx"] {
        let reply = host.session.make_tailored_version(
            &host.ledger,
            name,
            &format!("Tailored versions/{name}"),
            host.now,
        );
        let created: Value = serde_json::from_str(&reply.json).expect("json");
        let node = id(&created["value"]["created"]);
        let (_, file) = host.take(reply);
        versions.push((node, file.expect("version file")));
    }
    let mut own = pitch_deck();
    for placed in &mut own.slide_mut(ids::TRACTION).shapes {
        if let Shape::TextBox { .. } = placed.shape {
            placed.shape = traction_box("$1.3M ARR");
        }
    }
    let northgate = with_ids(&versions[1].1, &own);
    let reply = host.session.save_checkpoint(
        &host.ledger,
        &versions[1].0,
        &northgate,
        "Northgate's numbers",
        host.now,
    );
    let (file, _) = host.take(reply);
    versions[1].1 = file.expect("checkpointed");

    let reply = host.session.send_copy(
        &host.ledger,
        &versions[0].0,
        &versions[0].1,
        r#"{"recipient":"Pellow Ventures","purpose":"Seed round","embedId":true}"#,
        host.now,
    );
    golden.keep("send-copy-deck", &reply);
    host.take(reply);

    let edited = with_ids(&master_file, &edits::traction_update(pitch_deck()));
    golden.keep(
        "deck-panel",
        &host.session.deck_panel(&host.ledger, Some(&edited)),
    );
    let reply = host.session.save_checkpoint(
        &host.ledger,
        &master,
        &edited,
        "New traction numbers",
        host.now,
    );
    host.take(reply);
    for (node, file) in &versions {
        let staged = host.session.stage_version_file(node, file);
        assert!(staged.json.contains(r#""ok":true"#));
    }
    let reply = host.session.update_versions(&host.ledger, "[]", host.now);
    let value = golden.keep("update-versions", &reply);
    assert_eq!(value["value"]["headline"], "Updated all 2 versions");
    let mut reply = reply;
    let seed_file = reply
        .files
        .iter()
        .find(|(n, _)| *n == versions[0].0)
        .map(|(_, f)| f.clone())
        .expect("seed fund written");
    let north_file = reply
        .files
        .iter()
        .find(|(n, _)| *n == versions[1].0)
        .map(|(_, f)| f.clone())
        .expect("northgate written");
    reply.files.clear();
    let v: Value = serde_json::from_str(&reply.json).expect("json");
    host.ledger.extend_from_slice(
        v["value"]["outcome"]["ledgerLines"]
            .as_str()
            .expect("lines")
            .as_bytes(),
    );

    // What Northgate, open in PowerPoint, still has to choose.
    let value = golden.keep(
        "updates-deck",
        &host
            .session
            .updates(&host.ledger, &versions[1].0, &north_file),
    );
    assert_eq!(
        value["value"]["groups"][0]["view"]["overlaps"][0]["summary"],
        "Both changed '$1.2M ARR'"
    );

    // Bring in for the open deck (decision 0019): the master's notes change,
    // into Seed fund open in PowerPoint.
    let edited = with_ids(
        &master_file,
        &edits::notes_edited(edits::traction_update(pitch_deck())),
    );
    let reply = host.session.save_checkpoint(
        &host.ledger,
        &master,
        &edited,
        "Notes for the traction slide",
        host.now,
    );
    host.take(reply);
    let reply =
        host.session
            .prepare_open_deck(&host.ledger, &versions[0].0, &seed_file, "[]", host.now);
    let value = golden.keep("prepare-open-deck", &reply);
    assert_eq!(value["value"]["version"]["text"], "Brought in 1 change");
    let merged = reply.file.expect("merged deck");
    golden.keep(
        "deck-slide-ops",
        &host.session.deck_slide_ops(&merged, &seed_file),
    );
    let reply = host.session.finish_open_deck(
        &host.ledger,
        &versions[0].0,
        &seed_file,
        &merged,
        "[]",
        host.now,
    );
    let value = golden.keep("finish-open-deck", &reply);
    assert!(value["value"]["ids"]["checkpoint"].is_string());
    host.take(reply);
    let reply = host.session.finish_open_deck(
        &host.ledger,
        &versions[0].0,
        &seed_file,
        &seed_file,
        "[]",
        host.now,
    );
    assert!(reply.json.contains(r#""ok":false"#), "{}", reply.json);
}

/// The JSON for "bring in" on every item id of a workbook card.
fn bring_in_ids(card: &Value) -> String {
    let cards: Vec<Value> = card["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .map(|c| serde_json::json!({"card": c, "decision": "bringIn"}))
        .collect();
    serde_json::to_string(&cards).expect("json")
}

/// The founder's financial model: what changed in the five categories with
/// Excel's hints, a cell side by side, the master's price brought into a
/// tailored workbook open in Excel, and a copy that came back kept in the
/// master (decision 0023).
fn workbook_replies(golden: &mut Golden) {
    use lineage_core::docx::props::{read_ids, write_ids};
    use lineage_testkit::workbooks::{self, Assumptions};

    let with_ids = |file: &[u8], built: &lineage_testkit::xlsx::Workbook| -> Vec<u8> {
        let ids = read_ids(&Package::read(file).expect("reads"))
            .expect("reads")
            .expect("ids");
        let mut package = Package::read(&built.to_bytes()).expect("reads");
        write_ids(&mut package, &ids).expect("writes");
        package.to_bytes().expect("bytes")
    };
    let model = |new_seats: i128, price: &str, churn: &str| {
        workbooks::financial_model(&Assumptions {
            new_seats,
            price: price.to_owned(),
            churn: churn.to_owned(),
            ..Assumptions::default()
        })
    };
    let base = Assumptions::default();
    let (seats, price, churn) = (base.new_seats, base.price.clone(), base.churn.clone());
    let mut host = Host {
        session: Session::new([44; 32]),
        ledger: Vec::new(),
        stored: Vec::new(),
        now: START,
    };
    let reply = host.session.start_tracking(
        &workbooks::model_master().to_bytes(),
        "Startup model.xlsx",
        "Startup model.xlsx",
        host.now,
    );
    let (master_file, _) = host.take(reply);
    let master_file = master_file.expect("ids written");
    let state: Value = serde_json::from_str(&host.session.family(&host.ledger).json).expect("json");
    let family_id = id(&state["value"]["familyId"]);
    let master = id(&state["value"]["nodes"][0]["id"]);
    let reply = host.session.make_tailored_version(
        &host.ledger,
        "Startup model - Northwind.xlsx",
        "Tailored versions/Startup model - Northwind.xlsx",
        host.now,
    );
    let created: Value = serde_json::from_str(&reply.json).expect("json");
    let northwind = id(&created["value"]["created"]);
    let (_, file) = host.take(reply);
    let north_file = with_ids(&file.expect("version file"), &model(12, &price, &churn));
    let reply = host.session.save_checkpoint(
        &host.ledger,
        &northwind,
        &north_file,
        "Northwind's seat plan",
        host.now,
    );
    let (file, _) = host.take(reply);
    let north_file = file.unwrap_or(north_file);

    // Send a copy: what goes with it, then the copy.
    golden.keep(
        "goes-with-copy",
        &host
            .session
            .goes_with_copy(&workbooks::awkward_cells().to_bytes()),
    );
    let reply = host.session.send_copy(
        &host.ledger,
        &master,
        &master_file,
        r#"{"recipient":"Harbor Lane Capital","purpose":"Series A","embedId":true}"#,
        host.now,
    );
    golden.keep("send-copy-workbook", &reply);
    let (_, copy) = host.take(reply);
    let copy = copy.expect("the copy");

    // The founder changes the price in Excel.
    let edited = with_ids(&master_file, &model(seats, "55", &churn));
    let value = golden.keep(
        "workbook-what-changed",
        &host.session.workbook_what_changed(
            &host.ledger,
            &master,
            &edited,
            r#"[{"sheet":"Assumptions","address":"B4"}]"#,
        ),
    );
    assert_eq!(
        value["value"]["workbook"]["groups"][0]["cards"][0]["why"],
        "Edited in this session"
    );
    // The same edit, reported by Excel as a co-author's (decision 0032).
    let value = golden.keep(
        "workbook-what-changed-co-author",
        &host.session.workbook_what_changed(
            &host.ledger,
            &master,
            &edited,
            r#"[{"sheet":"Assumptions","address":"B4","by":"coAuthor"}]"#,
        ),
    );
    assert_eq!(
        value["value"]["workbook"]["groups"][0]["cards"][0]["why"],
        "Edited by someone you share it with, in this session"
    );
    golden.keep(
        "cell-side-by-side",
        &host.session.cell_side_by_side(
            &host.ledger,
            &master,
            &edited,
            r#"{"sheet":"Assumptions","address":"B4"}"#,
        ),
    );
    let reply = host
        .session
        .save_checkpoint(&host.ledger, &master, &edited, "New price", host.now);
    let (file, _) = host.take(reply);
    let master_file = file.unwrap_or(edited);
    let reply = host.session.offer_to_versions(
        &host.ledger,
        &serde_json::to_string(&[&northwind]).expect("json"),
        host.now,
    );
    host.take(reply);

    // Northwind, open in Excel, brings the price in.
    let updates = golden.keep(
        "updates-workbook",
        &host.session.updates(&host.ledger, &northwind, &north_file),
    );
    let cards =
        bring_in_ids(&updates["value"]["groups"][0]["view"]["workbook"]["groups"][0]["cards"][0]);
    let value = golden.keep(
        "prepare-workbook-offer",
        &host
            .session
            .prepare_workbook_offer(&host.ledger, &northwind, &north_file, &cards),
    );
    assert_eq!(value["value"]["text"], "Brings in 1 change");
    // Excel didn't take it: nothing is recorded.
    let reply = host.session.finish_workbook_offer(
        &host.ledger,
        &northwind,
        &north_file,
        &north_file,
        &cards,
        host.now,
    );
    golden.keep("error-not-landed", &reply);
    // The way back after Excel carried out the plan's one step, and the check
    // that the workbook is as it was.
    let value = golden.keep(
        "workbook-way-back",
        &host
            .session
            .workbook_way_back(&host.ledger, &northwind, &north_file, &cards, 1, None),
    );
    assert_eq!(value["value"][0]["value"]["text"], "49");
    golden.keep(
        "workbook-put-back",
        &host
            .session
            .workbook_put_back(&north_file, &north_file, "[]"),
    );
    // What Excel holds after carrying out the plan.
    let held = with_ids(&north_file, &model(12, "55", &churn));
    let reply = host.session.finish_workbook_offer(
        &host.ledger,
        &northwind,
        &north_file,
        &held,
        &cards,
        host.now,
    );
    let value = golden.keep("finish-workbook-offer", &reply);
    assert!(value["value"]["ids"]["checkpoint"].is_string(), "{value}");
    host.take(reply);

    // Harbor Lane's copy comes back with a different churn rate.
    let back = with_ids(&copy, &model(seats, &price, "0.03"));
    let ledgers = serde_json::json!([{
        "family": family_id,
        "ledger": String::from_utf8(host.ledger.clone()).expect("utf-8"),
    }])
    .to_string();
    let found: Value =
        serde_json::from_str(&host.session.recognise(&back, &ledgers).json).expect("json");
    let candidate = &found["value"]["candidates"][0];
    let reply = host.session.record_returned(
        &host.ledger,
        &back,
        &serde_json::json!({
            "name": "Startup model - Harbor Lane comments.xlsx",
            "checkpoint": candidate["checkpoint"],
            "sent": candidate["sent"],
            "matchedBy": "ids",
        })
        .to_string(),
        host.now,
    );
    let recorded: Value = serde_json::from_str(&reply.json).expect("json");
    let returned = id(&recorded["value"]["created"]);
    host.take(reply);
    let review = golden.keep(
        "returned-review-workbook",
        &host
            .session
            .returned_review(&host.ledger, &master, &master_file, &returned),
    );
    let cards = bring_in_ids(&review["value"]["view"]["workbook"]["groups"][0]["cards"][0]);
    let value = golden.keep(
        "prepare-workbook-keep",
        &host
            .session
            .prepare_workbook_keep(&host.ledger, &master, &master_file, &returned, &cards),
    );
    assert_eq!(value["value"]["text"], "Keeps 1 change");
    let held = with_ids(&master_file, &model(seats, "55", "0.03"));
    let reply = host.session.finish_workbook_keep(
        &host.ledger,
        &master,
        &master_file,
        &held,
        &returned,
        &cards,
        host.now,
    );
    let value = golden.keep("finish-workbook-keep", &reply);
    assert_eq!(value["ok"], true, "{value}");
    host.take(reply);
}

/// Phase 5B: the founder's model with macros, pivots and queries. What
/// changed when a macro, a pivot and a query change; a copy that came back
/// with new macro code; and the document page's "Contains macros".
fn object_replies(golden: &mut Golden) {
    use lineage_core::docx::props::{read_ids, write_ids};
    use lineage_testkit::advanced;

    let with_ids = |file: &[u8], built: &lineage_testkit::xlsx::Workbook| -> Vec<u8> {
        let ids = read_ids(&Package::read(file).expect("reads"))
            .expect("reads")
            .expect("ids");
        let mut package = Package::read(&built.to_bytes()).expect("reads");
        write_ids(&mut package, &ids).expect("writes");
        package.to_bytes().expect("bytes")
    };
    let mut host = Host {
        session: Session::new([45; 32]),
        ledger: Vec::new(),
        stored: Vec::new(),
        now: START,
    };
    let reply = host.session.start_tracking(
        &advanced::model_full().to_bytes(),
        "Startup model.xlsm",
        "Startup model.xlsm",
        host.now,
    );
    let (master_file, _) = host.take(reply);
    let master_file = master_file.expect("ids written");
    let state: Value = serde_json::from_str(&host.session.family(&host.ledger).json).expect("json");
    let family_id = id(&state["value"]["familyId"]);
    let master = id(&state["value"]["nodes"][0]["id"]);

    // A macro, a pivot table and a query changed together.
    let mut edited = advanced::model_full_macro_edited();
    advanced::pivot_mut(&mut edited, "SeatsByPlan").rows = 2;
    let edited = with_ids(&master_file, &edited);
    let value = golden.keep(
        "workbook-what-changed-objects",
        &host
            .session
            .workbook_what_changed(&host.ledger, &master, &edited, "[]"),
    );
    let groups = value["value"]["workbook"]["groups"]
        .as_array()
        .expect("groups");
    let objects: Vec<&str> = groups.iter().filter_map(|g| g["object"].as_str()).collect();
    assert_eq!(objects, ["macros", "pivots"], "{value}");
    assert!(
        value["value"]["workbook"]["newMacroCode"]
            .as_bool()
            .expect("bool")
    );

    // A copy goes out and comes back with new macro code.
    let reply = host.session.send_copy(
        &host.ledger,
        &master,
        &master_file,
        r#"{"recipient":"Harbor Lane Capital","purpose":"Series A","embedId":true}"#,
        host.now,
    );
    let (_, copy) = host.take(reply);
    let back = with_ids(
        &copy.expect("the copy"),
        &advanced::model_full_returned_with_macro(),
    );
    let ledgers = serde_json::json!([{
        "family": family_id,
        "ledger": String::from_utf8(host.ledger.clone()).expect("utf-8"),
    }])
    .to_string();
    let found: Value =
        serde_json::from_str(&host.session.recognise(&back, &ledgers).json).expect("json");
    let candidate = &found["value"]["candidates"][0];
    let reply = host.session.record_returned(
        &host.ledger,
        &back,
        &serde_json::json!({
            "name": "Startup model - Harbor Lane.xlsm",
            "checkpoint": candidate["checkpoint"],
            "sent": candidate["sent"],
            "matchedBy": "ids",
        })
        .to_string(),
        host.now,
    );
    let recorded: Value = serde_json::from_str(&reply.json).expect("json");
    let returned = id(&recorded["value"]["created"]);
    host.take(reply);
    let review = golden.keep(
        "returned-review-macros",
        &host
            .session
            .returned_review(&host.ledger, &master, &master_file, &returned),
    );
    assert!(
        review["value"]["view"]["workbook"]["newMacroCode"]
            .as_bool()
            .expect("bool"),
        "{review}"
    );
    let sent = golden.keep(
        "sent-list-macros",
        &host.session.sent_list(&host.ledger, None),
    );
    assert!(
        sent.to_string().contains("including new macro code"),
        "{sent}"
    );
    let document = golden.keep("document-macros", &host.session.document(&host.ledger));
    assert_eq!(
        document["value"]["page"]["badges"][0], "Contains macros",
        "{document}"
    );
}
