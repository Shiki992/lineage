// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_testkit::workbooks::{self, Assumptions};

use super::*;

fn open(wb: &lineage_testkit::xlsx::Workbook) -> Workbook {
    Workbook::open(&wb.to_bytes()).expect("opens")
}

fn model(a: Assumptions) -> Workbook {
    open(&workbooks::financial_model(&a))
}

fn cells(p: &Plan<'_>) -> Vec<(String, Status)> {
    p.items
        .iter()
        .filter_map(|i| match &i.what {
            What::Cell(c) => Some((c.address(), i.status)),
            _ => None,
        })
        .collect()
}

#[test]
fn a_change_to_another_input_comes_in_by_itself() {
    let base = open(&workbooks::model_master());
    let ours = open(&workbooks::model_northwind());
    let theirs = open(&workbooks::model_price_update());
    let p = plan(&base, &ours, &theirs);
    assert_eq!(cells(&p), [("Assumptions!B4".to_owned(), Status::Clean)]);
    assert_eq!(p.items[0].choices(), [Decision::BringIn, Decision::Decline]);
}

#[test]
fn both_changing_one_cell_is_an_overlap() {
    let base = open(&workbooks::model_master());
    let ours = open(&workbooks::model_northwind());
    let theirs = model(Assumptions {
        new_seats: 18,
        ..Assumptions::default()
    });
    let p = plan(&base, &ours, &theirs);
    assert_eq!(
        cells(&p),
        [(
            "Assumptions!B3".to_owned(),
            Status::Overlap(Conflict::BothChanged)
        )]
    );
    let item = &p.items[0];
    assert_eq!(
        item.mine.as_ref().map(|m| m.after.shown.as_str()),
        Some("12")
    );
    assert_eq!(item.choices(), [Decision::KeepMine, Decision::UseMasters]);
}

#[test]
fn the_same_change_on_both_sides_is_already_there() {
    let base = open(&workbooks::model_master());
    let ours = open(&workbooks::model_price_update());
    let theirs = open(&workbooks::model_price_update());
    let p = plan(&base, &ours, &theirs);
    assert_eq!(
        cells(&p),
        [("Assumptions!B4".to_owned(), Status::AlreadyThere)]
    );
    assert!(p.items[0].choices().is_empty());
}

#[test]
fn a_formula_whose_inputs_this_version_changed_is_an_overlap() {
    // The master adds a fixed 250 to the COGS formula (in Excel, filled
    // down); this version changed the COGS share that formula reads.
    let base = open(&workbooks::model_master());
    let ours = model(Assumptions {
        cogs_share: "0.2".to_owned(),
        ..Assumptions::default()
    });
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/xlsx/excel/model-cogs-formula.xlsx");
    let theirs = Workbook::open(&std::fs::read(path).expect("reads")).expect("opens");
    let p = plan(&base, &ours, &theirs);
    let formulas: Vec<Status> = p
        .items
        .iter()
        .filter(|i| matches!(&i.what, What::Cell(c) if c.category == Category::Formula))
        .map(|i| i.status)
        .collect();
    assert_eq!(formulas.len(), 24);
    assert!(
        formulas
            .iter()
            .all(|s| *s == Status::Overlap(Conflict::FormulaAndInputs)),
        "{formulas:?}"
    );
}

#[test]
fn a_formula_needing_a_name_this_version_lacks_is_by_hand() {
    // The master's COGS formula uses the new name Hosting; names come in by
    // hand, so the formulas do too, and the new Hosting row by itself.
    let base = open(&workbooks::model_master());
    let ours = open(&workbooks::model_northwind());
    let theirs = open(&workbooks::model_hosting());
    let p = plan(&base, &ours, &theirs);
    let formulas: Vec<Status> = p
        .items
        .iter()
        .filter(|i| matches!(&i.what, What::Cell(c) if c.category == Category::Formula))
        .map(|i| i.status)
        .collect();
    assert_eq!(formulas.len(), 24);
    assert!(formulas.iter().all(|s| *s == Status::ByHand(Manual::Name)));
    assert!(
        p.items
            .iter()
            .any(|i| matches!(i.what, What::Rows { .. }) && i.status == Status::Clean)
    );
}

#[test]
fn an_input_a_changed_formula_of_mine_reads_is_an_overlap() {
    let base = open(&workbooks::model_master());
    // This version changed every COGS formula; the master changes the COGS
    // share they read.
    let ours = open(&workbooks::model_hosting());
    let theirs = model(Assumptions {
        cogs_share: "0.2".to_owned(),
        ..Assumptions::default()
    });
    let p = plan(&base, &ours, &theirs);
    assert_eq!(
        cells(&p),
        [(
            "Assumptions!B6".to_owned(),
            Status::Overlap(Conflict::FormulaAndInputs)
        )]
    );
}

#[test]
fn rows_the_master_added_come_in_and_edits_beside_them_too() {
    let base = open(&workbooks::tracker_master());
    let mut apps = workbooks::applications();
    apps[4].next = "Negotiate start date";
    let ours = open(&workbooks::tracker(&apps, false));
    let theirs = open(&workbooks::tracker_updated());
    let p = plan(&base, &ours, &theirs);
    let rows: Vec<&Item> = p
        .items
        .iter()
        .filter(|i| matches!(i.what, What::Rows { .. }))
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, Status::Clean);
    assert!(
        cells(&p).iter().all(|(_, s)| *s == Status::Clean),
        "{:?}",
        cells(&p)
    );
}

#[test]
fn a_change_to_a_row_this_version_deleted_has_no_place() {
    let base = open(&workbooks::tracker_master());
    let apps: Vec<_> = workbooks::applications()
        .into_iter()
        .filter(|a| a.id != 2)
        .collect();
    let ours = open(&workbooks::tracker(&apps, false));
    let theirs = open(&workbooks::tracker_updated());
    let p = plan(&base, &ours, &theirs);
    let deleted: Vec<_> = cells(&p)
        .into_iter()
        .filter(|(_, s)| *s == Status::Overlap(Conflict::EditedVsDeleted))
        .collect();
    assert_eq!(
        deleted.len(),
        2,
        "application 2's status and next step: {deleted:?}"
    );
    let item = p
        .items
        .iter()
        .find(|i| i.status == Status::Overlap(Conflict::EditedVsDeleted))
        .expect("an item");
    assert_eq!(item.choices(), [Decision::KeepMine]);
}

#[test]
fn sorting_and_removing_rows_are_by_hand() {
    let base = open(&workbooks::budget_master());
    let ours = open(&workbooks::budget_master());
    let sorted = open(&workbooks::budget_sorted());
    let p = plan(&base, &ours, &sorted);
    assert_eq!(p.items.len(), 1);
    assert_eq!(p.items[0].status, Status::ByHand(Manual::RowsMoved));
    let fewer: Vec<_> = workbooks::budget_transactions()
        .into_iter()
        .skip(1)
        .collect();
    let fewer = open(&workbooks::budget(&fewer));
    let p = plan(&base, &ours, &fewer);
    assert!(
        p.items
            .iter()
            .any(|i| i.status == Status::ByHand(Manual::RowsRemoved))
    );
}

#[test]
fn rows_added_at_the_same_place_on_both_sides_overlap() {
    let base = open(&workbooks::budget_master());
    let mut mine = workbooks::budget_transactions();
    mine.push(workbooks::Txn {
        date: 46080,
        category: "Savings",
        description: "Holiday fund",
        amount: "150",
    });
    let ours = open(&workbooks::budget(&mine));
    let theirs = open(&workbooks::budget_added());
    let p = plan(&base, &ours, &theirs);
    let rows: Vec<&Item> = p
        .items
        .iter()
        .filter(|i| matches!(i.what, What::Rows { .. }))
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, Status::Overlap(Conflict::BothAdded));
    assert_eq!(rows[0].choices().len(), 3);
}
