// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! `cargo xtask bench-xlsx [case]`: times each stage of comparing two
//! workbooks, natively, for the Phase 5 budgets (decision 0025). Build with
//! `--release`. Each case can run on its own, so a wrapper can read the
//! process's peak memory for one case at a time
//! (`scripts/bench-xlsx.ps1`). Invented workbooks only.

use std::time::{Duration, Instant};

use lineage_core::Package;
use lineage_core::xlsx::Workbook;
use lineage_core::xlsx::align::{align, pair_sheets};
use lineage_core::xlsx::deps::Deps;
use lineage_core::xlsx::diff::diff;
use lineage_core::xlsx::view::view;
use lineage_testkit::workbooks;

/// The cases: a name and the two workbooks, as bytes.
fn cases() -> Vec<(&'static str, Vec<u8>, Vec<u8>)> {
    let edited = |rows: u32| {
        let mut list = workbooks::orders(rows);
        list[(rows / 2) as usize].qty += 1;
        workbooks::orders_workbook(&list).to_bytes()
    };
    vec![
        (
            "model-price",
            workbooks::model_master().to_bytes(),
            workbooks::model_price_update().to_bytes(),
        ),
        ("stores-60", workbooks::many_sheets(60).to_bytes(), {
            let mut wb = workbooks::many_sheets(60);
            if let Some(sheet) = wb.sheets.get_mut(5) {
                sheet.set(
                    "B10",
                    lineage_testkit::xlsx::num("12345.67"),
                    lineage_testkit::xlsx::Style::Money,
                );
            }
            wb.to_bytes()
        }),
        (
            "orders-50k-edit",
            workbooks::large(50_000).to_bytes(),
            edited(50_000),
        ),
        (
            "orders-50k-sorted",
            workbooks::large(50_000).to_bytes(),
            workbooks::large_sorted(50_000).to_bytes(),
        ),
        (
            "orders-200k-edit",
            workbooks::large(200_000).to_bytes(),
            edited(200_000),
        ),
        (
            "orders-200k-sorted",
            workbooks::large(200_000).to_bytes(),
            workbooks::large_sorted(200_000).to_bytes(),
        ),
    ]
}

fn ms(d: Duration) -> String {
    format!("{:.0}", d.as_secs_f64() * 1000.0)
}

fn time<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let out = f();
    (out, start.elapsed())
}

fn run(name: &str, a: &[u8], b: &[u8]) -> Result<(), String> {
    let (pa, zip) = time(|| Package::read(a));
    let pa = pa.map_err(|e| e.to_string())?;
    let xml: usize = pa
        .part_names()
        .filter_map(|n| pa.part(n))
        .map(<[u8]>::len)
        .sum();
    let (wa, model) = time(|| Workbook::from_package(pa));
    let wa = wa.map_err(|e| e.to_string())?;
    let wb = Workbook::open(b).map_err(|e| e.to_string())?;
    let (_, alignment) = time(|| {
        let pairs = pair_sheets(&wa, &wb);
        pairs
            .both
            .iter()
            .map(|&(i, j)| align(&wa, i, &wb, j).rows.pairs.len())
            .sum::<usize>()
    });
    let (_, deps) = time(|| Deps::build(&wb).cells.len());
    let (d, whole) = time(|| diff(&wa, &wb, &[]));
    let (v, words) = time(|| view(&d, &wa, &wb));
    println!(
        "{name}\tfile {} KB\txml {} KB\tcells {}\tzip {} ms\tmodel {} ms\talign {} ms\tdeps {} ms\tdiff {} ms\tview {} ms\t{}",
        a.len() / 1024,
        xml / 1024,
        wa.cell_count(),
        ms(zip),
        ms(model),
        ms(alignment),
        ms(deps),
        ms(whole),
        ms(words),
        v.headline
    );
    Ok(())
}

/// Where the generated workbooks go, so a measured run only reads them.
fn dir() -> std::path::PathBuf {
    crate::repo_root().join("target").join("bench-xlsx")
}

const NAMES: [&str; 6] = [
    "model-price",
    "stores-60",
    "orders-50k-edit",
    "orders-50k-sorted",
    "orders-200k-edit",
    "orders-200k-sorted",
];

/// `prepare` writes every case's two workbooks; a case name runs that case
/// from the files; nothing runs every case.
///
/// # Errors
/// When a workbook doesn't open or a file can't be read or written.
pub fn bench(only: Option<&str>) -> Result<(), String> {
    let dir = dir();
    if only == Some("prepare") {
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        for (name, a, b) in cases() {
            std::fs::write(dir.join(format!("{name}-a.xlsx")), a).map_err(|e| e.to_string())?;
            std::fs::write(dir.join(format!("{name}-b.xlsx")), b).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    for name in NAMES {
        if only.is_none_or(|o| o == name) {
            let read = |side: &str| {
                std::fs::read(dir.join(format!("{name}-{side}.xlsx")))
                    .map_err(|e| format!("{e}: run `cargo xtask bench-xlsx prepare` first"))
            };
            run(name, &read("a")?, &read("b")?)?;
        }
    }
    Ok(())
}
