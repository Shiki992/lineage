// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Test-only builders for synthetic Word documents, PowerPoint decks and Excel
//! workbooks. Never shipped.
//!
//! `cargo xtask fixtures` writes the files in [`fixtures::all`] to
//! `fixtures/docx/synthetic/` and those in [`decks::all`] to
//! `fixtures/pptx/synthetic/`, and engine tests use the same builders to make
//! documents in memory (including random ones for property tests). Every name,
//! company and figure in here is invented.

#![forbid(unsafe_code)]

pub mod advanced;
pub mod checksum;
pub mod decks;
pub mod docx;
pub mod features;
pub mod fixtures;
pub mod mashup;
pub mod pivots;
pub mod png;
pub mod pptx;
pub mod resume;
pub mod returned;
pub mod vba;
pub mod workbooks;
pub mod xlsx;
pub mod zip_writer;
