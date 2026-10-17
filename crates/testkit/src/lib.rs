// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Test-only builders for synthetic Word documents and PowerPoint decks. Never
//! shipped.
//!
//! `cargo xtask fixtures` writes the files in [`fixtures::all`] to
//! `fixtures/docx/synthetic/` and those in [`decks::all`] to
//! `fixtures/pptx/synthetic/`, and engine tests use the same builders to make
//! documents in memory (including random ones for property tests). Every name,
//! company and figure in here is invented.

#![forbid(unsafe_code)]

pub mod checksum;
pub mod decks;
pub mod docx;
pub mod features;
pub mod fixtures;
pub mod png;
pub mod pptx;
pub mod resume;
pub mod returned;
pub mod zip_writer;
