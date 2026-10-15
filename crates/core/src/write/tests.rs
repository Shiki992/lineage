// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use lineage_testkit::docx::{Package, Rel, para, plain, run, styled};
use lineage_testkit::resume::{self, Line};

use super::*;
use crate::docx::View;
use crate::merge::{self, Decision, Status};
use crate::testutil::{open, open_lines, open_view, resume_bytes, smoke};

const DATE: &str = "2026-10-15T09:30:00Z";

fn rev() -> Revision {
    Revision::lineage(DATE)
}

/// Brings in every clean item and checks the result.
fn bring_in_all(name: &str, base: &Docx, ours: &Docx, theirs: &Docx) -> Vec<u8> {
    let plan = merge::plan(base, ours, theirs);
    let decisions: Vec<(String, Decision)> = plan
        .items
        .iter()
        .filter(|i| i.status == Status::Clean)
        .map(|i| (i.change.id.clone(), Decision::BringIn))
        .collect();
    assert!(!decisions.is_empty(), "{name}: nothing clean to bring in");
    let bytes = apply(&plan, &decisions, &rev()).unwrap_or_else(|e| panic!("{name}: {e}"));
    check_output(name, ours, &bytes);
    bytes
}

/// Every output: reopens, rejecting everything gives ours back, revisions are by
/// Lineage with unique ids, and element order follows the schema.
fn check_output(name: &str, ours: &Docx, bytes: &[u8]) {
    smoke(name, bytes);
    let mut rejected = open_view(bytes, View::Rejected).body_lines();
    let ours_lines = ours.body_lines();
    // A paragraph added after the last one leaves its (empty) paragraph behind
    // when rejected: Word cannot remove the document's last paragraph mark.
    if rejected.len() == ours_lines.len() + 1 && rejected.last().is_some_and(String::is_empty) {
        rejected.pop();
    }
    assert_eq!(
        rejected, ours_lines,
        "{name}: rejecting gives this version back"
    );
    let docx = open(bytes);
    let text = docx.part_text(&docx.main_part).expect("text");
    let doc = crate::xml::parse("document", text).expect("parses");
    let mut ids = BTreeSet::new();
    let mut revisions = 0;
    for node in doc.descendants().filter(roxmltree::Node::is_element) {
        if node.tag_name().namespace() != Some(ns::W) {
            continue;
        }
        let name_ = node.tag_name().name();
        if matches!(name_, "ins" | "del" | "rPrChange" | "pPrChange") {
            revisions += 1;
            assert_eq!(node.attribute((ns::W, "author")), Some("Lineage"), "{name}");
            assert_eq!(node.attribute((ns::W, "date")), Some(DATE), "{name}");
            let id = node.attribute((ns::W, "id")).expect("id");
            assert!(
                ids.insert(id.to_owned()),
                "{name}: duplicate revision id {id}"
            );
        }
        check_order(name, node);
    }
    assert!(revisions > 0, "{name}: no tracked changes written");
}

/// Element order rules the writer must keep (ECMA-376 Part 1).
fn check_order(name: &str, node: roxmltree::Node<'_, '_>) {
    let kids: Vec<&str> = node
        .children()
        .filter(|c| c.is_element() && c.tag_name().namespace() == Some(ns::W))
        .map(|c| c.tag_name().name())
        .collect();
    let pos = |n: &str| kids.iter().position(|k| *k == n);
    match node.tag_name().name() {
        "pPr" => {
            // rPr, then sectPr, then pPrChange, all after the other properties.
            let tail = ["rPr", "sectPr", "pPrChange"];
            let mut last = None;
            for t in tail {
                if let Some(p) = pos(t) {
                    assert!(last.is_none_or(|l| p > l), "{name}: pPr order {kids:?}");
                    last = Some(p);
                }
            }
            if let Some(first_tail) = tail.iter().filter_map(|t| pos(t)).min() {
                assert!(
                    kids[first_tail..].iter().all(|k| tail.contains(k)),
                    "{name}: pPr order {kids:?}"
                );
            }
            if let Some(p) = pos("pPrChange") {
                assert_eq!(p, kids.len() - 1, "{name}: pPrChange last {kids:?}");
            }
        }
        "rPr" => {
            if let Some(p) = pos("rPrChange") {
                assert_eq!(p, kids.len() - 1, "{name}: rPrChange last {kids:?}");
            }
            let is_mark = node.parent().is_some_and(|p| p.tag_name().name() == "pPr");
            if is_mark && let Some(p) = pos("ins").or(pos("del")) {
                assert_eq!(p, 0, "{name}: mark revision first {kids:?}");
            }
        }
        "trPr" => {
            if let Some(p) = pos("ins").or(pos("del")) {
                assert_eq!(p, kids.len() - 1, "{name}: row revision last {kids:?}");
            }
        }
        _ => {}
    }
}

fn lines_of(lines: &[Line]) -> Vec<String> {
    open_lines(lines).body_lines()
}

#[test]
fn bringing_in_a_new_job() {
    let base = open_lines(&resume::master_v1_lines());
    let ours = open_lines(&resume::tailored_data_lines());
    let theirs = open_lines(&resume::master_v2_lines());
    let bytes = bring_in_all("new-job", &base, &ours, &theirs);
    let mut expected = resume::tailored_data_lines();
    let at = expected
        .iter()
        .position(|l| *l == Line::H1("Experience".into()))
        .expect("experience")
        + 1;
    expected.insert(at, Line::H2(resume::NEW_JOB_TITLE.into()));
    expected.insert(at + 1, Line::Bullet(resume::NEW_JOB_BULLET.into()));
    assert_eq!(open(&bytes).body_lines(), lines_of(&expected));
    // The new paragraphs keep their style and list.
    let docx = open(&bytes);
    let added = docx
        .body()
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Paragraph(p) if p.text() == resume::NEW_JOB_BULLET => Some(p),
            _ => None,
        })
        .expect("bullet");
    assert!(added.props.numbering.is_some());
}

#[test]
fn untouched_paragraphs_are_byte_identical() {
    let base = open_lines(&resume::master_v1_lines());
    let ours = open_lines(&resume::tailored_data_lines());
    let theirs = open_lines(&resume::master_v2_lines());
    let bytes = bring_in_all("untouched", &base, &ours, &theirs);
    let before = ours.part_text(&ours.main_part).expect("text");
    let after_docx = open(&bytes);
    let after = after_docx.part_text(&after_docx.main_part).expect("text");
    for block in &ours.body().blocks {
        let raw = &before[block.span()];
        assert!(after.contains(raw), "a paragraph changed: {raw}");
    }
    // Every other part is untouched.
    let original =
        crate::Package::read(&resume_bytes(&resume::tailored_data_lines())).expect("reads");
    let output = crate::Package::read(&bytes).expect("reads");
    for name in original.part_names() {
        if name != "word/document.xml" {
            assert_eq!(original.part(name), output.part(name), "{name}");
        }
    }
}

#[test]
fn bringing_in_an_edit_and_a_formatting_change() {
    let base = open_lines(&resume::master_v2_lines());
    let ours = open_lines(&resume::tailored_data_lines());
    let theirs = open_lines(&resume::master_v3_lines());
    let bytes = bring_in_all("edit-and-bold", &base, &ours, &theirs);
    let docx = open(&bytes);
    let lines = docx.body_lines();
    assert!(lines.contains(&resume::CORVIA_TITLE_V3.to_owned()));
    assert!(!lines.contains(&resume::CORVIA_TITLE.to_owned()));
    let title = match &docx.body().blocks[0] {
        Block::Paragraph(p) => p,
        other => panic!("{other:?}"),
    };
    assert!(
        title.runs.iter().all(|r| r.props.direct.get("b").is_some()),
        "the name is bold"
    );
    let old = open_view(&bytes, View::Rejected);
    let old_title = match &old.body().blocks[0] {
        Block::Paragraph(p) => p,
        other => panic!("{other:?}"),
    };
    assert!(
        old_title
            .runs
            .iter()
            .all(|r| r.props.direct.get("b").is_none())
    );
}

#[test]
fn bringing_in_deletions_including_the_last_paragraph() {
    let base = open_lines(&resume::master_v1_lines());
    let ours = open_lines(&resume::master_v1_lines());
    let mut theirs_lines = resume::master_v1_lines();
    theirs_lines
        .retain(|l| *l != Line::Bullet("Automated weekly claims reports in Python.".into()));
    theirs_lines.pop(); // the last paragraph
    let theirs = open_lines(&theirs_lines);
    let bytes = bring_in_all("deletions", &base, &ours, &theirs);
    let mut expected = lines_of(&theirs_lines);
    // Word cannot delete the last paragraph mark: an empty paragraph stays.
    expected.push(String::new());
    assert_eq!(open(&bytes).body_lines(), expected);
}

#[test]
fn bringing_in_a_move() {
    let base = open_lines(&resume::master_v1_lines());
    let ours = open_lines(&resume::master_v1_lines());
    let theirs = open_lines(&resume::reordered_lines());
    let bytes = bring_in_all("moves", &base, &ours, &theirs);
    let mut expected = lines_of(&resume::reordered_lines());
    // Education was the last paragraph; its mark stays where it was.
    expected.push(String::new());
    assert_eq!(open(&bytes).body_lines(), expected);
}

#[test]
fn using_the_masters_side_of_an_overlap() {
    let base = open_lines(&resume::master_v1_lines());
    let ours = open_lines(&resume::tailored_same_para_lines());
    let theirs = open_lines(&resume::master_v3_lines());
    let plan = merge::plan(&base, &ours, &theirs);
    let corvia = plan
        .items
        .iter()
        .find(|i| matches!(i.status, Status::Overlap(_)))
        .expect("overlap");
    assert!(corvia.masters_writable);
    let decisions = vec![(corvia.change.id.clone(), Decision::UseMasters)];
    let bytes = apply(&plan, &decisions, &rev()).expect("writes");
    check_output("overlap-masters", &ours, &bytes);
    assert!(
        open(&bytes)
            .body_lines()
            .contains(&resume::CORVIA_TITLE_V3.to_owned())
    );

    // Bringing in an overlap without choosing a side is refused.
    let refused = apply(
        &plan,
        &[(corvia.change.id.clone(), Decision::BringIn)],
        &rev(),
    );
    assert!(matches!(refused, Err(Error::CannotEdit(_))));
}

#[test]
fn keeping_both_additions() {
    let base = open_lines(&resume::master_v1_lines());
    let mut lines = resume::master_v1_lines();
    let experience = lines
        .iter()
        .position(|l| *l == Line::H1("Experience".into()))
        .expect("experience");
    lines.insert(
        experience + 1,
        Line::Plain("Open to contract roles.".into()),
    );
    let ours = open_lines(&lines);
    let theirs = open_lines(&resume::master_v2_lines());
    let plan = merge::plan(&base, &ours, &theirs);
    let decisions: Vec<(String, Decision)> = plan
        .items
        .iter()
        .map(|i| {
            (
                i.change.id.clone(),
                Decision::KeepBoth {
                    masters_first: false,
                },
            )
        })
        .collect();
    let bytes = apply(&plan, &decisions, &rev()).expect("writes");
    check_output("keep-both", &ours, &bytes);
    let got = open(&bytes).body_lines();
    let at = got
        .iter()
        .position(|l| l == "Open to contract roles.")
        .expect("mine");
    assert_eq!(got[at + 1], resume::NEW_JOB_TITLE);
    assert_eq!(got[at + 2], resume::NEW_JOB_BULLET);
}

#[test]
fn declining_writes_nothing() {
    let base = open_lines(&resume::master_v1_lines());
    let ours_bytes = resume_bytes(&resume::tailored_data_lines());
    let ours = open(&ours_bytes);
    let theirs = open_lines(&resume::master_v2_lines());
    let plan = merge::plan(&base, &ours, &theirs);
    let decisions: Vec<(String, Decision)> = plan
        .items
        .iter()
        .map(|i| (i.change.id.clone(), Decision::Decline))
        .collect();
    assert_eq!(
        apply(&plan, &decisions, &rev()).expect("writes"),
        ours_bytes
    );
}

fn table_doc(rows: &[[&str; 3]]) -> Docx {
    let cells: String = rows
        .iter()
        .map(|r| {
            let tc = |t: &str| {
                format!(
                    r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr>{}</w:tc>"#,
                    plain(t)
                )
            };
            format!("<w:tr>{}{}{}</w:tr>", tc(r[0]), tc(r[1]), tc(r[2]))
        })
        .collect();
    let table = format!(
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="9000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>{cells}</w:tbl>"#
    );
    open(
        &Package::new(
            "t",
            [styled("Heading1", "Traction"), table, plain("After.")].concat(),
        )
        .to_bytes(),
    )
}

#[test]
fn bringing_in_table_changes() {
    let base = table_doc(&[
        ["Quarter", "Revenue", "Customers"],
        ["Q2", "$18k", "12"],
        ["Q3", "$26k", "17"],
    ]);
    let ours = table_doc(&[
        ["Quarter", "Revenue", "Customers"],
        ["Q2", "$18k", "12"],
        ["Q3", "$26k", "18"],
    ]);
    let theirs = table_doc(&[
        ["Quarter", "Revenue", "Customers"],
        ["Q3", "$31k", "17"],
        ["Q4", "$34k", "23"],
    ]);
    let bytes = bring_in_all("table", &base, &ours, &theirs);
    assert_eq!(
        open(&bytes).body_lines(),
        [
            "Traction",
            "Quarter",
            "Revenue",
            "Customers",
            "Q3",
            "$31k",
            "18",
            "Q4",
            "$34k",
            "23",
            "After."
        ]
    );
}

#[test]
fn a_new_hyperlink_brings_its_relationship() {
    let link = |url_id: &'static str| {
        para(
            "",
            &[
                run("Portfolio: "),
                format!(
                    r#"<w:hyperlink r:id="{url_id}" w:history="1">{}</w:hyperlink>"#,
                    run("example.com/avery")
                ),
            ]
            .concat(),
        )
    };
    let base_pkg = Package::new(
        "t",
        [styled("Title", "Avery Quinn"), plain("Contact")].concat(),
    );
    let mut theirs_pkg = Package::new(
        "t",
        [
            styled("Title", "Avery Quinn"),
            plain("Contact"),
            link("rIdWeb"),
        ]
        .concat(),
    );
    theirs_pkg.rels = vec![Rel::hyperlink("rIdWeb", "https://example.com/avery")];
    let base = open(&base_pkg.to_bytes());
    let ours = open(&base_pkg.to_bytes());
    let theirs = open(&theirs_pkg.to_bytes());
    let bytes = bring_in_all("hyperlink", &base, &ours, &theirs);
    let package = crate::Package::read(&bytes).expect("reads");
    let rels = String::from_utf8(
        package
            .part("word/_rels/document.xml.rels")
            .expect("rels")
            .to_vec(),
    )
    .expect("utf-8");
    assert!(
        rels.contains(r#"Target="https://example.com/avery" TargetMode="External""#),
        "{rels}"
    );
    let doc =
        String::from_utf8(package.part("word/document.xml").expect("doc").to_vec()).expect("utf-8");
    assert!(doc.contains(r#"r:id="rIdLineage1""#));
}

#[test]
fn revision_ids_continue_after_existing_ones() {
    let fixture = crate::testutil::fixture("tracked-changes.docx");
    let base = open(&fixture.to_bytes());
    let ours = open(&fixture.to_bytes());
    let mut theirs_pkg = crate::testutil::fixture("tracked-changes.docx");
    theirs_pkg.body.push_str(&plain("A new closing line."));
    let theirs = open(&theirs_pkg.to_bytes());
    let plan = merge::plan(&base, &ours, &theirs);
    let item = plan
        .items
        .iter()
        .find(|i| i.status == Status::Clean)
        .expect("clean insert");
    let bytes = apply(
        &plan,
        &[(item.change.id.clone(), Decision::BringIn)],
        &rev(),
    )
    .expect("writes");
    let doc = open(&bytes);
    let text = doc.part_text(&doc.main_part).expect("text");
    assert!(
        text.contains(r#"w:id="106""#),
        "new ids start above the highest existing (105)"
    );
}

#[test]
fn bad_revision_dates_are_refused() {
    let base = open_lines(&resume::master_v1_lines());
    let theirs = open_lines(&resume::master_v2_lines());
    let plan = merge::plan(&base, &base, &theirs);
    let decisions: Vec<(String, Decision)> = plan
        .items
        .iter()
        .map(|i| (i.change.id.clone(), Decision::BringIn))
        .collect();
    assert!(apply(&plan, &decisions, &Revision::lineage("yesterday")).is_err());
}
