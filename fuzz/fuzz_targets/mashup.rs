// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Any bytes into the Power Query reader ([MS-QDEFF]). Text that looks like
//! XML goes in as it is; anything else becomes the base64 of a DataMashup
//! item, so the fuzzer reaches the binary stream and its inner package. The
//! M splitter gets the bytes as text too. None of it may panic.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lineage_core::xlsx::query;
use lineage_testkit::mashup::base64;

fuzz_target!(|data: &[u8]| {
    let item = if data.first() == Some(&b'<') || data.starts_with(&[0xFF, 0xFE]) {
        data.to_vec()
    } else {
        format!(
            "<DataMashup xmlns=\"http://schemas.microsoft.com/DataMashup\">{}</DataMashup>",
            base64(data)
        )
        .into_bytes()
    };
    if let Ok(m) = query::read("customXml/item1.xml", &item) {
        assert!(query::diff(Some(&m), Some(&m)).is_empty());
    }
    let _ = query::members(&String::from_utf8_lossy(data));
});
