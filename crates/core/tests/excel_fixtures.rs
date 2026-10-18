// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Workbooks saved and edited by real Excel (`scripts/excel-resave.ps1`,
//! fixtures/xlsx/excel/). They check that what Excel writes is read the way
//! the synthetic workbooks are: the same sheets, rows and cells, with
//! numbers compared by the double they stand for.

use std::collections::BTreeMap;
use std::path::Path;

use lineage_core::Workbook;
use lineage_core::xlsx::refs::{a1, number_key};
use lineage_core::xlsx::{Cached, Content};

fn fixture(dir: &str, name: &str) -> Workbook {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/xlsx")
        .join(dir)
        .join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    Workbook::open(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Every cell as "Sheet!A1" → what it holds, in words a test can compare.
fn cells(wb: &Workbook) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (index, sheet) in wb.sheets.iter().enumerate() {
        let text = wb.sheet_text(index);
        for row in &sheet.rows {
            for cell in &row.cells {
                let value = match &cell.content {
                    Content::Blank => continue,
                    Content::Shared(i) => {
                        format!("text {}", wb.shared(*i).map_or("?", |s| s.text.as_str()))
                    }
                    Content::Inline(t) => format!("text {t}"),
                    Content::Number(s) => format!("number {:?}", number_key(s.get(text))),
                    Content::Bool(b) => format!("bool {b}"),
                    Content::Error(e) => format!("error {}", e.get(text)),
                    Content::IsoDate(s) => format!("date {}", s.get(text)),
                    Content::Formula(f) => {
                        let cached = match &f.cached {
                            // Excel shows 15 significant digits, and its own
                            // recalculation can land a bit away from exact
                            // decimal arithmetic.
                            Some(Cached::Number(n)) => shown(n.get(text)),
                            Some(Cached::Text(t)) => t.get(text).to_owned(),
                            Some(Cached::Bool(b)) => b.to_string(),
                            Some(Cached::Error(e)) => e.get(text).to_owned(),
                            None => String::new(),
                        };
                        format!("formula = {cached}")
                    }
                };
                out.insert(format!("{}!{}", sheet.name, a1(cell.col, row.r)), value);
            }
        }
    }
    out
}

/// A calculated number as Excel shows it: 15 significant digits.
fn shown(lexeme: &str) -> String {
    let value: f64 = lexeme.parse().unwrap_or(f64::NAN);
    format!("{value:.14e}")
}

#[test]
fn every_excel_fixture_opens() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/xlsx/excel");
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).expect("the folder") {
        let path = entry.expect("an entry").path();
        if path.extension().is_some_and(|e| e == "xlsx") {
            let bytes = std::fs::read(&path).expect("reads");
            Workbook::open(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            count += 1;
        }
    }
    assert!(count >= 12, "{count}");
}

#[test]
fn a_workbook_saved_by_excel_holds_the_same_cells() {
    for (synthetic, saved) in [
        ("model.xlsx", "model-saved.xlsx"),
        ("budget.xlsx", "budget-saved.xlsx"),
        ("awkward-cells.xlsx", "awkward-saved.xlsx"),
        ("rates.xlsx", "rates-saved.xlsx"),
    ] {
        let ours = cells(&fixture("synthetic", synthetic));
        let excel = cells(&fixture("excel", saved));
        let mut differ = Vec::new();
        for (at, value) in &ours {
            // TODAY() moved on when Excel recalculated.
            if at == "Summary!B6" && synthetic == "model.xlsx" {
                continue;
            }
            match excel.get(at) {
                Some(v) if v == value => {}
                other => differ.push(format!("{at}: {value} / {other:?}")),
            }
        }
        for at in excel.keys().filter(|at| !ours.contains_key(*at)) {
            differ.push(format!("{at}: only in Excel's"));
        }
        assert!(differ.is_empty(), "{saved}:\n{}", differ.join("\n"));
    }
}

#[test]
fn excel_keeps_what_the_engine_relies_on() {
    let wb = fixture("excel", "model-saved.xlsx");
    let names: Vec<&str> = wb.names.iter().map(|n| n.name.as_str()).collect();
    assert_eq!(
        names,
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
    assert_eq!(wb.sheets[revenue].rows.len(), 25);
    let wb = fixture("excel", "tracker-filtered.xlsx");
    let hidden: Vec<u32> = wb.sheets[0]
        .rows
        .iter()
        .filter(|r| r.hidden)
        .map(|r| r.r)
        .collect();
    assert_eq!(hidden, [4, 7], "Excel hides the rejected applications");
    let wb = fixture("excel", "rates-saved.xlsx");
    assert_eq!(wb.query_areas.len(), 1);
}
