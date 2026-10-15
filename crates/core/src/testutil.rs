// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Helpers shared by the engine's unit tests.

use std::path::PathBuf;

use lineage_testkit::docx::Package;
use lineage_testkit::{fixtures, resume};

use crate::docx::{Docx, View};

/// A synthetic fixture's builder, by file name.
pub fn fixture(name: &str) -> Package {
    fixtures::all()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("no fixture {name}"))
        .1
}

/// A resume built from lines, as bytes.
pub fn resume_bytes(lines: &[resume::Line]) -> Vec<u8> {
    resume::package("Test resume", lines).to_bytes()
}

pub fn open(bytes: &[u8]) -> Docx {
    Docx::open(bytes).unwrap_or_else(|e| panic!("opens: {e}"))
}

pub fn open_view(bytes: &[u8], view: View) -> Docx {
    Docx::open_view(bytes, view).unwrap_or_else(|e| panic!("opens: {e}"))
}

pub fn open_lines(lines: &[resume::Line]) -> Docx {
    open(&resume_bytes(lines))
}

/// Writes an output for the Word smoke test when `LINEAGE_SMOKE_DIR` is set,
/// with the paragraph text Word should show once every revision is accepted and
/// once every revision is rejected.
pub fn smoke(name: &str, bytes: &[u8]) {
    let Some(dir) = std::env::var_os("LINEAGE_SMOKE_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("smoke folder");
    std::fs::write(dir.join(format!("{name}.docx")), bytes).expect("write output");
    for (view, suffix) in [(View::Accepted, "accepted"), (View::Rejected, "rejected")] {
        let lines = open_view(bytes, view).body_lines().join("\n");
        std::fs::write(dir.join(format!("{name}.{suffix}.txt")), lines)
            .expect("write expected text");
    }
}
