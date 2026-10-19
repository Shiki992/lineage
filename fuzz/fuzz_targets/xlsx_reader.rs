// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Any bytes into the .xlsx reader. A zip goes in as it is; anything else
//! becomes the worksheet part of a small workbook, so the fuzzer reaches the
//! streamed sheet reader without first having to forge zip checksums. None
//! of it may panic; refusing the file is fine.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lineage_core::Workbook;
use lineage_testkit::xlsx;

fuzz_target!(|data: &[u8]| {
    let bytes = if data.starts_with(b"PK") {
        data.to_vec()
    } else {
        let mut wb = xlsx::Workbook::new("Fuzz");
        wb.sheets.push(xlsx::Sheet::new("Data", 1));
        let mut parts = wb.parts();
        for (name, part) in &mut parts {
            if name == "xl/worksheets/sheet1.xml" {
                *part = data.to_vec();
            }
        }
        lineage_testkit::zip_writer::write_stored(&parts)
    };
    if let Ok(wb) = Workbook::open(&bytes) {
        for (index, sheet) in wb.sheets.iter().enumerate() {
            let text = wb.sheet_text(index);
            for row in &sheet.rows {
                let _ = row.span.get(text);
            }
        }
        let _ = wb.cell_count();
    }
});
