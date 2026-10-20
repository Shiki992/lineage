// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Any bytes into the VBA project reader. A compound file goes in as it
//! is; anything else becomes a project's compressed dir stream and module
//! source, so the fuzzer reaches the dir records and module decoding
//! without first forging a compound file. None of it may panic, and a
//! project that reads must compare equal to itself.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lineage_core::vba;
use lineage_testkit::vba::{CfbStream, compress, write_cfb};

fuzz_target!(|data: &[u8]| {
    let bytes = if data.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        data.to_vec()
    } else {
        let (dir, module) = data.split_at(data.len() / 2);
        write_cfb(&[
            CfbStream {
                path: "PROJECT".to_owned(),
                data: b"ID=\"{00000000-0000-0000-0000-000000000000}\"\r\nModule=Module1\r\n".to_vec(),
            },
            CfbStream {
                path: "VBA/dir".to_owned(),
                data: compress(dir),
            },
            CfbStream {
                path: "VBA/Module1".to_owned(),
                data: compress(module),
            },
        ])
    };
    if let Ok(project) = vba::read(&bytes, &[data]) {
        assert!(vba::diff::diff(Some(&project), Some(&project)).is_empty());
    }
});
