// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_testkit::docx::{Package as Builder, para, plain, run, run_with, styled};
use lineage_testkit::fixtures;

use super::*;
use crate::docx::props::{custom_properties, read_ids};
use crate::package::Relationships;
use crate::testutil::{open, smoke};

fn ids() -> LineageIds {
    LineageIds {
        family: "0f1e2d3c4b5a69788796a5b4c3d2e1f0".to_owned(),
        checkpoint: "00112233445566778899aabbccddeeff".to_owned(),
    }
}

/// Everything a clean copy must not contain, checked in every XML part.
fn assert_clean(name: &str, bytes: &[u8], with_ids: bool) {
    let package = Package::read(bytes).expect("reads");
    for part in package.part_names() {
        let lower = part.to_ascii_lowercase();
        assert!(!removable(&lower), "{name}: {part} should be gone");
        if !part.ends_with(".xml") {
            continue;
        }
        let text = std::str::from_utf8(package.part(part).expect("part")).expect("utf-8");
        let doc = xml::parse(part, text).expect("parses");
        for node in doc.descendants().filter(Node::is_element) {
            if node.tag_name().namespace() == Some(ns::W) {
                let tag = node.tag_name().name();
                assert!(
                    !matches!(
                        tag,
                        "ins"
                            | "del"
                            | "moveFrom"
                            | "moveTo"
                            | "commentRangeStart"
                            | "commentRangeEnd"
                            | "commentReference"
                    ) && !REVISION_PROPS.contains(&tag)
                        && !REVISION_RANGES.contains(&tag),
                    "{name}: {part} still has w:{tag}"
                );
                if tag == "r" {
                    assert!(!vanished(node), "{name}: hidden text in {part}");
                }
                if matches!(tag, "docVars" | "rsids" | "attachedTemplate") {
                    panic!("{name}: settings still have w:{tag}");
                }
            }
            for attr in node.attributes() {
                assert!(
                    !(attr.namespace() == Some(ns::W) && attr.name().starts_with("rsid")),
                    "{name}: {part} still has rsids"
                );
            }
        }
    }
    let props = custom_properties(&package).expect("reads");
    if with_ids {
        let names: Vec<&str> = props.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(
            names,
            [FAMILY_ID, CHECKPOINT_ID],
            "{name}: only the two ids"
        );
        assert_eq!(read_ids(&package).expect("reads"), Some(ids()));
    } else {
        assert!(props.is_empty(), "{name}: no custom properties");
        assert!(custom_part(&package).is_none());
        let types = String::from_utf8(package.part("[Content_Types].xml").expect("types").to_vec())
            .expect("utf-8");
        assert!(
            !types.contains("custom-properties"),
            "{name}: no content type left behind"
        );
    }
    let core =
        std::str::from_utf8(package.part("docProps/core.xml").expect("core")).expect("utf-8");
    assert!(
        core.trim_end().ends_with("/>"),
        "{name}: core properties are empty"
    );
    for part in package.part_names().filter(|n| n.ends_with(".rels")) {
        let rels = Relationships::parse(part, package.part(part).expect("rels")).expect("parses");
        let source = source_of(part);
        for rel in &rels.items {
            if !rel.external {
                let target = resolve_target(&source, &rel.target);
                assert!(
                    package.contains(&target),
                    "{name}: {part} points at missing {target}"
                );
            }
        }
    }
}

#[test]
fn every_fixture_cleans_to_its_accepted_text() {
    for (name, fixture) in fixtures::all() {
        let bytes = fixture.to_bytes();
        let original = open(&bytes);
        let (clean, _) = clean_copy(&bytes, Some(&ids())).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_clean(name, &clean, true);
        let copy = open(&clean);
        assert_eq!(
            copy.body_lines(),
            original.body_lines(),
            "{name}: the text a reader sees is unchanged"
        );
        smoke(&format!("clean-{}", name.trim_end_matches(".docx")), &clean);
    }
}

#[test]
fn without_the_id_there_are_no_custom_properties() {
    for name in [
        "custom-properties.docx",
        "headings-paragraphs.docx",
        "comment.docx",
    ] {
        let bytes = fixtures::all()
            .into_iter()
            .find(|(n, _)| *n == name)
            .expect("fixture")
            .1
            .to_bytes();
        let (clean, _) = clean_copy(&bytes, None).expect("cleans");
        assert_clean(name, &clean, false);
    }
}

#[test]
fn the_report_counts_what_went() {
    let comment = fixtures::all()
        .into_iter()
        .find(|(n, _)| *n == "comment.docx")
        .expect("fixture")
        .1;
    let (_, report) = clean_copy(&comment.to_bytes(), Some(&ids())).expect("cleans");
    assert_eq!(report.comments, 3);
    assert!(
        report
            .parts_removed
            .iter()
            .any(|p| p == "word/comments.xml")
    );

    let tracked = fixtures::all()
        .into_iter()
        .find(|(n, _)| *n == "tracked-moves-and-tables.docx")
        .expect("fixture")
        .1;
    let (_, report) = clean_copy(&tracked.to_bytes(), Some(&ids())).expect("cleans");
    assert!(report.revisions >= 10, "{report:?}");
}

#[test]
fn hidden_text_is_removed() {
    let body = [
        styled("Title", "Avery Quinn"),
        para(
            "",
            &[
                run("Visible. "),
                run_with("<w:vanish/>", "Note to self: tailored for Northgate."),
                run("Also visible."),
            ]
            .concat(),
        ),
        plain("The end."),
    ]
    .concat();
    let bytes = Builder::new("t", body).to_bytes();
    let (clean, report) = clean_copy(&bytes, Some(&ids())).expect("cleans");
    assert_eq!(report.hidden_runs, 1);
    assert_clean("hidden", &clean, true);
    assert_eq!(open(&clean).body_lines()[1], "Visible. Also visible.");
}

#[test]
fn a_tracked_change_written_by_lineage_is_accepted() {
    use crate::merge::{self, Decision, Status};
    use crate::write::{Revision, apply};
    use lineage_testkit::resume;
    let base = crate::testutil::open_lines(&resume::master_v1_lines());
    let theirs = crate::testutil::open_lines(&resume::master_v3_lines());
    let plan = merge::plan(&base, &base, &theirs);
    let decisions: Vec<(String, Decision)> = plan
        .items
        .iter()
        .filter(|i| i.status == Status::Clean)
        .map(|i| (i.change.id.clone(), Decision::BringIn))
        .collect();
    let merged = apply(
        &plan,
        &decisions,
        &Revision::lineage("2026-10-15T09:30:00Z"),
    )
    .expect("writes");
    let (clean, report) = clean_copy(&merged, Some(&ids())).expect("cleans");
    assert!(report.revisions > 0);
    assert_clean("lineage-revisions", &clean, true);
    assert_eq!(open(&clean).body_lines(), theirs.body_lines());
}
