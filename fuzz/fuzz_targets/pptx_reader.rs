// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Any bytes into the .pptx reader, then everything that reads a deck model:
//! comparing it with itself, merging it with itself, and making a clean copy.
//! None of it may panic; refusing the file is fine.

#![no_main]

use libfuzzer_sys::fuzz_target;
use lineage_core::Deck;
use lineage_core::pptx::{clean::clean_copy, diff::diff, merge::plan};

fuzz_target!(|data: &[u8]| {
    if let Ok(deck) = Deck::open(data) {
        let _ = deck.lines();
        assert!(diff(&deck, &deck).changes.is_empty(), "a deck differs from itself");
        assert!(plan(&deck, &deck, &deck).items.is_empty());
    }
    let _ = clean_copy(data, None);
});
