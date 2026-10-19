// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use lineage_testkit::workbooks;

use super::*;

fn open(wb: &lineage_testkit::xlsx::Workbook) -> Workbook {
    Workbook::open(&wb.to_bytes()).expect("opens")
}

fn excel(name: &str) -> Workbook {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/xlsx/excel")
        .join(name);
    Workbook::open(&std::fs::read(path).expect("reads")).expect("opens")
}

fn cells(d: &WorkbookDiff, category: Category) -> Vec<&CellChange> {
    d.cells.iter().filter(|c| c.category == category).collect()
}

fn structure(d: &WorkbookDiff) -> Vec<String> {
    d.structure
        .iter()
        .map(|s| format!("{}: {:?}", s.sheet, s.kind))
        .collect()
}

#[test]
fn a_workbook_has_no_changes_from_itself() {
    for (name, wb) in workbooks::all() {
        let x = open(&wb);
        let d = diff(&x, &x, &[]);
        assert!(
            d.is_empty(),
            "{name}: {:?} {:?}",
            structure(&d),
            d.cells.first()
        );
    }
}

#[test]
fn a_price_change_is_one_input_and_many_knock_ons() {
    let (a, b) = (
        open(&workbooks::model_master()),
        open(&workbooks::model_price_update()),
    );
    let d = diff(&a, &b, &[]);
    let inputs = cells(&d, Category::Input);
    assert_eq!(inputs.len(), 1, "{inputs:?}");
    let price = inputs[0];
    assert_eq!(price.address(), "Assumptions!B4");
    assert_eq!(
        (price.before.shown.as_str(), price.after.shown.as_str()),
        ("$49", "$55")
    );
    assert_eq!(price.label.as_deref(), Some("Price per seat"));
    assert_eq!(
        price.provenance,
        Provenance::Unknown,
        "no event says who typed it"
    );
    assert!(cells(&d, Category::Formula).is_empty());
    // MRR, COGS and margin for 24 months, eight P&L cells, three summary cells.
    let knock_on: Vec<&Calculated> = d
        .calculated
        .iter()
        .filter(|c| c.category == Category::KnockOn)
        .collect();
    assert_eq!(knock_on.len(), 24 * 3 + 8 + 3);
    assert!(
        knock_on
            .iter()
            .all(|c| c.provenance == Provenance::Downstream)
    );
    assert!(d.structure.is_empty(), "{:?}", structure(&d));
    let summary = b.sheet_index("Summary").expect("summary");
    assert!(knock_on.iter().any(|c| c.sheet_b == summary && c.terminal));
}

#[test]
fn an_event_from_this_session_confirms_an_edit() {
    let (a, b) = (
        open(&workbooks::model_master()),
        open(&workbooks::model_price_update()),
    );
    let hint = Hint {
        sheet: "Assumptions".to_owned(),
        area: Area::cell(2, 4),
    };
    let d = diff(&a, &b, &[hint]);
    assert_eq!(
        cells(&d, Category::Input)[0].provenance,
        Provenance::DirectEdit
    );
}

#[test]
fn a_formula_filled_down_is_one_group() {
    let a = open(&workbooks::model_master());
    let b = excel("model-cogs-formula.xlsx");
    let d = diff(&a, &b, &[]);
    let formulas = cells(&d, Category::Formula);
    assert_eq!(formulas.len(), 24, "{:?}", formulas.first());
    assert!(formulas.iter().all(|f| f.group == formulas[0].group));
    assert_eq!(formulas[0].before.formula.as_deref(), Some("=C2*CogsShare"));
    assert_eq!(
        formulas[0].after.formula.as_deref(),
        Some("=C2*CogsShare+250")
    );
    assert!(cells(&d, Category::Input).is_empty());
    assert!(
        d.calculated
            .iter()
            .filter(|c| c.category == Category::KnockOn)
            .count()
            >= 24
    );
}

#[test]
fn a_row_excel_inserted_changes_no_formula() {
    let a = open(&workbooks::model_master());
    let b = excel("model-row-inserted.xlsx");
    let d = diff(&a, &b, &[]);
    assert!(
        cells(&d, Category::Formula).is_empty(),
        "{:?}",
        cells(&d, Category::Formula).first()
    );
    assert!(
        cells(&d, Category::Input).is_empty(),
        "{:?}",
        cells(&d, Category::Input).first()
    );
    let s = structure(&d);
    assert!(
        s.contains(&"Assumptions: RowsAdded { rows: [4] }".to_owned()),
        "{s:?}"
    );
    assert!(
        !s.iter().any(|x| x.contains("Name")),
        "names moved with their rows: {s:?}"
    );
}

#[test]
fn a_hosting_line_is_a_new_row_a_new_name_and_a_formula_change() {
    let (a, b) = (
        open(&workbooks::model_master()),
        open(&workbooks::model_hosting()),
    );
    let d = diff(&a, &b, &[]);
    assert_eq!(cells(&d, Category::Formula).len(), 24);
    let s = structure(&d);
    assert!(
        s.contains(&"Assumptions: RowsAdded { rows: [8] }".to_owned()),
        "{s:?}"
    );
    assert!(
        s.contains(&": NameAdded { name: \"Hosting\" }".to_owned()),
        "{s:?}"
    );
}

#[test]
fn a_sorted_budget_is_one_structure_change() {
    let (a, b) = (
        open(&workbooks::budget_master()),
        open(&workbooks::budget_sorted()),
    );
    let d = diff(&a, &b, &[]);
    assert!(d.cells.is_empty(), "{:?}", d.cells.first());
    assert!(d.calculated.is_empty());
    assert_eq!(
        structure(&d),
        ["Transactions: RowsSorted { by: \"Amount\", descending: true }"]
    );
    let b = excel("budget-sorted.xlsx");
    let d = diff(&a, &b, &[]);
    assert!(
        cells(&d, Category::Input).is_empty(),
        "Excel's sort: {:?}",
        d.cells.first()
    );
}

#[test]
fn a_recategorised_line_moves_two_totals() {
    let (a, b) = (
        open(&workbooks::budget_master()),
        open(&workbooks::budget_recategorised()),
    );
    let d = diff(&a, &b, &[]);
    let inputs = cells(&d, Category::Input);
    assert_eq!(inputs.len(), 1);
    assert_eq!(
        (
            inputs[0].before.shown.as_str(),
            inputs[0].after.shown.as_str()
        ),
        ("Eating out", "Groceries")
    );
    assert_eq!(inputs[0].header.as_deref(), Some("Category"));
    let moved: Vec<u32> = d.calculated.iter().map(|c| c.row).collect();
    assert_eq!(moved.len(), 2, "Groceries and Eating out; the total stays");
    assert!(
        d.calculated
            .iter()
            .all(|c| c.provenance == Provenance::Downstream)
    );
}

#[test]
fn filtering_hides_rows_and_changes_nothing_else() {
    let (a, b) = (
        open(&workbooks::tracker_master()),
        open(&workbooks::tracker_filtered()),
    );
    let d = diff(&a, &b, &[]);
    assert!(d.cells.is_empty());
    assert_eq!(
        structure(&d),
        [
            "Applications: RowsHidden { rows: [4] }",
            "Applications: RowsHidden { rows: [7] }"
        ]
    );
}

#[test]
fn refreshed_data_is_never_an_edit() {
    let (a, b) = (
        open(&workbooks::connected_master()),
        open(&workbooks::connected_refreshed()),
    );
    let d = diff(&a, &b, &[]);
    assert!(
        cells(&d, Category::Input).is_empty(),
        "{:?}",
        cells(&d, Category::Input)
    );
    let refreshed = cells(&d, Category::Refreshed);
    assert_eq!(refreshed.len(), 3, "three rates");
    assert!(
        refreshed
            .iter()
            .all(|c| c.provenance == Provenance::Refresh)
    );
    // The converted prices moved because of the refresh.
    assert!(!d.calculated.is_empty());
    assert!(
        d.calculated
            .iter()
            .all(|c| c.category == Category::Refreshed)
    );
}

#[test]
fn a_tracker_update_adds_rows_and_changes_a_status() {
    let (a, b) = (
        open(&workbooks::tracker_master()),
        open(&workbooks::tracker_updated()),
    );
    let d = diff(&a, &b, &[]);
    let inputs = cells(&d, Category::Input);
    assert_eq!(inputs.len(), 2, "status and next step: {inputs:?}");
    assert!(structure(&d).contains(&"Applications: RowsAdded { rows: [10, 11] }".to_owned()));
    assert!(
        d.calculated
            .iter()
            .any(|c| c.provenance == Provenance::Downstream),
        "counts moved"
    );
}

#[test]
fn excels_own_save_shows_no_edits() {
    for (ours, theirs) in [
        ("model", "model-saved.xlsx"),
        ("budget", "budget-saved.xlsx"),
        ("awkward", "awkward-saved.xlsx"),
    ] {
        let a = match ours {
            "model" => open(&workbooks::model_master()),
            "budget" => open(&workbooks::budget_master()),
            _ => open(&workbooks::awkward_cells()),
        };
        let b = excel(theirs);
        let d = diff(&a, &b, &[]);
        assert!(
            cells(&d, Category::Input).is_empty(),
            "{theirs}: {:?}",
            cells(&d, Category::Input)
        );
        assert!(
            cells(&d, Category::Formula).is_empty(),
            "{theirs}: {:?}",
            cells(&d, Category::Formula)
        );
        // TODAY() moved on: refreshed, never knock-on or an edit.
        assert!(
            d.calculated
                .iter()
                .all(|c| c.category == Category::Refreshed),
            "{theirs}: {:?}",
            d.calculated
        );
    }
}

#[test]
fn excels_edits_show_exactly_what_was_done() {
    let cases: [(Workbook, &str, &[&str]); 5] = [
        (open(&workbooks::model_master()), "model-saved.xlsx", &[]),
        (open(&workbooks::budget_master()), "budget-saved.xlsx", &[]),
        (
            open(&workbooks::budget_master()),
            "budget-sorted.xlsx",
            &["Transactions: RowsSorted { by: \"Amount\", descending: true }"],
        ),
        (
            open(&workbooks::tracker_master()),
            "tracker-filtered.xlsx",
            &[
                "Applications: RowsHidden { rows: [4] }",
                "Applications: RowsHidden { rows: [7] }",
                "Applications: Filter",
            ],
        ),
        (
            open(&workbooks::budget_master()),
            "budget-row-added.xlsx",
            &["Transactions: RowsAdded { rows: [18] }"],
        ),
    ];
    for (a, name, expected) in cases {
        let d = diff(&a, &excel(name), &[]);
        assert_eq!(structure(&d), expected, "{name}");
        assert!(d.cells.is_empty(), "{name}: {:?}", d.cells.first());
    }
}

#[test]
fn sorting_rows_under_a_range_changes_no_formula() {
    // The summary's SUMIF ranges keep their text when the rows are sorted
    // (found in the Phase 5 measurements).
    let (a, b) = (
        open(&workbooks::large(2000)),
        open(&workbooks::large_sorted(2000)),
    );
    let d = diff(&a, &b, &[]);
    assert!(d.cells.is_empty(), "{:?}", d.cells.first());
    assert!(d.calculated.is_empty(), "the totals by region are the same");
    assert_eq!(d.structure.len(), 1, "the rows moved");
}
