// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Any bytes into the zip reader. Whatever it accepts must write back and read
//! again to the same parts.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lineage_core::Package;

fuzz_target!(|data: &[u8]| {
    if let Ok(package) = Package::read(data) {
        let bytes = package.to_bytes().expect("an accepted package writes");
        let again = Package::read(&bytes).expect("what we write reads back");
        assert_eq!(package.parts(), again.parts());
    }
});
