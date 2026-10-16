// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use lineage_testkit::docx::{Package, Rel, para, plain, run, run_with, styled};
use lineage_testkit::resume::{self, Line};
use lineage_testkit::returned::{self as kit, LIBREOFFICE, Saved};

use super::*;
use crate::docx::{ParaProps, RunProps, View};
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
    let rejected = open_view(bytes, View::Rejected).body_lines();
    assert_eq!(
        rejected,
        ours.body_lines(),
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
    // No empty paragraph is left where the last one was.
    assert_eq!(open(&bytes).body_lines(), lines_of(&theirs_lines));
}

/// Every way the end of the document can change, each checked both ways:
/// accepting gives the master's version and rejecting gives this version back,
/// with no empty paragraph left over.
#[test]
fn the_end_of_the_document_changes_without_leftover_paragraphs() {
    let base_lines = resume::master_v1_lines();
    let n = base_lines.len();
    let mut cases: Vec<(&str, Vec<Line>)> = Vec::new();
    // Two paragraphs added after the last one.
    let mut added = base_lines.clone();
    added.push(Line::H1("References".into()));
    added.push(Line::Plain("Available on request.".into()));
    cases.push(("end-added", added));
    // The last paragraph replaced by two others.
    let mut replaced = base_lines.clone();
    replaced.pop();
    replaced.push(Line::Plain(
        "BSc Mathematics, Lakeside University, 2018".into(),
    ));
    replaced.push(Line::Plain("Data science certificate, 2021".into()));
    cases.push(("end-replaced", replaced));
    // The last paragraph edited, and one added after it.
    let mut edited = base_lines.clone();
    edited[n - 1] =
        Line::Plain("BSc Computer Science (honours), Northbridge University, 2018".into());
    edited.push(Line::Plain("Dean's list, three years.".into()));
    cases.push(("end-edited-and-added", edited));
    // The last two paragraphs deleted.
    let mut cut = base_lines.clone();
    cut.truncate(n - 2);
    cases.push(("end-cut", cut));
    // A heading made the last paragraph by deleting what followed it, then restyled.
    let mut restyled = base_lines.clone();
    restyled.truncate(n - 1);
    restyled[n - 2] = Line::H2("Education".into());
    cases.push(("end-restyled", restyled));
    for (name, theirs_lines) in cases {
        let base = open_lines(&base_lines);
        let theirs = open_lines(&theirs_lines);
        let bytes = bring_in_all(name, &base, &base, &theirs);
        assert_eq!(
            open(&bytes).body_lines(),
            theirs.body_lines(),
            "{name}: accepting gives the master's"
        );
    }
}

#[test]
fn the_end_of_a_table_cell_changes_without_leftover_paragraphs() {
    let cell = |paras: &[&str]| -> String {
        let body: String = paras.iter().map(|t| plain(t)).collect();
        format!(r#"<w:tc><w:tcPr><w:tcW w:w="4500" w:type="dxa"/></w:tcPr>{body}</w:tc>"#)
    };
    let doc = |a: &[&str], b: &[&str]| {
        let table = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="9000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="4500"/><w:gridCol w:w="4500"/></w:tblGrid><w:tr>{}{}</w:tr></w:tbl>"#,
            cell(a),
            cell(b)
        );
        open(
            &Package::new(
                "t",
                [styled("Title", "Skills"), table, plain("After.")].concat(),
            )
            .to_bytes(),
        )
    };
    let base = doc(&["SQL", "Python"], &["dbt", "Airflow"]);
    let theirs = doc(&["SQL"], &["dbt", "Airflow", "Spark"]);
    let bytes = bring_in_all("cell-ends", &base, &base, &theirs);
    assert_eq!(open(&bytes).body_lines(), theirs.body_lines());
}

/// Found on real documents: Word writes an empty paragraph as `<w:p .../>`,
/// and rewriting it must open the tag rather than close it twice.
#[test]
fn self_closing_paragraphs_are_rewritten_safely() {
    let table = r#"<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let base_body = [
        styled("Title", "Avery Quinn"),
        table.to_owned(),
        r#"<w:p w:rsidR="00A1B2C3"/>"#.to_owned(),
    ]
    .concat();
    let added = [base_body.clone(), plain("Added after the empty paragraph.")].concat();
    let replaced = [
        styled("Title", "Avery Quinn"),
        table.to_owned(),
        plain("Now with text."),
    ]
    .concat();
    let base = open(&Package::new("t", base_body).to_bytes());
    for (name, body) in [
        ("self-closing-append", added),
        ("self-closing-rewrite", replaced),
    ] {
        let theirs = open(&Package::new("t", body).to_bytes());
        let bytes = bring_in_all(name, &base, &base, &theirs);
        assert_eq!(open(&bytes).body_lines(), theirs.body_lines(), "{name}");
    }
}

/// Checked in Word: a deleted paragraph right before a table goes entirely when
/// accepted, and an inserted one goes entirely when rejected.
#[test]
fn paragraphs_before_tables_come_and_go_cleanly() {
    let table = r#"<w:tbl><w:tblPr><w:tblW w:w="4000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="4000"/></w:tblGrid><w:tr><w:tc><w:tcPr><w:tcW w:w="4000" w:type="dxa"/></w:tcPr><w:p><w:r><w:t>Cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#;
    let doc = |middle: &str| {
        open(
            &Package::new(
                "t",
                [
                    plain("First"),
                    middle.to_owned(),
                    table.to_owned(),
                    plain("Last"),
                ]
                .concat(),
            )
            .to_bytes(),
        )
    };
    let without = doc("");
    let with = doc(&plain("Just before the table"));
    let added = bring_in_all("before-table-added", &without, &without, &with);
    assert_eq!(open(&added).body_lines(), with.body_lines());
    let removed = bring_in_all("before-table-removed", &with, &with, &without);
    assert_eq!(open(&removed).body_lines(), without.body_lines());
}

#[test]
fn bringing_in_a_move() {
    let base = open_lines(&resume::master_v1_lines());
    let ours = open_lines(&resume::master_v1_lines());
    let theirs = open_lines(&resume::reordered_lines());
    let bytes = bring_in_all("moves", &base, &ours, &theirs);
    assert_eq!(
        open(&bytes).body_lines(),
        lines_of(&resume::reordered_lines())
    );
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
fn a_paragraph_tag_with_a_prefix_ours_lacks_is_still_written() {
    // Found in Phase 3: a Google Docs export puts `w14:paraId` on every
    // paragraph. The tag itself is never copied, so the change is writable.
    let w14 = |text: &str, n: u32| {
        format!(
            r#"<w:p w14:paraId="{n:08X}"><w:pPr><w:pStyle w:val="ListParagraph"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr>{}</w:p>"#,
            run(text)
        )
    };
    let base_lines = resume::master_v1_lines();
    let base = open_lines(&base_lines);
    let mut theirs_pkg = resume::package("t", &base_lines);
    theirs_pkg.doc_ns = vec![
        (
            "mc",
            "http://schemas.openxmlformats.org/markup-compatibility/2006",
        ),
        (
            "w14",
            "http://schemas.microsoft.com/office/word/2010/wordml",
        ),
    ];
    theirs_pkg.ignorable = Some("w14");
    theirs_pkg.body = theirs_pkg.body.replacen(
        &resume::package(
            "t",
            &[Line::Bullet(
                "Built the shipment data warehouse used by forty analysts.".into(),
            )],
        )
        .body,
        &w14(
            "Designed the shipment data warehouse used by forty analysts.",
            7,
        ),
        1,
    );
    assert!(theirs_pkg.body.contains("w14:paraId"));
    let theirs = open(&theirs_pkg.to_bytes());
    let plan = merge::plan(&base, &base, &theirs);
    assert!(
        plan.items.iter().all(|i| i.status == Status::Clean),
        "{:?}",
        plan.items.iter().map(|i| &i.status).collect::<Vec<_>>()
    );
    let bytes = bring_in_all("w14-tag", &base, &base, &theirs);
    let docx = open(&bytes);
    assert!(
        !docx
            .part_text(&docx.main_part)
            .expect("text")
            .contains("w14:")
    );
    assert!(
        open_view(&bytes, View::Accepted)
            .body_lines()
            .contains(&"Designed the shipment data warehouse used by forty analysts.".to_owned())
    );
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

// Words only (decision 0014): keeping a change from a file another app saved
// brings in the words, in this file's formatting.

/// Keeps every clean change to the words, words only, and checks the output.
fn keep_words(name: &str, base: &Docx, ours: &Docx, theirs: &Docx) -> Vec<u8> {
    let plan = merge::plan(base, ours, theirs);
    let decisions: Vec<(String, Decision)> = plan
        .items
        .iter()
        .filter(|i| i.status == Status::Clean && !i.change.formatting_only)
        .map(|i| (i.change.id.clone(), Decision::BringIn))
        .collect();
    assert!(!decisions.is_empty(), "{name}: nothing to keep");
    let bytes = apply_with(&plan, &decisions, &rev(), Bring::WordsOnly)
        .unwrap_or_else(|e| panic!("{name}: {e}"));
    check_output(name, ours, &bytes);
    bytes
}

/// How every body paragraph looks: its text, its formatting, and the run
/// formatting of every character.
fn looks(docx: &Docx) -> Vec<(String, ParaProps, Vec<RunProps>)> {
    docx.body()
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => {
                let mut runs = Vec::new();
                for inline in &p.inlines {
                    if let Inline::Text { text, run } = inline {
                        runs.extend(std::iter::repeat_n(
                            p.runs[*run].props.clone(),
                            text.chars().count(),
                        ));
                    }
                }
                Some((p.text(), p.props.clone(), runs))
            }
            _ => None,
        })
        .collect()
}

fn main_xml(bytes: &[u8]) -> String {
    let docx = open(bytes);
    docx.part_text(&docx.main_part).expect("text").to_owned()
}

/// Nothing of another app's formatting arrived, and no run formatting was
/// tracked. (Paragraph formatting can be: where the end of the document
/// changes, the last paragraph mark passes between two of this file's own
/// paragraph formats, see `ends.rs`.)
fn assert_words_only(name: &str, bytes: &[u8], ours: &Docx) {
    let xml = main_xml(bytes);
    let before = ours.part_text(&ours.main_part).expect("text");
    for trace in ["Arial", "w:rtl", r#"w:line="276""#, "rPrChange"] {
        assert_eq!(
            xml.matches(trace).count(),
            before.matches(trace).count(),
            "{name}: {trace}"
        );
    }
}

#[test]
fn words_from_a_google_docs_export_come_in_with_our_formatting() {
    let base_lines = resume::master_v1_lines();
    let base = open_lines(&base_lines);
    for (name, edited) in [
        ("words-google-light", kit::light_edits(&base_lines)),
        ("words-google-heavy", kit::heavy_edits(&base_lines)),
        ("words-google-moved", resume::reordered_lines()),
    ] {
        let theirs = open(&kit::package("t", &edited, Saved::GoogleExport).to_bytes());
        let bytes = keep_words(name, &base, &base, &theirs);
        assert_words_only(name, &bytes, &base);
        if name != "words-google-moved" {
            assert!(!main_xml(&bytes).contains("pPrChange"), "{name}");
        }
        // Exactly as if the same words had been typed into this file.
        assert_eq!(looks(&open(&bytes)), looks(&open_lines(&edited)), "{name}");
    }
}

#[test]
fn the_end_of_the_document_changes_words_only() {
    let base_lines = resume::master_v1_lines();
    let n = base_lines.len();
    let mut added = base_lines.clone();
    added.push(Line::H1("References".into()));
    added.push(Line::Plain("Available on request.".into()));
    let mut replaced = base_lines.clone();
    replaced.pop();
    replaced.push(Line::Plain("Data science certificate, 2021".into()));
    replaced.push(Line::H1("Volunteering".into()));
    replaced.push(Line::Bullet("Runs a weekend coding club.".into()));
    let mut edited = base_lines.clone();
    edited[n - 1] =
        Line::Plain("BSc Computer Science (honours), Northbridge University, 2018".into());
    edited.push(Line::Bullet(
        "On the dean\u{2019}s list for three years.".into(),
    ));
    let base = open_lines(&base_lines);
    for (name, lines) in [
        ("words-end-added", added),
        ("words-end-replaced", replaced),
        ("words-end-edited-and-added", edited),
    ] {
        let theirs = open(&kit::package("t", &lines, Saved::GoogleExport).to_bytes());
        let bytes = keep_words(name, &base, &base, &theirs);
        assert_words_only(name, &bytes, &base);
        assert_eq!(looks(&open(&bytes)), looks(&open_lines(&lines)), "{name}");
    }
}

/// Keeps every change to the words in each edited real re-save in `folder`
/// (made from the inputs in `kit::LIBREOFFICE`), words only: none of `traces`
/// arrive, and the result looks exactly as if the same words had been typed
/// into the file that left.
fn real_resaves_bring_only_their_words(folder: &str, traces: &[&str]) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/docx/returned")
        .join(folder);
    let mut kept = 0;
    for input in LIBREOFFICE.iter().filter(|i| i.edits != kit::Edits::None) {
        let base = open(&resume::package("t", &input.source.lines()).to_bytes());
        let theirs = open(&std::fs::read(root.join(input.name)).expect("fixture"));
        let expected = input.edits.apply(&input.source.lines());
        // The real app kept every word that went in.
        assert_eq!(theirs.body_lines(), lines_of(&expected), "{}", input.name);
        let name = format!("words-{folder}-{}", input.name.trim_end_matches(".docx"));
        let bytes = keep_words(&name, &base, &base, &theirs);
        let xml = main_xml(&bytes);
        for trace in traces.iter().chain(&["pPrChange", "rPrChange"]) {
            assert!(!xml.contains(trace), "{name}: {trace}");
        }
        assert_eq!(
            looks(&open(&bytes)),
            looks(&open_lines(&expected)),
            "{name}"
        );
        kept += 1;
    }
    assert_eq!(kept, 4);
}

#[test]
fn real_libreoffice_saves_bring_only_their_words() {
    real_resaves_bring_only_their_words(
        "libreoffice",
        &[r#"w:val="Normal""#, "widowControl", "w:bidi"],
    );
}

#[test]
fn real_google_docs_exports_bring_only_their_words() {
    // Google writes `w:rtl` on every run and `w14:paraId` on every paragraph.
    real_resaves_bring_only_their_words("google", &["w:rtl", "w14:"]);
}

/// A paragraph of runs, each `(rpr, text)`.
fn runs_para(ppr: &str, runs: &[(&str, &str)]) -> String {
    let content: String = runs
        .iter()
        .map(|(rpr, text)| {
            if rpr.is_empty() {
                run(text)
            } else {
                run_with(rpr, text)
            }
        })
        .collect();
    para(ppr, &content)
}

fn one_para_doc(body: &str) -> Docx {
    open(
        &Package::new(
            "t",
            [
                styled("Title", "Avery Quinn"),
                body.to_owned(),
                plain("After."),
            ]
            .concat(),
        )
        .to_bytes(),
    )
}

/// The text and the run formatting under each character of the paragraph
/// after the title.
fn chars_of(bytes: &[u8]) -> (String, Vec<RunProps>) {
    let all = looks(&open(bytes));
    (all[1].0.clone(), all[1].2.clone())
}

const ARIAL: &str = r#"<w:rFonts w:ascii="Arial" w:hAnsi="Arial"/>"#;

#[test]
fn added_words_take_the_formatting_of_their_neighbours() {
    // A bold label: words added inside it are bold, words added after it are not.
    let ours = one_para_doc(&runs_para(
        "",
        &[("<w:b/>", "Languages:"), ("", " SQL and Python")],
    ));
    let bold_arial = format!("{ARIAL}<w:b/>");
    let theirs = one_para_doc(&runs_para(
        "",
        &[
            (&bold_arial, "Languages and tools:"),
            (ARIAL, " SQL, Python and dbt"),
        ],
    ));
    let bytes = keep_words("words-neighbours", &ours, &ours, &theirs);
    assert_words_only("words-neighbours", &bytes, &ours);
    let (text, props) = chars_of(&bytes);
    assert_eq!(text, "Languages and tools: SQL, Python and dbt");
    let label = "Languages and tools:".chars().count();
    for (i, p) in props.iter().enumerate() {
        assert_eq!(p.direct.get("b").is_some(), i < label, "character {i}");
    }

    // A replaced word takes the formatting of the word it replaces.
    let ours = one_para_doc(&runs_para(
        "",
        &[("", "Led "), ("<w:i/>", "three"), ("", " projects")],
    ));
    let italic_arial = format!("{ARIAL}<w:i/>");
    let theirs = one_para_doc(&runs_para(
        "",
        &[
            (ARIAL, "Led "),
            (&italic_arial, "four"),
            (ARIAL, " projects"),
        ],
    ));
    let bytes = keep_words("words-replaced", &ours, &ours, &theirs);
    assert_words_only("words-replaced", &bytes, &ours);
    let (text, props) = chars_of(&bytes);
    assert_eq!(text, "Led four projects");
    let italic: Vec<bool> = props.iter().map(|p| p.direct.get("i").is_some()).collect();
    assert_eq!(
        italic,
        [[false; 4].as_slice(), &[true; 4], &[false; 9]].concat()
    );
}

#[test]
fn a_new_paragraph_with_nothing_alike_keeps_its_own_formatting() {
    let base_lines = resume::master_v1_lines();
    let mut lines = base_lines.clone();
    lines.push(Line::Raw(styled("Quote", "Avery always ships on time.")));
    let base = open_lines(&base_lines);
    let theirs = open_lines(&lines);
    let bytes = keep_words("words-nothing-alike", &base, &base, &theirs);
    let last = looks(&open(&bytes)).pop().expect("a paragraph");
    assert_eq!(last.0, "Avery always ships on time.");
    assert_eq!(last.1.style.as_deref(), Some("Quote"));
}

#[test]
fn keeping_a_formatting_change_in_words_only_mode_brings_the_formatting() {
    let base = open_lines(&resume::master_v2_lines());
    let theirs = open_lines(&resume::master_v3_lines());
    let plan = merge::plan(&base, &base, &theirs);
    let decisions: Vec<(String, Decision)> = plan
        .items
        .iter()
        .filter(|i| i.status == Status::Clean)
        .map(|i| (i.change.id.clone(), Decision::BringIn))
        .collect();
    assert!(plan.items.iter().any(|i| i.change.formatting_only));
    let words = apply_with(&plan, &decisions, &rev(), Bring::WordsOnly).expect("writes");
    let everything = apply(&plan, &decisions, &rev()).expect("writes");
    check_output("words-format-kept", &base, &words);
    // The name made bold comes in either way; the edit is the same words.
    assert_eq!(looks(&open(&words)), looks(&open(&everything)));
}

#[test]
fn words_only_in_a_table_cell() {
    let doc = |cells: [&str; 3], rpr: &str| {
        let tc = |t: &str| {
            format!(
                r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr>{}</w:tc>"#,
                runs_para("", &[(rpr, t)])
            )
        };
        let table = format!(
            r#"<w:tbl><w:tblPr><w:tblW w:w="9000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid><w:tr>{}{}{}</w:tr></w:tbl>"#,
            tc(cells[0]),
            tc(cells[1]),
            tc(cells[2])
        );
        open(
            &Package::new(
                "t",
                [styled("Title", "Traction"), table, plain("After.")].concat(),
            )
            .to_bytes(),
        )
    };
    let ours = doc(["Q3", "$26k", "17 customers"], "<w:b/>");
    let theirs = doc(["Q3", "$31k", "17 customers"], &format!("{ARIAL}<w:b/>"));
    let bytes = keep_words("words-cell", &ours, &ours, &theirs);
    assert_words_only("words-cell", &bytes, &ours);
    assert!(open(&bytes).body_lines().contains(&"$31k".to_owned()));
}

#[test]
fn using_their_side_of_an_overlap_words_only() {
    let base_lines = resume::master_v1_lines();
    let base = open_lines(&base_lines);
    let theirs_of =
        |lines: &[Line]| open(&kit::package("t", lines, Saved::GoogleExport).to_bytes());
    let mut with_mine = base_lines.clone();
    let experience = with_mine
        .iter()
        .position(|l| *l == Line::H1("Experience".into()))
        .expect("experience");
    with_mine.insert(
        experience + 1,
        Line::Plain("Open to contract roles.".into()),
    );
    let cases: [(&str, Vec<Line>, Vec<Line>, Decision); 3] = [
        (
            "words-overlap-both-edited",
            resume::tailored_same_para_lines(),
            resume::master_v3_lines(),
            Decision::UseMasters,
        ),
        (
            "words-overlap-edited-vs-deleted",
            resume::tailored_deleted_job_lines(),
            resume::master_v3_lines(),
            Decision::UseMasters,
        ),
        (
            "words-overlap-both-added",
            with_mine,
            resume::master_v2_lines(),
            Decision::KeepBoth {
                masters_first: false,
            },
        ),
    ];
    for (name, ours_lines, theirs_lines, decision) in cases {
        let ours = open_lines(&ours_lines);
        let theirs = theirs_of(&theirs_lines);
        let plan = merge::plan(&base, &ours, &theirs);
        let decisions: Vec<(String, Decision)> = plan
            .items
            .iter()
            .filter(|i| {
                matches!(i.status, Status::Overlap(_)) && i.change.kind != ChangeKind::Format
            })
            .map(|i| (i.change.id.clone(), decision))
            .collect();
        assert!(!decisions.is_empty(), "{name}: no overlap");
        let bytes = apply_with(&plan, &decisions, &rev(), Bring::WordsOnly)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        check_output(name, &ours, &bytes);
        assert_words_only(name, &bytes, &ours);
        // Their words, laid out the way the same decision lays out a Word file.
        let word_theirs = open_lines(&theirs_lines);
        let word_plan = merge::plan(&base, &ours, &word_theirs);
        let word_decisions: Vec<(String, Decision)> = word_plan
            .items
            .iter()
            .filter(|i| {
                matches!(i.status, Status::Overlap(_)) && i.change.kind != ChangeKind::Format
            })
            .map(|i| (i.change.id.clone(), decision))
            .collect();
        let expected = apply(&word_plan, &word_decisions, &rev()).expect("writes");
        assert_eq!(
            open(&bytes).body_lines(),
            open(&expected).body_lines(),
            "{name}"
        );
    }
}
