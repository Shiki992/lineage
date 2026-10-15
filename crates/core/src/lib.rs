// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Lineage document engine.
//!
//! Reads Office packages, builds canonical models, computes what changed, runs
//! three-way merges and writes results back. Pure: no file, network or clock
//! access. Callers pass bytes in and get bytes back.

#![forbid(unsafe_code)]

pub mod check;
pub mod clean;
pub mod diff;
pub mod docx;
pub mod error;
pub mod hash;
pub mod merge;
pub mod package;
pub mod summary;
pub mod view;
pub mod write;
pub mod xml;

#[cfg(test)]
pub(crate) mod testutil;

pub use docx::{Docx, View};
pub use error::{Error, Result};
pub use package::Package;

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
