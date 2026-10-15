// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Test-only harness for end-to-end scenarios (decision 0001).
//!
//! Phase 1 adds local-folder and in-memory storage here, one function per button the
//! personal screens will have, and scenario tests that click through them the way a
//! person would. This crate is never shipped, and there is no user-facing command line.

#![forbid(unsafe_code)]

/// The engine version the harness was built against.
#[must_use]
pub fn engine_version() -> &'static str {
    lineage::ENGINE_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_against_the_family_crate() {
        assert_eq!(engine_version(), lineage::ENGINE_VERSION);
    }
}
