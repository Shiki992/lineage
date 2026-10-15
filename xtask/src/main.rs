// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Repository tasks. Run with `cargo xtask <task>`.

#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let task = std::env::args().nth(1);
    match task.as_deref() {
        _ => {
            eprintln!("usage: cargo xtask <task>");
            ExitCode::FAILURE
        }
    }
}
