// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use lineage_testkit::workbooks::{self, Assumptions};

use super::*;
use crate::xlsx::merge::{Manual, plan};

fn open(wb: &lineage_testkit::xlsx::Workbook) -> Workbook {
    Workbook::open(&wb.to_bytes()).expect("opens")
}

fn bring_all(p: &Plan<'_>) -> Vec<(String, Decision)> {
    p.items
        .iter()
        .filter(|i| i.status == Status::Clean)
        .map(|i| (i.id.clone(), Decision::BringIn))
        .collect()
}

#[test]
fn an_input_is_one_cell_set_with_its_lexeme() {
    let base = open(&workbooks::model_master());
    let ours = open(&workbooks::model_northwind());
    let theirs = open(&workbooks::model_price_update());
    let p = plan(&base, &ours, &theirs);
    let plan = apply_plan(&p, &bring_all(&p)).expect("a plan");
    assert_eq!(
        plan.steps,
        [Step::Set {
            sheet: "Assumptions".to_owned(),
            col: 2,
            row: 4,
            value: Write::Number("55".to_owned()),
        }]
    );
    assert_eq!(plan.expect[0].value, Write::Number("49".to_owned()));
    assert_eq!(
        plan.undo[0],
        Step::Set {
            sheet: "Assumptions".to_owned(),
            col: 2,
            row: 4,
            value: Write::Number("49".to_owned()),
        }
    );
    // What Excel holds afterwards: this version's seats plan with the
    // master's price, whatever it recalculated.
    let after = open(&workbooks::financial_model(&Assumptions {
        new_seats: 12,
        price: "55".to_owned(),
        ..Assumptions::default()
    }));
    landed(&ours, &plan, &after).expect("landed");
    // Anything else is a mismatch: say the person typed a new churn rate
    // while the plan ran.
    let typed = open(&workbooks::financial_model(&Assumptions {
        new_seats: 12,
        price: "55".to_owned(),
        churn: "0.03".to_owned(),
        ..Assumptions::default()
    }));
    let err = landed(&ours, &plan, &typed).expect_err("a mismatch");
    assert!(err.to_string().contains("assumptions!B5"), "{err}");
    put_back(&ours, &ours).expect("unchanged");
    // After the way back, the typed churn rate stays: back as it was except
    // where the person typed, and not otherwise.
    let typed_back = open(&workbooks::financial_model(&Assumptions {
        new_seats: 12,
        churn: "0.03".to_owned(),
        ..Assumptions::default()
    }));
    assert!(put_back(&ours, &typed_back).is_err());
    let edited = |at: &str| crate::xlsx::diff::Hint {
        sheet: "Assumptions".to_owned(),
        area: crate::xlsx::refs::parse_area(at).expect("area"),
        by: crate::xlsx::diff::Editor::You,
    };
    put_back_except(&ours, &typed_back, &[edited("B5")]).expect("only their typing differs");
    assert!(put_back_except(&ours, &typed_back, &[edited("B6:C9")]).is_err());
}

#[test]
fn new_rows_are_inserted_and_filled() {
    let base = open(&workbooks::tracker_master());
    let mut apps = workbooks::applications();
    apps[4].next = "Negotiate start date";
    let ours = open(&workbooks::tracker(&apps, false));
    let theirs = open(&workbooks::tracker_updated());
    let p = plan(&base, &ours, &theirs);
    let plan = apply_plan(&p, &bring_all(&p)).expect("a plan");
    assert_eq!(
        plan.steps[0],
        Step::InsertRows {
            sheet: "Applications".to_owned(),
            at: 10,
            count: 2,
        }
    );
    assert!(plan.steps.contains(&Step::Set {
        sheet: "Applications".to_owned(),
        col: 1,
        row: 10,
        value: Write::Text("APP-009".to_owned()),
    }));
    assert_eq!(
        plan.undo.last(),
        Some(&Step::DeleteRows {
            sheet: "Applications".to_owned(),
            at: 10,
            count: 2,
        })
    );
    // Excel's result: both sides' changes, the new applications last.
    let mut merged = workbooks::applications();
    merged[4].next = "Negotiate start date";
    merged[1].status = "Interview";
    merged[1].next = "Call with hiring manager";
    let updated = workbooks::tracker_updated();
    let _ = updated;
    merged.push(workbooks::Application {
        id: 9,
        company: "Lanternfish",
        role: "Data analyst",
        applied: 46042,
        status: "Applied",
        next: "",
    });
    merged.push(workbooks::Application {
        id: 10,
        company: "Greyhaven Bank",
        role: "Risk analyst",
        applied: 46043,
        status: "Applied",
        next: "Tailor cover letter",
    });
    let after = open(&workbooks::tracker(&merged, false));
    // The autoFilter range grows with the rows; its cells are what count.
    landed(&ours, &plan, &after).expect("landed");
}

#[test]
fn rows_inserted_above_move_the_cells_set_below() {
    // The master adds a hosting row (row 8) to the assumptions; this
    // version changed the opex below it? No: opex is row 7, above. Bring in
    // the row and a churn change above it.
    let base = open(&workbooks::model_master());
    let ours = open(&workbooks::model_northwind());
    let theirs = open(&workbooks::financial_model(&Assumptions {
        hosting: Some(1200),
        churn: "0.03".to_owned(),
        ..Assumptions::default()
    }));
    let p = plan(&base, &ours, &theirs);
    // The COGS formulas need the name Hosting, which comes in by hand.
    assert!(
        p.items
            .iter()
            .any(|i| i.status == Status::ByHand(Manual::Name))
    );
    let plan = apply_plan(&p, &bring_all(&p)).expect("a plan");
    assert_eq!(
        plan.steps[0],
        Step::InsertRows {
            sheet: "Assumptions".to_owned(),
            at: 8,
            count: 1,
        }
    );
    assert!(plan.steps.contains(&Step::Set {
        sheet: "Assumptions".to_owned(),
        col: 2,
        row: 5,
        value: Write::Number("0.03".to_owned()),
    }));
    assert!(plan.steps.contains(&Step::Set {
        sheet: "Assumptions".to_owned(),
        col: 2,
        row: 8,
        value: Write::Number("1200".to_owned()),
    }));
}

#[test]
fn a_formula_is_written_in_this_versions_rows() {
    // This version has an extra assumption row near the top (inserted by
    // Excel), so the master's formulas must point one row lower.
    let base = open(&workbooks::model_master());
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/xlsx/excel/model-row-inserted.xlsx");
    let ours = Workbook::open(&std::fs::read(path).expect("reads")).expect("opens");
    let mut kit = workbooks::model_master();
    // The master points the summary's seats at month 12 instead of 24.
    kit.sheet_mut("Summary").set(
        "B5",
        lineage_testkit::xlsx::formula_num("Revenue!B13", "285"),
        lineage_testkit::xlsx::Style::Thousands,
    );
    // And works out opex from the price.
    kit.sheet_mut("Assumptions").set(
        "B7",
        lineage_testkit::xlsx::formula("B4*1000", None),
        lineage_testkit::xlsx::Style::Currency,
    );
    let theirs = open(&kit);
    let p = plan(&base, &ours, &theirs);
    let plan = apply_plan(&p, &bring_all(&p)).expect("a plan");
    let sets: Vec<&Step> = plan.steps.iter().collect();
    assert!(
        sets.contains(&&Step::Set {
            sheet: "Assumptions".to_owned(),
            col: 2,
            row: 8,
            value: Write::Formula("B5*1000".to_owned()),
        }),
        "the price is on row 5 in this version: {sets:?}"
    );
    assert!(sets.contains(&&Step::Set {
        sheet: "Summary".to_owned(),
        col: 2,
        row: 5,
        value: Write::Formula("Revenue!B13".to_owned()),
    }));
}

#[test]
fn a_decision_must_be_one_of_the_choices() {
    let base = open(&workbooks::model_master());
    let ours = open(&workbooks::model_northwind());
    let theirs = open(&workbooks::model_price_update());
    let p = plan(&base, &ours, &theirs);
    let id = p.items[0].id.clone();
    assert!(apply_plan(&p, &[(id, Decision::UseMasters)]).is_err());
    assert!(apply_plan(&p, &[("nothing".to_owned(), Decision::BringIn)]).is_err());
}

#[test]
fn inserts_shift_formulas_as_excel_does() {
    let mut ins = BTreeMap::new();
    ins.insert("Data".to_owned(), vec![(5, 2)]);
    let shift = |f: &str| shift_for_inserts(&tokenize(f), "Data", &ins);
    assert_eq!(shift("A4+A5"), "A4+A7");
    assert_eq!(shift("SUM(A2:A10)"), "SUM(A2:A12)");
    assert_eq!(shift("$B$9"), "$B$11");
    assert_eq!(shift("Other!A9"), "Other!A9");
    assert_eq!(shift("Data!A9+'Data'!A1"), "Data!A11+'Data'!A1");
    assert_eq!(shift("SUM(C:C)"), "SUM(C:C)");
}
