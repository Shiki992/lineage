// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;
use std::path::Path as FsPath;

use lineage_testkit::{docx::Package, fixtures, resume};

use super::*;
use crate::docx::Docx;

fn open_lines(lines: &[resume::Line]) -> Docx {
    Docx::open(&resume::package("t", lines).to_bytes()).expect("opens")
}

fn fixture(name: &str) -> Package {
    fixtures::all()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("no fixture {name}"))
        .1
}

fn open(package: &Package) -> Docx {
    Docx::open(&package.to_bytes()).expect("opens")
}

/// Validates a change list against spec/format/v0 with boon.
fn assert_valid(list: &spec::ChangeList) {
    let schema_path = FsPath::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/format/v0/change-list.schema.json");
    let schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(schema_path).expect("schema")).expect("json");
    let id = "https://github.com/Shiki992/lineage/spec/format/v0/change-list.schema.json";
    let mut schemas = boon::Schemas::new();
    let mut compiler = boon::Compiler::new();
    compiler.add_resource(id, schema).expect("resource");
    let index = compiler.compile(id, &mut schemas).expect("compiles");
    let instance = serde_json::to_value(list).expect("serialises");
    if let Err(error) = schemas.validate(&instance, index) {
        panic!("invalid change list:\n{error:#}\n{}", list.to_json());
    }
}

fn kinds(d: &DocDiff) -> Vec<ChangeKind> {
    d.changes.iter().map(|c| c.kind).collect()
}

#[test]
fn a_document_has_no_changes_against_itself() {
    for (name, package) in fixtures::all() {
        let docx = open(&package);
        let d = diff(&docx, &docx);
        assert!(d.is_empty(), "{name}: {:?}", d.changes);
    }
}

#[test]
fn a_new_job_is_two_insertions_under_experience() {
    let d = diff(
        &open_lines(&resume::master_v1_lines()),
        &open_lines(&resume::master_v2_lines()),
    );
    assert_eq!(kinds(&d), [ChangeKind::Insert, ChangeKind::Insert]);
    let first = &d.changes[0];
    assert_eq!(first.after_text.as_deref(), Some(resume::NEW_JOB_TITLE));
    assert_eq!(first.gap, Some(Path::top(4)));
    assert_eq!(first.heading.as_deref(), Some("Experience"));
    assert_eq!(first.unit, Unit::Paragraph);
    assert_eq!(
        d.changes[1].gap,
        Some(Path::top(4)),
        "both follow the same block"
    );
    assert_valid(&spec::change_list(&d.changes, |_| None));
}

#[test]
fn edits_and_formatting_are_separate_changes() {
    let d = diff(
        &open_lines(&resume::master_v2_lines()),
        &open_lines(&resume::master_v3_lines()),
    );
    assert_eq!(kinds(&d), [ChangeKind::Format, ChangeKind::Modify]);
    let format = &d.changes[0];
    assert!(format.formatting_only);
    assert_eq!(
        format.format_notes,
        [FormatNote::Toggle {
            property: "b",
            on: true
        }]
    );
    assert_eq!(format.char_range, Some((0, 11)));
    let modify = &d.changes[1];
    assert!(!modify.formatting_only);
    assert_eq!(modify.before_text.as_deref(), Some(resume::CORVIA_TITLE));
    assert_eq!(modify.after_text.as_deref(), Some(resume::CORVIA_TITLE_V3));
    assert_eq!(
        modify.segments,
        [
            Segment {
                kind: SegmentKind::Removed,
                text: "Data".into()
            },
            Segment {
                kind: SegmentKind::Added,
                text: "Lead data".into()
            },
            Segment {
                kind: SegmentKind::Same,
                text: " engineer, Corvia Freight, 2020 to 2023".into()
            },
        ]
    );
    assert!(modify.confidence >= 50 && modify.confidence < 100);
    assert_valid(&spec::change_list(&d.changes, |_| None));
}

#[test]
fn reordered_sections_are_moves_not_deletions() {
    let d = diff(
        &open_lines(&resume::master_v1_lines()),
        &open_lines(&resume::reordered_lines()),
    );
    assert!(!d.changes.is_empty());
    assert!(
        d.changes.iter().all(|c| c.kind == ChangeKind::Move),
        "{:?}",
        kinds(&d)
    );
    for change in &d.changes {
        assert_eq!(change.before_text, change.after_text);
        assert_eq!(change.confidence, 100);
    }
    assert_valid(&spec::change_list(&d.changes, |_| None));
}

#[test]
fn a_tailored_version_shows_its_own_edits() {
    let d = diff(
        &open_lines(&resume::master_v1_lines()),
        &open_lines(&resume::tailored_data_lines()),
    );
    let mut seen: Vec<ChangeKind> = kinds(&d);
    seen.sort();
    seen.dedup();
    assert!(
        seen.contains(&ChangeKind::Modify),
        "summary rewritten: {:?}",
        kinds(&d)
    );
    assert!(seen.contains(&ChangeKind::Delete), "bullet removed");
    assert!(seen.contains(&ChangeKind::Move), "skills moved");
    assert!(!seen.contains(&ChangeKind::Insert));
    assert_valid(&spec::change_list(&d.changes, |_| None));
}

#[test]
fn ids_are_stable_and_unique() {
    let a = open_lines(&resume::master_v1_lines());
    let b = open_lines(&resume::tailored_data_lines());
    let first: Vec<String> = diff(&a, &b).changes.into_iter().map(|c| c.id).collect();
    let second: Vec<String> = diff(&a, &b).changes.into_iter().map(|c| c.id).collect();
    assert_eq!(first, second);
    let unique: BTreeSet<&String> = first.iter().collect();
    assert_eq!(unique.len(), first.len());
    assert!(first.iter().all(|id| id.len() == 17 && id.starts_with('c')));
}

#[test]
fn equal_paragraphs_added_twice_get_different_ids() {
    let mut lines = resume::master_v1_lines();
    lines.push(resume::Line::Plain("References on request.".into()));
    lines.push(resume::Line::Plain("References on request.".into()));
    let d = diff(&open_lines(&resume::master_v1_lines()), &open_lines(&lines));
    assert_eq!(d.changes.len(), 2);
    assert_ne!(d.changes[0].id, d.changes[1].id);
}

fn table_package(rows: &[(&str, &str, &str)]) -> Package {
    use lineage_testkit::docx::{plain, styled};
    let cells: String = rows
        .iter()
        .map(|(x, y, z)| {
            let tc = |t: &str| {
                format!(
                    r#"<w:tc><w:tcPr><w:tcW w:w="3000" w:type="dxa"/></w:tcPr>{}</w:tc>"#,
                    plain(t)
                )
            };
            format!("<w:tr>{}{}{}</w:tr>", tc(x), tc(y), tc(z))
        })
        .collect();
    let table = format!(
        r#"<w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/><w:tblW w:w="9000" w:type="dxa"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>{cells}</w:tbl>"#
    );
    Package::new(
        "t",
        [styled("Heading1", "Traction"), table, plain("After.")].concat(),
    )
}

#[test]
fn table_cells_and_rows_have_cell_locations() {
    let before = table_package(&[
        ("Quarter", "Revenue", "Customers"),
        ("Q3 2026", "$26k", "17"),
    ]);
    let after = table_package(&[
        ("Quarter", "Revenue", "Customers"),
        ("Q3 2026", "$31k", "17"),
        ("Q4 2026", "$34k", "23"),
    ]);
    let d = diff(&open(&before), &open(&after));
    assert_eq!(
        kinds(&d),
        [ChangeKind::Modify, ChangeKind::Insert],
        "{:?}",
        d.changes
    );
    let cell = &d.changes[0];
    assert_eq!(
        cell.after,
        Some(Path {
            block: 1,
            cells: vec![CellStep {
                row: 1,
                column: Some(1),
                block: Some(0)
            }]
        })
    );
    assert_eq!(cell.heading.as_deref(), Some("Traction"));
    let row = &d.changes[1];
    assert_eq!(row.unit, Unit::Row);
    assert_eq!(
        row.gap,
        Some(Path {
            block: 1,
            cells: vec![CellStep {
                row: 1,
                column: None,
                block: None
            }]
        })
    );
    let table = d
        .story(&StoryKey::Body)
        .and_then(|s| s.blocks.as_ref())
        .and_then(|s| s.tables.get(&1));
    assert!(table.is_some_and(|t| !t.structural));
    assert_valid(&spec::change_list(&d.changes, |_| None));
}

#[test]
fn changed_columns_make_the_table_one_change() {
    let before = fixture("table.docx");
    let mut after = fixture("table.docx");
    after.body = after
        .body
        .replace(r#"<w:gridSpan w:val="3"/>"#, r#"<w:gridSpan w:val="2"/>"#);
    let d = diff(&open(&before), &open(&after));
    assert_eq!(d.changes.len(), 1, "{:?}", d.changes);
    assert_eq!(d.changes[0].unit, Unit::Table);
    assert_eq!(d.changes[0].by_hand, Some(ByHand::TableStructure));
}

#[test]
fn header_changes_are_in_their_story() {
    let before = fixture("header-footer.docx");
    let mut after = fixture("header-footer.docx");
    after.header = after.header.map(|h| h.replace("Resume", "CV"));
    let d = diff(&open(&before), &open(&after));
    assert_eq!(d.changes.len(), 1);
    assert_eq!(
        d.changes[0].story,
        StoryKey::Header {
            section: 0,
            kind: "default".into()
        }
    );
    assert_eq!(d.changes[0].part, "word/header1.xml");
    assert_valid(&spec::change_list(&d.changes, |_| None));
}

#[test]
fn footnote_changes_are_by_hand() {
    let before = fixture("footnote.docx");
    let mut after = fixture("footnote.docx");
    after.footnotes = after
        .footnotes
        .map(|f| f.replace("invented body", "imaginary body"));
    let d = diff(&open(&before), &open(&after));
    let note = d
        .changes
        .iter()
        .find(|c| c.story == StoryKey::Footnotes)
        .expect("footnote change");
    assert_eq!(note.by_hand, Some(ByHand::Note));
    assert_valid(&spec::change_list(&d.changes, |_| None));
}

#[test]
fn every_fixture_pair_gives_a_valid_change_list() {
    let all = fixtures::all();
    let docs: Vec<Docx> = all.iter().map(|(_, p)| open(p)).collect();
    for (x, a) in docs.iter().enumerate() {
        for b in docs.iter().skip(x + 1).step_by(3) {
            let d = diff(a, b);
            assert_valid(&spec::change_list(&d.changes, |_| Some("s".into())));
        }
    }
}
