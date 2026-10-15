// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Lineage family logic.
//!
//! Turns ledger events and blobs into new events and blobs: checkpoints, tailored
//! versions, offers, change states, sent copies and file identity. Pure: storage is
//! the caller's job. The logic arrives in Phase 1.

#![forbid(unsafe_code)]

pub use lineage_core::{ENGINE_VERSION, SPEC_VERSION};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn re_exports_core_versions() {
        assert_eq!(ENGINE_VERSION, lineage_core::ENGINE_VERSION);
        assert_eq!(SPEC_VERSION, lineage_core::SPEC_VERSION);
    }
}
