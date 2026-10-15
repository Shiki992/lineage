// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Lineage document engine.
//!
//! Reads Office packages, builds canonical models, computes what changed, runs
//! three-way merges and writes results back. Pure: no file, network or clock access.
//! The engine itself arrives in Phase 1; Phase 0 only fixes the crate's place in the
//! workspace and the open change format it will implement (`spec/format/`).

#![forbid(unsafe_code)]

/// Version of the open change format (`spec/format/v0`) this crate implements.
pub const SPEC_VERSION: &str = "0";

/// Version of the engine, taken from the workspace manifest.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_are_set() {
        assert_eq!(SPEC_VERSION, "0");
        assert!(!ENGINE_VERSION.is_empty());
    }
}
