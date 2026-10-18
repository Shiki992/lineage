// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Property tests for the workbook engine's invariants:
//!
//! - any formula text tokenizes and renders back exactly;
//! - shifting a formula and shifting it back gives the original, when no
//!   reference falls off the sheet;
//! - the reader never panics on damaged worksheet XML;
//! - re-sorting a sheet, and sorting it back, gives no input or formula
//!   changes when its records can be matched;
//! - a formula change and a cached-result change are classified
//!   independently.

use lineage_core::Workbook;
use lineage_core::xlsx::diff::{Category, diff};
use lineage_core::xlsx::formula::tokenize;
use lineage_testkit::xlsx::{self, Cached, Style, Value, num, text};
use proptest::prelude::*;

/// A stock sheet: ID, Name, Qty, Price and a Total formula per row, in the
/// order given. `keyed` leaves the ID column out when false.
fn stock(rows: &[(u32, u32, u32)], keyed: bool, formula_of: impl Fn(u32) -> String) -> Workbook {
    let mut wb = xlsx::Workbook::new("Stock");
    let mut sheet = xlsx::Sheet::new("Stock", 1);
    let mut header = vec![];
    if keyed {
        header.push(Some((text("ID"), Style::Bold)));
    }
    for h in ["Name", "Qty", "Price", "Total"] {
        header.push(Some((text(h), Style::Bold)));
    }
    sheet.push_row(header);
    let first = if keyed { 2 } else { 1 };
    let col = |n: u32| xlsx::col_name(first + n);
    for (i, (id, qty, price)) in rows.iter().enumerate() {
        let r = u32::try_from(i).unwrap_or(0) + 2;
        let mut cells = vec![];
        if keyed {
            cells.push(Some((text(format!("SKU-{id:04}")), Style::General)));
        }
        cells.push(Some((text(format!("Part {id}")), Style::General)));
        cells.push(Some((num(qty.to_string()), Style::General)));
        cells.push(Some((num(price.to_string()), Style::General)));
        let f = formula_of(r)
            .replace("{q}", &format!("{}{r}", col(1)))
            .replace("{p}", &format!("{}{r}", col(2)));
        cells.push(Some((
            Value::Formula {
                f,
                cached: Some(Cached::Number((qty * price).to_string())),
            },
            Style::General,
        )));
        sheet.push_row(cells);
    }
    wb.sheets.push(sheet);
    Workbook::open(&wb.to_bytes()).expect("opens")
}

fn records() -> impl Strategy<Value = Vec<(u32, u32, u32)>> {
    prop::collection::btree_set(1u32..5000, 2..25).prop_flat_map(|ids| {
        let ids: Vec<u32> = ids.into_iter().collect();
        let n = ids.len();
        (Just(ids), prop::collection::vec((1u32..50, 1u32..900), n)).prop_map(|(ids, vals)| {
            ids.into_iter()
                .zip(vals)
                .map(|(id, (q, p))| (id, q, p))
                .collect()
        })
    })
}

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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn re_sorting_and_sorting_back_changes_no_input(rows in records(), keyed in any::<bool>(), seed in any::<u64>()) {
        let mut shuffled = rows.clone();
        // A deterministic shuffle from the seed.
        let mut state = seed | 1;
        for i in (1..shuffled.len()).rev() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let j = usize::try_from(state % (i as u64 + 1)).unwrap_or(0);
            shuffled.swap(i, j);
        }
        let total = |_r: u32| "{q}*{p}".to_owned();
        let a = stock(&rows, keyed, total);
        let b = stock(&shuffled, keyed, total);
        for (x, y) in [(&a, &b), (&b, &a)] {
            let d = diff(x, y, &[]);
            let edits: Vec<_> = d
                .cells
                .iter()
                .filter(|c| matches!(c.category, Category::Input | Category::Formula))
                .collect();
            prop_assert!(edits.is_empty(), "{:?}", edits.first());
            prop_assert!(d.calculated.is_empty());
            prop_assert!(d.unmatched.is_empty());
        }
        // Sorted back: nothing at all.
        let back = stock(&rows, keyed, total);
        prop_assert!(diff(&a, &back, &[]).is_empty());
    }

    #[test]
    fn formulas_and_results_are_classified_apart(rows in records(), which in 0usize..25, bump in 1u32..9) {
        let which = which % rows.len();
        let total = |_r: u32| "{q}*{p}".to_owned();
        let a = stock(&rows, true, total);
        // The formula changes; its stored result doesn't.
        let target = u32::try_from(which).unwrap_or(0) + 2;
        let b = stock(&rows, true, |r| if r == target { "{q}*{p}+0".to_owned() } else { "{q}*{p}".to_owned() });
        let d = diff(&a, &b, &[]);
        prop_assert_eq!(d.cells.iter().filter(|c| c.category == Category::Formula).count(), 1);
        prop_assert!(d.calculated.is_empty());
        // An input changes: its total's stored result moves, the formula doesn't.
        let mut changed = rows.clone();
        changed[which].1 += bump;
        let c = stock(&changed, true, total);
        let d = diff(&a, &c, &[]);
        prop_assert_eq!(d.cells.iter().filter(|c| c.category == Category::Input).count(), 1);
        prop_assert_eq!(d.cells.iter().filter(|c| c.category == Category::Formula).count(), 0);
        prop_assert_eq!(d.calculated.len(), 1);
        prop_assert_eq!(d.calculated[0].category, Category::KnockOn);
    }
}
