// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_testkit::workbooks;
use lineage_testkit::xlsx as kit;

use super::refs::{number_key, parse_cell};
use super::*;

fn open(wb: &kit::Workbook) -> Workbook {
    Workbook::open(&wb.to_bytes()).expect("opens")
}

fn cell<'a>(wb: &'a Workbook, sheet: &str, at: &str) -> &'a Cell {
    let index = wb.sheet_index(sheet).expect("sheet");
    let (col, row) = parse_cell(at).expect("a cell");
    wb.sheets[index]
        .cell(col, row)
        .unwrap_or_else(|| panic!("no {sheet}!{at}"))
}

#[test]
fn every_fixture_opens_and_round_trips_unchanged() {
    for (name, wb) in workbooks::all()
        .into_iter()
        .chain(lineage_testkit::advanced::all())
    {
        let bytes = wb.to_bytes();
        let opened = Workbook::open(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(opened.sheets.len(), wb.sheets.len(), "{name}");
        for (ours, theirs) in opened.sheets.iter().zip(&wb.rendered_sheets()) {
            assert_eq!(ours.name, theirs.name, "{name}");
            assert_eq!(ours.rows.len(), theirs.rows.len(), "{name}: {}", ours.name);
        }
        let package = Package::read(&bytes).expect("reads");
        let again = Package::read(&package.to_bytes().expect("writes")).expect("rereads");
        for part in package.part_names() {
            assert!(again.is_unchanged(part), "{name}: {part}");
            assert_eq!(package.part(part), again.part(part), "{name}: {part}");
        }
    }
}

#[test]
fn the_model_has_its_sheets_names_formulas_and_formats() {
    let wb = open(&workbooks::model_master());
    let names: Vec<&str> = wb.sheets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Assumptions", "Revenue", "P&L", "Summary"]);
    assert!(!wb.macro_enabled);
    assert_eq!(wb.extension(), ".xlsx");
    let defined: Vec<&str> = wb.names.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(
        defined,
        [
            "Churn",
            "CogsShare",
            "NewSeats",
            "Opex",
            "Price",
            "StartSeats"
        ]
    );

    let revenue = wb.sheet_index("Revenue").expect("revenue");
    let text = wb.sheet_text(revenue);
    let c2 = cell(&wb, "Revenue", "C2");
    let Content::Formula(f) = &c2.content else {
        panic!("C2 is a formula");
    };
    assert_eq!(
        f.text.as_ref().map(|t| t.get(text)),
        Some("B2*Price*(1-Churn)")
    );
    assert!(matches!(
        f.kind,
        FormulaKind::Shared {
            si: 1,
            range: Some(_)
        }
    ));
    assert!(matches!(&f.cached, Some(Cached::Number(n)) if n.get(text) == "5762.4"));
    let c3 = cell(&wb, "Revenue", "C3");
    let Content::Formula(f) = &c3.content else {
        panic!("C3 is a formula");
    };
    assert!(f.text.is_none());
    assert!(matches!(f.kind, FormulaKind::Shared { si: 1, range: None }));

    let price = cell(&wb, "Assumptions", "B4");
    let assumptions = wb.sheet_index("Assumptions").expect("assumptions");
    assert!(
        matches!(price.content, Content::Number(s) if s.get(wb.sheet_text(assumptions)) == "49")
    );
    assert_eq!(wb.styles.format_code(price.style), "\"$\"#,##0");
    assert_eq!(
        wb.styles.format_code(cell(&wb, "Assumptions", "B5").style),
        "0.00%"
    );
    let label = cell(&wb, "Assumptions", "A4");
    let Content::Shared(i) = label.content else {
        panic!("a shared string");
    };
    assert_eq!(
        wb.shared(i).map(|s| s.text.as_str()),
        Some("Price per seat")
    );
    assert_eq!(wb.sheets[assumptions].comments, ["xl/comments1.xml"]);
    let summary = &wb.sheets[wb.sheet_index("Summary").expect("summary")];
    assert_eq!(summary.merged.len(), 1);
    assert_ne!(summary.keys.conditional, SheetKeys::default().conditional);
    // The same format in another style table position is the same key.
    assert_eq!(wb.styles.key(0), wb.styles.key(0));
    assert_ne!(wb.styles.key(price.style), wb.styles.key(0));
}

#[test]
fn awkward_cells_keep_what_they_are() {
    let wb = open(&workbooks::awkward_cells());
    let text = wb.sheet_text(0);
    let shared = |at: &str| match cell(&wb, "Cells", at).content {
        Content::Shared(i) => wb.shared(i).map(|s| s.text.clone()),
        _ => None,
    };
    assert_eq!(shared("B2").as_deref(), Some("00123"));
    assert_eq!(shared("B3").as_deref(), Some("1234567890123456789"));
    assert_eq!(shared("B4").as_deref(), Some("TRUE"));
    assert_eq!(shared("B16").as_deref(), Some("trailing "));
    let number = |at: &str| match cell(&wb, "Cells", at).content {
        Content::Number(s) => Some(s.get(text).to_owned()),
        _ => None,
    };
    assert_eq!(number("B5").as_deref(), Some("1E-3"));
    assert_eq!(number("B6").as_deref(), Some("0.30000000000000004"));
    assert_eq!(number("B7").as_deref(), Some("-0"));
    assert_eq!(number("B8").as_deref(), Some("12345678901234567000"));
    assert_eq!(cell(&wb, "Cells", "B9").content, Content::Bool(true));
    assert!(
        matches!(&cell(&wb, "Cells", "B10").content, Content::Error(e) if e.get(text) == "#N/A")
    );
    assert_eq!(
        cell(&wb, "Cells", "B11").content,
        Content::Inline("Written by another app".into())
    );
    let cached = |at: &str| match &cell(&wb, "Cells", at).content {
        Content::Formula(f) => f.cached.clone(),
        _ => None,
    };
    assert!(
        matches!(cached("B12"), Some(Cached::Text(t)) if t.get(text) == "Leading zeros and more")
    );
    assert!(matches!(cached("B13"), Some(Cached::Error(e)) if e.get(text) == "#DIV/0!"));
    assert_eq!(cached("B14"), Some(Cached::Bool(true)));
    let Content::Formula(array) = &cell(&wb, "Cells", "B15").content else {
        panic!("an array formula");
    };
    assert!(matches!(array.kind, FormulaKind::Array { range: Some(_) }));
    // The formula's `&quot;` was unescaped into owned text.
    let Content::Formula(f) = &cell(&wb, "Cells", "B12").content else {
        panic!("a formula");
    };
    assert_eq!(
        f.text.as_ref().map(|t| t.get(text)),
        Some("A2&\" and more\"")
    );
    assert_eq!(wb.sheets[1].state, SheetState::Hidden);
    assert_eq!(wb.sheets[2].state, SheetState::VeryHidden);
}

#[test]
fn tables_filters_and_hidden_rows() {
    let wb = open(&workbooks::budget_master());
    assert_eq!(wb.tables.len(), 1);
    let table = &wb.tables[0];
    assert_eq!(table.name, "Transactions");
    assert_eq!(table.columns, ["Date", "Category", "Description", "Amount"]);
    assert_eq!(table.area.rows(), 17);
    assert!(!table.query);

    let wb = open(&workbooks::tracker_filtered());
    let sheet = &wb.sheets[0];
    assert!(sheet.auto_filter.is_some());
    let hidden: Vec<u32> = sheet
        .rows
        .iter()
        .filter(|r| r.hidden)
        .map(|r| r.r)
        .collect();
    assert_eq!(hidden, [4, 7]);
}

#[test]
fn data_connections_and_what_they_fill() {
    let wb = open(&workbooks::connected_master());
    assert_eq!(wb.connections.len(), 1);
    assert_eq!(wb.connections[0].kind, 6);
    assert_eq!(wb.query_areas.len(), 1);
    let (sheet, area) = wb.query_areas[0];
    assert_eq!(sheet, 0);
    assert_eq!(refs::area_name(&area), "A1:B4");
}

#[test]
fn macro_workbooks_keep_their_macros_untracked() {
    let wb = open(&workbooks::model_xlsm());
    assert!(wb.macro_enabled);
    assert_eq!(wb.extension(), ".xlsm");
    assert_eq!(
        wb.unsupported,
        [Unsupported {
            kind: UnsupportedKind::Macros,
            part: "xl/vbaProject.bin".to_owned()
        }]
    );
    assert_eq!(UnsupportedKind::Macros.label(), "Macros");
}

#[test]
fn rows_record_where_they_are() {
    let kit = workbooks::budget_master();
    let wb = open(&kit);
    let text = wb.sheet_text(0);
    let sheet = &wb.sheets[0];
    for row in &sheet.rows {
        let xml = row.span.get(text);
        assert!(xml.starts_with(&format!("<row r=\"{}\"", row.r)), "{xml}");
        assert!(xml.ends_with("</row>"), "{xml}");
    }
    let data = sheet.sheet_data.get(text);
    assert!(data.starts_with("<sheetData>") && data.ends_with("</sheetData>"));
}

#[test]
fn large_sheets_stream() {
    let wb = open(&workbooks::large(5000));
    assert_eq!(wb.sheets[0].rows.len(), 5001);
    assert_eq!(wb.cell_count(), 6 * 5001 + 12);
}

#[test]
fn the_panel_refuses_workbooks_past_its_limit() {
    // 6 cells a row, and 12 on the summary: 6,018 filled cells.
    let bytes = workbooks::large(1000).to_bytes();
    let read = || crate::Package::read(&bytes).expect("reads");
    assert_eq!(
        Workbook::from_package_limited(read(), 6_000),
        Err(Error::WorkbookTooLarge)
    );
    assert!(Workbook::from_package_limited(read(), 6_018).is_ok());
    assert!(Workbook::open_for_panel(&bytes).is_ok());
    assert_eq!(PANEL_CELLS, 1_500_000, "decision 0025");
}

/// A one-sheet workbook whose worksheet part is exactly `xml`.
fn probe(xml: &str) -> Vec<u8> {
    let mut wb = kit::Workbook::new("Probe");
    wb.sheets.push(kit::Sheet::new("Data", 1));
    let mut parts = wb.parts();
    for (name, bytes) in &mut parts {
        if name == "xl/worksheets/sheet1.xml" {
            *bytes = xml.as_bytes().to_vec();
        }
    }
    lineage_testkit::zip_writer::write_stored(&parts)
}

fn sheet_xml(data: &str) -> Vec<u8> {
    probe(&format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><worksheet xmlns="{}">{data}</worksheet>"#,
        kit::NS_MAIN
    ))
}

#[test]
fn damaged_sheets_are_refused() {
    let cases = [
        (
            r#"<sheetData><row r="2"/><row r="1"/></sheetData>"#,
            "rows out of order",
        ),
        (
            r#"<sheetData><row r="1"><c r="B1"/><c r="A1"/></row></sheetData>"#,
            "cells out of order",
        ),
        (
            r#"<sheetData><row r="1"><c r="A2"/></row></sheetData>"#,
            "a cell in another row",
        ),
        (
            r#"<sheetData><row r="1"><c r="A1"><v>twelve</v></c></row></sheetData>"#,
            "text in a number",
        ),
        (
            r#"<sheetData><row r="1"><c r="A1" t="q"><v>1</v></c></row></sheetData>"#,
            "an unknown type",
        ),
        (
            r#"<sheetData><row r="1048577"/></sheetData>"#,
            "past the last row",
        ),
    ];
    for (data, what) in cases {
        let err = Workbook::open(&sheet_xml(data)).expect_err(what);
        assert!(matches!(err, Error::Damaged(_)), "{what}: {err}");
    }
    let err = Workbook::open(&sheet_xml("<sheetData><row>")).expect_err("unclosed");
    assert!(matches!(err, Error::BadXml { .. }), "{err}");
}

#[test]
fn rows_and_cells_without_numbers_follow_on() {
    let wb = Workbook::open(&sheet_xml(
        r#"<sheetData><row><c><v>1</v></c><c><v>2</v></c></row><row><c t="s"><v>0</v></c></row></sheetData>"#,
    ))
    .expect("opens");
    let sheet = &wb.sheets[0];
    assert_eq!(sheet.rows.iter().map(|r| r.r).collect::<Vec<_>>(), [1, 2]);
    assert_eq!(
        sheet.rows[0]
            .cells
            .iter()
            .map(|c| c.col)
            .collect::<Vec<_>>(),
        [1, 2]
    );
}

#[test]
fn prefixed_namespaces_and_entities_are_read() {
    let wb = Workbook::open(&probe(&format!(
        r#"<?xml version="1.0"?><x:worksheet xmlns:x="{}"><x:sheetData><x:row r="1"><x:c r="A1"><x:f>IF(B1&gt;0,"&#x41;","b")</x:f><x:v>1</x:v></x:c></x:row></x:sheetData></x:worksheet>"#,
        kit::NS_MAIN
    )))
    .expect("opens");
    let text = wb.sheet_text(0);
    let Content::Formula(f) = &wb.sheets[0].rows[0].cells[0].content else {
        panic!("a formula");
    };
    assert_eq!(
        f.text.as_ref().map(|t| t.get(text)),
        Some("IF(B1>0,\"A\",\"b\")")
    );
}

#[test]
fn a_dtd_and_strict_xml_are_refused() {
    let dtd = probe(&format!(
        r#"<?xml version="1.0"?><!DOCTYPE worksheet [<!ENTITY a "aaaa">]><worksheet xmlns="{}"><sheetData/></worksheet>"#,
        kit::NS_MAIN
    ));
    assert!(matches!(Workbook::open(&dtd), Err(Error::BadXml { .. })));
    let strict = probe(
        r#"<worksheet xmlns="http://purl.oclc.org/ooxml/spreadsheetml/main"><sheetData/></worksheet>"#,
    );
    assert!(matches!(
        Workbook::open(&strict),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn a_word_document_is_not_a_workbook() {
    let docx = crate::testutil::fixture("bookmarks-crossref.docx").to_bytes();
    assert!(matches!(Workbook::open(&docx), Err(Error::NotAWorkbook(_))));
}

#[test]
fn numbers_from_other_programs_compare_by_value() {
    assert_eq!(number_key("151.3"), number_key("151.30000000000001"));
}
