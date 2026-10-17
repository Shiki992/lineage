// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Any bytes into the .pptx reader, then everything that reads a deck model.
//! None of it may panic; refusing the file is fine.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lineage_core::Deck;

fuzz_target!(|data: &[u8]| {
    if let Ok(deck) = Deck::open(data) {
        let _ = deck.lines();
        for slide in &deck.slides {
            let _ = slide.title();
            let _ = slide.text();
        }
    }
});
