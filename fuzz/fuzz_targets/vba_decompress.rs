// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Any bytes into the VBA decompression ([MS-OVBA] 2.4.1). It may refuse
//! them; it may never panic or give more than its limit.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lineage_core::vba::compress::decompress;

const LIMIT: usize = 1 << 20;

fuzz_target!(|data: &[u8]| {
    // Mostly containers: the signature byte, then whatever came.
    let mut container = vec![0x01];
    container.extend_from_slice(data);
    for input in [data, container.as_slice()] {
        if let Ok(out) = decompress(input, LIMIT) {
            assert!(out.len() <= LIMIT);
        }
    }
});
