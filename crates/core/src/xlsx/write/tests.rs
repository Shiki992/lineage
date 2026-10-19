// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

use lineage_testkit::workbooks;

use super::*;
use crate::xlsx::apply::put_back;
use crate::xlsx::value;

fn excel(name: &str) -> Workbook {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/xlsx/excel")
        .join(name);
    Workbook::open(&std::fs::read(path).expect("reads")).expect("opens")
}

fn set(sheet: &str, col: u32, row: u32, value: Write) -> Step {
    Step::Set {
        sheet: sheet.to_owned(),
        col,
        row,
        value,
    }
}

fn shown(wb: &Workbook, sheet: &str, at: &str) -> String {
    let i = wb.sheet_index(sheet).expect("sheet");
    let (c, r) = crate::xlsx::refs::parse_cell(at).expect("cell");
    wb.sheets[i]
        .cell(c, r)
        .map(|cell| value::either(wb, wb.sheet_text(i), cell).text())
        .unwrap_or_default()
}

#[test]
fn an_inserted_row_matches_excels_own() {
    // Excel inserted a row above Price per seat and typed into it
    // (fixtures/xlsx/excel/model-row-inserted.xlsx). The writer does the
    // same to the synthetic model: every input and formula must agree.
    let bytes = workbooks::model_master().to_bytes();
    let steps = [
        Step::InsertRows {
            sheet: "Assumptions".to_owned(),
            at: 4,
            count: 1,
        },
        set(
            "Assumptions",
            1,
            4,
            Write::Text("Annual discount".to_owned()),
        ),
        set("Assumptions", 2, 4, Write::Number("0.05".to_owned())),
    ];
    let out = Workbook::open(&apply_steps(&bytes, &steps).expect("written")).expect("opens");
    put_back(&excel("model-row-inserted.xlsx"), &out).expect("the same cells as Excel's");
    // The names moved with their rows, as Excel moved them.
    let names: Vec<(&str, &str)> = out
        .names
        .iter()
        .map(|n| (n.name.as_str(), n.refers_to.as_str()))
        .collect();
    assert!(names.contains(&("Price", "Assumptions!$B$5")), "{names:?}");
    assert!(
        names.contains(&("StartSeats", "Assumptions!$B$2")),
        "{names:?}"
    );
    // Asked to recalculate, and the stale chain is gone.
    let main = String::from_utf8_lossy(out.package.part("xl/workbook.xml").expect("main"));
    assert!(main.contains(r#"fullCalcOnLoad="1""#));
    // The merged title and the conditional format on the summary are not on
    // this sheet, so they stay.
    assert_eq!(out.sheets[3].merged.len(), 1);
}

#[test]
fn a_set_keeps_the_cells_format_and_everything_else() {
    let kit = workbooks::model_master();
    let bytes = kit.to_bytes();
    let before = Workbook::open(&bytes).expect("opens");
    let out = apply_steps(
        &bytes,
        &[set("Assumptions", 2, 4, Write::Number("55".to_owned()))],
    )
    .expect("written");
    let after = Workbook::open(&out).expect("opens");
    assert_eq!(shown(&after, "Assumptions", "B4"), "55");
    let style = |wb: &Workbook| wb.sheets[0].cell(2, 4).map(|c| c.style);
    assert_eq!(style(&after), style(&before));
    // Every other part as it was.
    for part in before.package.part_names() {
        if part != "xl/worksheets/sheet1.xml" && part != "xl/workbook.xml" {
            assert_eq!(
                before.package.part(part),
                after.package.part(part),
                "{part}"
            );
        }
    }
}

#[test]
fn new_cells_and_rows_go_where_they_belong() {
    let bytes = workbooks::tracker_master().to_bytes();
    let steps = [
        // A new cell at the end of a row, one in the middle of an empty
        // stretch, a row after the last, and a row between two.
        set("Applications", 7, 2, Write::Text("Remote".to_owned())),
        set(
            "Applications",
            6,
            4,
            Write::Text("Ask for feedback".to_owned()),
        ),
        set("Applications", 1, 12, Write::Text("APP-011".to_owned())),
        set("Counts", 3, 3, Write::Formula("B3*2".to_owned())),
        set("Applications", 2, 12, Write::Text("Moss & Pine".to_owned())),
    ];
    let out = Workbook::open(&apply_steps(&bytes, &steps).expect("written")).expect("opens");
    assert_eq!(shown(&out, "Applications", "G2"), "Remote");
    assert_eq!(shown(&out, "Applications", "F4"), "Ask for feedback");
    assert_eq!(shown(&out, "Applications", "B12"), "Moss & Pine");
    assert_eq!(
        super::super::deps::formula_at(&out, 1, 3, 3).as_deref(),
        Some("B3*2")
    );
}

#[test]
fn text_that_looks_like_a_number_stays_text() {
    let bytes = workbooks::tracker_master().to_bytes();
    let out = apply_steps(
        &bytes,
        &[
            set("Applications", 6, 2, Write::Text("00123".to_owned())),
            set("Applications", 6, 3, Write::Text("=SUM(A1)".to_owned())),
            set("Applications", 6, 5, Write::Bool(true)),
            set("Applications", 6, 6, Write::Error("#N/A".to_owned())),
        ],
    )
    .expect("written");
    let out = Workbook::open(&out).expect("opens");
    let cell = |r| out.sheets[0].cell(6, r).map(|c| c.content.clone());
    assert_eq!(cell(2), Some(Content::Inline("00123".into())));
    assert_eq!(cell(3), Some(Content::Inline("=SUM(A1)".into())));
    assert_eq!(cell(5), Some(Content::Bool(true)));
    assert!(matches!(cell(6), Some(Content::Error(_))));
}

#[test]
fn overwriting_a_shared_formula_writes_out_its_group() {
    let bytes = workbooks::model_master().to_bytes();
    // C2 is the master of the MRR column's shared formula.
    let out = apply_steps(
        &bytes,
        &[set("Revenue", 3, 2, Write::Formula("B2*Price".to_owned()))],
    )
    .expect("written");
    let out = Workbook::open(&out).expect("opens");
    let revenue = out.sheet_index("Revenue").expect("sheet");
    assert_eq!(
        crate::xlsx::deps::formula_at(&out, revenue, 2, 3).as_deref(),
        Some("B2*Price")
    );
    assert_eq!(
        crate::xlsx::deps::formula_at(&out, revenue, 9, 3).as_deref(),
        Some("B9*Price*(1-Churn)"),
        "the rest of the column reads as before"
    );
}

#[test]
fn deleting_rows_is_refused() {
    let bytes = workbooks::tracker_master().to_bytes();
    let err = apply_steps(
        &bytes,
        &[Step::DeleteRows {
            sheet: "Applications".to_owned(),
            at: 3,
            count: 1,
        }],
    )
    .expect_err("refused");
    assert!(matches!(err, Error::CannotEdit(_)));
}

#[test]
fn rows_inserted_in_a_table_grow_it() {
    let bytes = workbooks::budget_master().to_bytes();
    let out = apply_steps(
        &bytes,
        &[Step::InsertRows {
            sheet: "Transactions".to_owned(),
            at: 5,
            count: 2,
        }],
    )
    .expect("written");
    let out = Workbook::open(&out).expect("opens");
    assert_eq!(crate::xlsx::refs::area_name(&out.tables[0].area), "A1:D19");
    // The summary's structured references don't change; plain ones would.
    assert_eq!(out.sheets[0].rows.iter().map(|r| r.r).max(), Some(19));
}

/// Writes what the writer makes to target/xlsx-written, for
/// scripts/excel-smoke.ps1 and LibreOffice to open.
#[test]
#[ignore = "writes files for opening in Excel"]
fn write_samples_for_excel() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/xlsx-written");
    std::fs::create_dir_all(&dir).expect("folder");
    let cases: Vec<(&str, Vec<u8>, Vec<Step>)> = vec![
        (
            "model-row-inserted",
            workbooks::model_master().to_bytes(),
            vec![
                Step::InsertRows {
                    sheet: "Assumptions".to_owned(),
                    at: 4,
                    count: 1,
                },
                set(
                    "Assumptions",
                    1,
                    4,
                    Write::Text("Annual discount".to_owned()),
                ),
                set("Assumptions", 2, 4, Write::Number("0.05".to_owned())),
            ],
        ),
        (
            "model-price",
            workbooks::model_master().to_bytes(),
            vec![set("Assumptions", 2, 4, Write::Number("55".to_owned()))],
        ),
        (
            "model-shared-overwritten",
            workbooks::model_master().to_bytes(),
            vec![set("Revenue", 3, 2, Write::Formula("B2*Price".to_owned()))],
        ),
        (
            "tracker-cells",
            workbooks::tracker_master().to_bytes(),
            vec![
                set("Applications", 7, 2, Write::Text("Remote".to_owned())),
                set("Applications", 6, 4, Write::Text("00123".to_owned())),
                set("Applications", 1, 12, Write::Text("APP-011".to_owned())),
                set("Counts", 3, 3, Write::Formula("B3*2".to_owned())),
                set("Applications", 6, 5, Write::Error("#N/A".to_owned())),
            ],
        ),
        (
            "budget-rows",
            workbooks::budget_master().to_bytes(),
            vec![
                Step::InsertRows {
                    sheet: "Transactions".to_owned(),
                    at: 5,
                    count: 2,
                },
                set("Transactions", 1, 5, Write::Number("46030".to_owned())),
                set("Transactions", 2, 5, Write::Text("Transport".to_owned())),
                set("Transactions", 3, 5, Write::Text("Taxi".to_owned())),
                set("Transactions", 4, 5, Write::Number("18.5".to_owned())),
            ],
        ),
    ];
    for (name, bytes, steps) in cases {
        let out = apply_steps(&bytes, &steps).expect("written");
        std::fs::write(dir.join(format!("{name}.xlsx")), out).expect("write");
    }
}

#[test]
fn macros_and_untracked_parts_come_through_a_write_untouched() {
    let bytes = workbooks::model_xlsm().to_bytes();
    let before = Workbook::open(&bytes).expect("opens");
    let out = apply_steps(
        &bytes,
        &[set("Assumptions", 2, 4, Write::Number("55".to_owned()))],
    )
    .expect("written");
    let after = Workbook::open(&out).expect("opens");
    assert!(after.macro_enabled);
    assert!(after.package.is_unchanged("xl/vbaProject.bin"));
    assert_eq!(
        before.package.part("xl/vbaProject.bin"),
        after.package.part("xl/vbaProject.bin")
    );
}
