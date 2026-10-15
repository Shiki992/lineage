// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Any bytes into the .docx reader, then everything that reads a model:
//! comparing it with itself, summarising, and making a clean copy. None of it
//! may panic; refusing the file is fine.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lineage_core::{Docx, View, clean::clean_copy, diff::diff};

fuzz_target!(|data: &[u8]| {
    for view in [View::Accepted, View::Rejected] {
        if let Ok(docx) = Docx::open_view(data, view) {
            let changes = diff(&docx, &docx);
            assert!(changes.is_empty(), "a document differs from itself");
            let _ = docx.body_lines();
        }
    }
    let _ = clean_copy(data, None);
});
