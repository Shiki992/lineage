// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Property tests for the workbook engine's invariants:
//!
//! - any formula text tokenizes and renders back exactly;
//! - shifting a formula and shifting it back gives the original, when no
//!   reference falls off the sheet;
//! - the reader never panics on damaged worksheet XML.

use lineage_core::Workbook;
use lineage_core::xlsx::formula::tokenize;
use lineage_testkit::xlsx;
use proptest::prelude::*;

/// Pieces formulas are made of, so random formulas look like real ones.
const PIECES: &[&str] = &[
    "A1",
    "$B$2",
    "C$3",
    "$D4",
    "AA10",
    "XFD1",
    "B2:C9",
    "A:A",
    "$C:$D",
    "3:5",
    "Sheet1!",
    "'P&L'!",
    "'Bob''s'!",
    "Jan:Mar!",
    "[1]",
    "[1]Prices!",
    "SUM(",
    "_xlfn.XLOOKUP(",
    "LOG10(",
    ")",
    ",",
    "+",
    "-",
    "*",
    "/",
    "^",
    "&",
    "=",
    "<>",
    " ",
    "\"text\"",
    "\"a\"\"b\"",
    "1.5",
    "1E+5",
    ".5",
    "TRUE",
    "#REF!",
    "#N/A",
    "{1,2;3,4}",
    "Price",
    "Table1[Amount]",
    "[@Qty]",
    "A1#",
    "%",
    "!",
    "'",
    "[",
    "]",
    ":",
    "$",
    "é",
    "€",
];

fn formula_text() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(PIECES), 0..12).prop_map(|p| p.concat())
}

proptest! {
    #[test]
    fn any_formula_renders_back(text in formula_text()) {
        prop_assert_eq!(tokenize(&text).render(), text);
    }

    #[test]
    fn any_text_renders_back(text in ".{0,40}") {
        prop_assert_eq!(tokenize(&text).render(), text);
    }

    #[test]
    fn shifting_there_and_back_is_the_same(text in formula_text(), rows in 0i64..50, cols in 0i64..20) {
        let f = tokenize(&text);
        let moved = f.shifted(rows, cols);
        // Only when nothing fell off the sheet on the way.
        let refs = |s: &str| s.matches("#REF!").count();
        if refs(&moved.render()) == refs(&text) {
            let back = moved.shifted(-rows, -cols);
            if refs(&back.render()) == refs(&text) {
                // Moved references are written upper-case, as Excel writes them.
                prop_assert_eq!(back.render().to_ascii_uppercase(), text.to_ascii_uppercase());
            }
        }
    }

    #[test]
    fn damaged_sheets_never_panic(body in prop::collection::vec(prop::sample::select(&[
        "<sheetData>", "</sheetData>", "<row r=\"1\">", "<row>", "</row>", "<row r=\"2\"/>",
        "<c r=\"A1\">", "<c>", "<c t=\"s\">", "<c t=\"b\">", "<c t=\"inlineStr\">", "</c>",
        "<v>1</v>", "<v>x</v>", "<v>&amp;</v>", "<f>SUM(A1)</f>", "<f t=\"shared\" si=\"0\"/>",
        "<is><t>a</t></is>", "<mergeCells><mergeCell ref=\"A1:B2\"/></mergeCells>",
        "<cols><col min=\"1\" max=\"2\"/></cols>", "<extLst/>", "&#65;", "&bogus;", "<![CDATA[x]]>",
    ][..]), 0..20)) {
        let xml = format!(
            r#"<worksheet xmlns="{}">{}</worksheet>"#,
            xlsx::NS_MAIN,
            body.concat()
        );
        let mut wb = xlsx::Workbook::new("Probe");
        wb.sheets.push(xlsx::Sheet::new("Data", 1));
        let mut parts = wb.parts();
        for (name, bytes) in &mut parts {
            if name == "xl/worksheets/sheet1.xml" {
                *bytes = xml.clone().into_bytes();
            }
        }
        let _ = Workbook::open(&lineage_testkit::zip_writer::write_stored(&parts));
    }
}
