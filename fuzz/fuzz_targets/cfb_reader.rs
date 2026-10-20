// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Any bytes into the compound-file reader that `vbaProject.bin` is read
//! with. It may refuse the file; it may never panic, loop or run past the
//! input.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lineage_core::vba::cfb;

fuzz_target!(|data: &[u8]| {
    if let Ok(file) = cfb::read(data) {
        for entry in &file.entries {
            assert!(!entry.path.is_empty());
            assert!(entry.data.len() <= cfb::MAX_FILE_BYTES);
        }
    }
});
