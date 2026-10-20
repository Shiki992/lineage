// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! `cargo xtask bench-objects`: times reading and comparing macros, Power
//! Query queries and pivot tables (Phase 5B), natively. Build with
//! `--release`. Invented workbooks only.

use std::time::{Duration, Instant};

use lineage_core::vba;
use lineage_core::xlsx::Workbook;
use lineage_core::xlsx::diff::diff;
use lineage_core::xlsx::query;
use lineage_core::xlsx::view::view;
use lineage_testkit::advanced;
use lineage_testkit::mashup;
use lineage_testkit::pivots::{CacheField, Pivot, PivotCache, ValueSpec};
use lineage_testkit::vba::{Module, ModuleKind, Project};

fn ms(d: Duration) -> String {
    format!("{:.1}", d.as_secs_f64() * 1000.0)
}

fn time<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let out = f();
    (out, start.elapsed())
}

fn long_project(lines: usize, edit: bool) -> Vec<u8> {
    let body: String = (0..lines)
        .map(|n| {
            if edit && n == lines / 2 {
                "    total = total * 2\n".to_owned()
            } else {
                format!("    total = total + {n} ' step {n}\n")
            }
        })
        .collect();
    Project::new(vec![Module::new(
        "Long",
        ModuleKind::Standard,
        &format!("Public Sub Sum()\n    Dim total As Double\n{body}End Sub"),
    )])
    .to_bytes()
}

/// Prints one line per case.
///
/// # Errors
/// When a workbook or part doesn't read.
pub fn bench() -> Result<(), String> {
    // A 20,000-line module, one line edited.
    let (a, b) = (long_project(20_000, false), long_project(20_000, true));
    let (pa, read) = time(|| vba::read(&a, &[]));
    let pa = pa.map_err(|e| e.to_string())?;
    let pb = vba::read(&b, &[]).map_err(|e| e.to_string())?;
    let (d, compare) = time(|| vba::diff::diff(Some(&pa), Some(&pb)));
    println!(
        "vba-20k-lines\tbin {} KB\tread {} ms\tcompare {} ms\t{}",
        a.len() / 1024,
        ms(read),
        ms(compare),
        d.modules.first().map_or("", |m| m.summary.as_str())
    );

    // 200 queries of 20 lines each, one edited.
    let queries = |edit: bool| -> Vec<mashup::Query> {
        (0..200)
            .map(|n| {
                let steps: String = (0..18)
                    .map(|k| format!("    Step{k} = Table.AddColumn(Source, \"c{k}\", each {n} + {k}),\n"))
                    .collect();
                let last = if edit && n == 100 { "Edited" } else { "Done" };
                mashup::Query::new(
                    &format!("Query{n}"),
                    &format!("{{00000000-0000-0000-0000-{n:012}}}"),
                    &format!("let\n    Source = #table({{}}, {{}}),\n{steps}    {last} = Step17\nin\n    {last}"),
                )
            })
            .collect()
    };
    let (ia, ib) = (
        mashup::item_xml(&queries(false)),
        mashup::item_xml(&queries(true)),
    );
    let (qa, read) = time(|| query::read("customXml/item1.xml", &ia));
    let qa = qa.map_err(|e| e.to_string())?;
    let qb = query::read("customXml/item1.xml", &ib).map_err(|e| e.to_string())?;
    let (d, compare) = time(|| query::diff(Some(&qa), Some(&qb)));
    println!(
        "queries-200\titem {} KB\tread {} ms\tcompare {} ms\t{}",
        ia.len() / 1024,
        ms(read),
        ms(compare),
        d.first().map_or("", |q| q.summary.as_str())
    );

    // A pivot cache of 20,000 records under two pivots.
    let records: Vec<Vec<String>> = (0..20_000)
        .map(|n| {
            vec![
                format!("Region {}", n % 40),
                format!("Plan {}", n % 7),
                (n % 90 + 1).to_string(),
            ]
        })
        .collect();
    let mut wb = lineage_testkit::workbooks::model_master();
    wb.sheets
        .push(lineage_testkit::xlsx::Sheet::new("Pivots", 99));
    wb.pivot_caches.push(PivotCache {
        sheet: "Assumptions".to_owned(),
        range: "A1:C20001".to_owned(),
        fields: vec![
            CacheField::text("Region"),
            CacheField::text("Plan"),
            CacheField::number("Seats"),
        ],
        records,
        refreshed: "46310.5".to_owned(),
    });
    for (n, rows, at) in [("ByRegion", 0, "A3"), ("ByPlan", 1, "E3")] {
        wb.pivots.push(Pivot {
            name: n.to_owned(),
            sheet: wb.sheets.len() - 1,
            cache: 0,
            at: at.to_owned(),
            rows,
            values: vec![ValueSpec::sum("Sum of Seats", 2)],
            filter: None,
            hidden: Vec::new(),
            style: "PivotStyleLight16".to_owned(),
            form: lineage_testkit::pivots::Form::Compact,
        });
    }
    let bytes = wb.to_bytes();
    let (w, open) = time(|| Workbook::open(&bytes));
    let w = w.map_err(|e| e.to_string())?;
    let (d, compare) = time(|| diff(&w, &w, &[]));
    println!(
        "pivot-20k-records\tfile {} KB\topen {} ms\tcompare {} ms\tpivots {}\tchanges {}",
        bytes.len() / 1024,
        ms(open),
        ms(compare),
        w.pivots.tables.len(),
        d.pivots.len()
    );

    // The full model, master against an edited macro: open both, compare,
    // and the view.
    let (a, b) = (
        advanced::model_full().to_bytes(),
        advanced::model_full_macro_edited().to_bytes(),
    );
    let ((wa, wb), open) = time(|| (Workbook::open(&a), Workbook::open(&b)));
    let (wa, wb) = (
        wa.map_err(|e| e.to_string())?,
        wb.map_err(|e| e.to_string())?,
    );
    let (d, compare) = time(|| diff(&wa, &wb, &[]));
    let (v, words) = time(|| view(&d, &wa, &wb));
    println!(
        "model-full\tfile {} KB\topen both {} ms\tcompare {} ms\tview {} ms\t{}",
        a.len() / 1024,
        ms(open),
        ms(compare),
        ms(words),
        v.headline
    );
    Ok(())
}
