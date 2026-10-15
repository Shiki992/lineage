// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Test-only builder for synthetic Word documents. Never shipped.
//!
//! `cargo xtask fixtures` writes the files in [`fixtures::all`] to
//! `fixtures/docx/synthetic/`, and engine tests use the same builder to make
//! documents in memory (including random ones for property tests). Every name,
//! company and figure in here is invented.

#![forbid(unsafe_code)]

pub mod checksum;
pub mod docx;
pub mod features;
pub mod fixtures;
pub mod png;
pub mod resume;
pub mod zip_writer;
