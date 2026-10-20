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

// --- Phase 5B: macros, pivot tables and Power Query, saved by Excel --------
// (scripts/excel-resave-5b.ps1; decisions 0028 to 0030)

mod phase_5b {
    use lineage_core::Workbook;
    use lineage_core::xlsx::diff::{Category, diff};
    use lineage_core::xlsx::model::UnsupportedKind;
    use lineage_core::xlsx::pivot::PivotChangeKind;
    use lineage_testkit::advanced;

    use super::fixture;

    fn open(wb: &lineage_testkit::xlsx::Workbook) -> Workbook {
        Workbook::open(&wb.to_bytes()).expect("opens")
    }

    fn untracked(wb: &Workbook) -> Vec<UnsupportedKind> {
        wb.unsupported.iter().map(|u| u.kind).collect()
    }

    /// The VBA spike: Excel accepts a project in the spec's on-write form
    /// and keeps every module's code exactly.
    #[test]
    fn excel_keeps_every_macro_line() {
        let ours = open(&advanced::model_macros_only());
        let saved = fixture("excel", "model-macros-saved.xlsm");
        let (a, b) = (
            ours.vba.as_ref().expect("ours"),
            saved.vba.as_ref().expect("Excel's reads"),
        );
        let names = |p: &lineage_core::vba::VbaProject| {
            let mut v: Vec<String> = p.modules.iter().map(|m| m.name.clone()).collect();
            v.sort();
            v
        };
        assert_eq!(names(a), names(b));
        for m in &a.modules {
            let n = b.module(&m.name).expect("module");
            assert_eq!(m.lines, n.lines, "{}", m.name);
            assert_eq!(m.kind, n.kind, "{}", m.name);
        }
        assert!(
            untracked(&saved)
                .iter()
                .all(|k| *k != UnsupportedKind::Macros)
        );
        let d = lineage_core::vba::diff::diff(Some(a), Some(b));
        assert!(d.modules.is_empty(), "{:?}", d.modules);
    }

    #[test]
    fn excel_keeps_the_pivots_slicer_and_queries() {
        for name in ["model-pivots-saved.xlsx", "model-full-saved.xlsm"] {
            let wb = fixture("excel", name);
            assert!(
                wb.pivots.unreadable.is_empty(),
                "{name}: {:?}",
                wb.pivots.unreadable
            );
            assert_eq!(wb.pivots.tables.len(), 2, "{name}");
            assert_eq!(wb.pivots.slicers.len(), 1, "{name}");
            assert_eq!(wb.pivots.slicers[0].caption, "Region", "{name}");
        }
        for name in ["model-queries-saved.xlsx", "model-full-saved.xlsm"] {
            let wb = fixture("excel", name);
            let q = wb
                .queries
                .as_ref()
                .unwrap_or_else(|| panic!("{name}: no queries"));
            let names: Vec<&str> = q.queries.iter().map(|q| q.name.as_str()).collect();
            assert_eq!(names, ["Rates", "RegionList"], "{name}");
        }
    }

    #[test]
    fn a_plain_save_by_excel_shows_no_changes() {
        let ours = open(&advanced::model_full_for_excel());
        let saved = fixture("excel", "model-full-saved.xlsm");
        let d = diff(&ours, &saved, &[]);
        assert!(d.macros.is_empty(), "{:?}", d.macros);
        assert!(d.queries.is_empty(), "{:?}", d.queries);
        assert!(
            d.pivots
                .iter()
                .all(|p| p.kind == PivotChangeKind::Refreshed)
                && d.slicers.is_empty(),
            "{:?} {:?}",
            d.pivots,
            d.slicers
        );
        assert!(
            d.cells.iter().all(|c| c.category != Category::Input),
            "{:?}",
            d.cells.iter().map(|c| c.address()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn excels_own_pivot_edits() {
        let base = fixture("excel", "model-full-saved.xlsm");
        let rows = fixture("excel", "model-full-pivot-rows-excel.xlsm");
        let d = diff(&base, &rows, &[]);
        let changed: Vec<&str> = d
            .pivots
            .iter()
            .filter(|p| p.kind == PivotChangeKind::Changed)
            .flat_map(|p| p.details.iter().map(String::as_str))
            .collect();
        assert!(
            changed.contains(&"Rows: Month (was Plan)"),
            "{:?}",
            d.pivots
        );

        let refreshed = fixture("excel", "model-full-refreshed-excel.xlsm");
        let d = diff(&base, &refreshed, &[]);
        let inputs: Vec<String> = d
            .cells
            .iter()
            .filter(|c| c.category == Category::Input)
            .map(|c| c.address())
            .collect();
        assert_eq!(inputs, ["Sales!E13"], "only the typed sale is an input");
        assert!(
            d.pivots
                .iter()
                .any(|p| p.kind == PivotChangeKind::Refreshed)
        );

        // As in the Excel run: the generated file, then Excel's refresh. Excel
        // re-formats the pivot's cells and fits its columns; none of that is
        // someone's formatting.
        let ours = open(&advanced::model_full_for_excel());
        let d = diff(&ours, &refreshed, &[]);
        let on_pivots: Vec<String> = d
            .structure
            .iter()
            .filter(|s| s.sheet == "Pivots")
            .map(|s| format!("{:?}", s.kind))
            .collect();
        assert!(on_pivots.is_empty(), "{on_pivots:?}");
        assert!(
            d.cells
                .iter()
                .all(|c| c.sheet != "Pivots" || c.category == Category::Refreshed),
            "only refreshed values on Pivots"
        );
        for p in &d.pivots {
            let refreshed_cells = d
                .cells
                .iter()
                .filter(|c| c.sheet == "Pivots" && c.category == Category::Refreshed)
                .count();
            assert!(p.cells <= refreshed_cells, "{p:?}");
        }

        let slicer = fixture("excel", "model-full-slicer-excel.xlsm");
        let d = diff(&base, &slicer, &[]);
        assert!(
            d.slicers
                .iter()
                .any(|s| s.summary == "Slicer Region: shows North, West (was all)"),
            "{:?}",
            d.slicers
        );
    }
}
