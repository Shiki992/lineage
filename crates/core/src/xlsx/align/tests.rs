// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use lineage_testkit::workbooks;
use lineage_testkit::xlsx::{self as kit, Style, num, text};

use super::*;

fn open(wb: &kit::Workbook) -> Workbook {
    Workbook::open(&wb.to_bytes()).expect("opens")
}

fn excel(name: &str) -> Workbook {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/xlsx/excel")
        .join(name);
    Workbook::open(&std::fs::read(path).expect("reads")).expect("opens")
}

fn aligned(a: &Workbook, b: &Workbook, sheet: &str) -> SheetAlignment {
    align(
        a,
        a.sheet_index(sheet).expect("sheet in a"),
        b,
        b.sheet_index(sheet).expect("sheet in b"),
    )
}

#[test]
fn a_sorted_budget_matches_every_line() {
    let (a, b) = (
        open(&workbooks::budget_master()),
        open(&workbooks::budget_sorted()),
    );
    let al = aligned(&a, &b, "Transactions");
    assert_eq!(al.rows.pairs.len(), 17, "the header and 16 lines");
    assert!(al.rows.only_a.is_empty() && al.rows.only_b.is_empty());
    assert!(al.rows.unsure_a.is_empty());
    assert!(!al.rows.moved_b.is_empty());
    assert_eq!(al.sorted_by, Some((4, true)), "by Amount, largest first");
    assert!(matches!(al.key, RowKey::Composite(..)), "{:?}", al.key);
    // Each line kept its own amount.
    for (ra, rb) in &al.rows.pairs {
        let x = a.sheets[0].cell(3, *ra).map(|c| c.content.clone());
        let y = b.sheets[0].cell(3, *rb).map(|c| c.content.clone());
        let words = |wb: &Workbook, c: Option<Content>| match c {
            Some(Content::Shared(i)) => wb.shared(i).map(|s| s.text.clone()),
            _ => None,
        };
        assert_eq!(words(&a, x), words(&b, y));
    }
}

#[test]
fn a_recategorised_line_is_the_same_line() {
    let (a, b) = (
        open(&workbooks::budget_master()),
        open(&workbooks::budget_recategorised()),
    );
    let al = aligned(&a, &b, "Transactions");
    assert_eq!(al.rows.pairs.len(), 17);
    assert!(al.rows.moved_b.is_empty());
    assert_eq!(al.rows.a_to_b[&6], 6, "Lunch with Priya stays on row 6");
}

#[test]
fn added_lines_are_inserted() {
    let (a, b) = (
        open(&workbooks::budget_master()),
        open(&workbooks::budget_added()),
    );
    let al = aligned(&a, &b, "Transactions");
    assert_eq!(al.rows.only_b, [18, 19, 20]);
    assert!(al.rows.only_a.is_empty());
}

#[test]
fn the_tracker_is_keyed_by_its_ids() {
    let (a, b) = (
        open(&workbooks::tracker_master()),
        open(&workbooks::tracker_sorted()),
    );
    let al = aligned(&a, &b, "Applications");
    assert_eq!(al.key, RowKey::Column(1, "id".to_owned()));
    assert_eq!(al.rows.pairs.len(), 9);
    assert_eq!(al.sorted_by, Some((2, false)), "by company");

    let b = open(&workbooks::tracker_updated());
    let al = aligned(&a, &b, "Applications");
    assert_eq!(al.rows.pairs.len(), 9);
    assert_eq!(al.rows.only_b, [10, 11]);
    assert!(al.rows.moved_b.is_empty());
}

#[test]
fn a_filtered_sheet_hides_rows_without_moving_them() {
    let (a, b) = (
        open(&workbooks::tracker_master()),
        open(&workbooks::tracker_filtered()),
    );
    let al = aligned(&a, &b, "Applications");
    assert_eq!(al.rows.pairs.len(), 9);
    assert!(al.rows.moved_b.is_empty());
    assert!(al.rows.pairs.iter().all(|(x, y)| x == y));
}

#[test]
fn a_row_excel_inserted_moves_nothing_else() {
    let a = open(&workbooks::model_master());
    let b = excel("model-row-inserted.xlsx");
    let al = aligned(&a, &b, "Assumptions");
    assert!(matches!(al.key, RowKey::Column(1, _)), "{:?}", al.key);
    assert_eq!(al.rows.only_b, [4], "Annual discount");
    assert!(al.rows.only_a.is_empty());
    assert_eq!(al.rows.a_to_b[&4], 5, "Price per seat moved down one");
    assert_eq!(al.rows.a_to_b[&7], 8);
    assert!(al.rows.moved_b.is_empty());
    let al = aligned(&a, &b, "Revenue");
    assert_eq!(al.rows.pairs.len(), 25);
    assert!(al.rows.pairs.iter().all(|(x, y)| x == y));
}

#[test]
fn excels_own_sort_matches_every_order() {
    let a = open(&workbooks::large(1000));
    let b = excel("orders-sorted.xlsx");
    let al = aligned(&a, &b, "Orders");
    assert_eq!(al.key, RowKey::Column(1, "id".to_owned()));
    assert_eq!(al.rows.pairs.len(), 1001);
    assert_eq!(al.sorted_by.map(|s| s.0), Some(2), "by customer");
}

#[test]
fn sheets_pair_by_name_then_by_id() {
    let a = open(&workbooks::model_master());
    let b = open(&workbooks::model_harbor_lane(workbooks::model_master()));
    let pairs = pair_sheets(&a, &b);
    assert_eq!(pairs.both, [(0, 0), (1, 1), (2, 2), (3, 3)]);
    assert_eq!(pairs.only_b, [4]);
    let mut renamed = workbooks::model_master();
    renamed.sheet_mut("P&L").name = "Profit and loss".to_owned();
    let pairs = pair_sheets(&a, &open(&renamed));
    assert_eq!(pairs.both.len(), 4, "a renamed sheet keeps its id");
    assert!(pairs.only_a.is_empty());
}

/// A small table: a header and rows of (name, qty, colour).
fn table(rows: &[(&str, &str, &str)]) -> kit::Workbook {
    let mut wb = kit::Workbook::new("Stock");
    let mut sheet = kit::Sheet::new("Stock", 1);
    sheet.push_row(vec![
        Some((text("Name"), Style::Bold)),
        Some((text("Qty"), Style::Bold)),
        Some((text("Colour"), Style::Bold)),
    ]);
    for (n, q, c) in rows {
        sheet.push_row(vec![
            Some((text(*n), Style::General)),
            Some((num(*q), Style::General)),
            Some((text(*c), Style::General)),
        ]);
    }
    wb.sheets.push(sheet);
    wb
}

#[test]
fn an_edited_row_matches_its_most_alike_row() {
    let a = open(&table(&[
        ("Bolt", "10", "red"),
        ("Nut", "20", "red"),
        ("Bolt", "30", "blue"),
    ]));
    let b = open(&table(&[
        ("Bolt", "10", "green"),
        ("Nut", "20", "red"),
        ("Bolt", "30", "blue"),
    ]));
    let al = aligned(&a, &b, "Stock");
    assert_eq!(al.rows.a_to_b[&2], 2);
    assert!(al.rows.only_a.is_empty() && al.rows.unsure_a.is_empty());
}

#[test]
fn two_equally_likely_rows_are_not_guessed() {
    let a = open(&table(&[("Bolt", "10", "red"), ("Nut", "5", "grey")]));
    let b = open(&table(&[
        ("Nut", "5", "grey"),
        ("Bolt", "10", "green"),
        ("Bolt", "10", "blue"),
    ]));
    let al = aligned(&a, &b, "Stock");
    assert_eq!(al.rows.unsure_a, [2], "could not match");
    assert_eq!(al.rows.unsure_b, [3, 4]);
    assert!(!al.rows.a_to_b.contains_key(&2));
    assert!(al.rows.only_a.is_empty(), "never counted as deleted");
}

#[test]
fn an_inserted_column_moves_no_other_column() {
    let a = open(&table(&[("Bolt", "10", "red"), ("Nut", "20", "grey")]));
    let mut kb = table(&[("Bolt", "10", "red"), ("Nut", "20", "grey")]);
    for row in &mut kb.sheets[0].rows {
        for cell in &mut row.cells {
            if cell.col >= 2 {
                cell.col += 1;
            }
        }
        let value = if row.r == 1 { "Bin" } else { "A-1" };
        row.cells.insert(
            1,
            kit::Cell {
                col: 2,
                value: text(value),
                style: Style::General,
            },
        );
    }
    let b = open(&kb);
    let al = aligned(&a, &b, "Stock");
    assert_eq!(al.cols.pairs, [(1, 1), (2, 3), (3, 4)]);
    assert_eq!(al.cols.only_b, [2]);
    assert_eq!(al.rows.pairs.len(), 3);
}

#[test]
fn rows_of_formulas_pair_by_position() {
    let a = open(&workbooks::model_master());
    let b = open(&workbooks::model_price_update());
    for sheet in ["Revenue", "P&L", "Summary"] {
        let al = aligned(&a, &b, sheet);
        assert!(al.rows.pairs.iter().all(|(x, y)| x == y), "{sheet}");
        assert!(
            al.rows.only_a.is_empty() && al.rows.only_b.is_empty(),
            "{sheet}"
        );
    }
}

#[test]
fn dates_never_key_alone() {
    assert!(is_date_format("yyyy\\-mm\\-dd"));
    assert!(is_date_format("m/d/yyyy"));
    assert!(is_date_format("h:mm AM/PM"));
    assert!(!is_date_format("\"$\"#,##0"));
    assert!(!is_date_format("[Red]#,##0"));
    assert!(!is_date_format("General"));
    assert!(!is_date_format("0.00%"));
}

#[test]
fn a_large_sorted_sheet_aligns() {
    let a = open(&workbooks::large(20_000));
    let b = open(&workbooks::large_sorted(20_000));
    let al = aligned(&a, &b, "Orders");
    assert_eq!(al.rows.pairs.len(), 20_001);
    assert!(al.rows.only_a.is_empty() && al.rows.only_b.is_empty());
}
