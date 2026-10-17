// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Lineage family logic.
//!
//! Turns ledger events and blobs into new events and blobs: checkpoints,
//! tailored versions, offers, change states (decision 0002), sent copies and file
//! identity. Pure: storage, ids and the clock are the host's, passed in.
//!
//! - [`event`]: the JSON Lines ledger.
//! - [`state`]: the family folded from it.
//! - [`commands`]: one function per panel button.
//! - [`changes`]: change states, the merge base, detection.
//! - [`identity`]: fingerprints and content matching.
//! - [`returned`]: files that come back: recognising, reviewing, keeping.
//! - [`blob`]: content-addressed blobs and snapshots.
//! - [`views`]: the panels, and a document's page in "My documents".
//! - [`read`]: reading a ledger that may be damaged, for screens that only show.
//! - [`library`], [`index`]: "My documents" and its rebuildable index.
//! - [`graph`]: the family tree.

#![forbid(unsafe_code)]

pub mod blob;
pub mod changes;
pub mod commands;
pub mod error;
pub mod event;
pub mod graph;
pub mod identity;
pub mod ids;
pub mod index;
pub mod library;
pub mod read;
pub mod returned;
pub mod state;
pub mod views;

pub use error::{Error, Result};
pub use ids::{Id, IdSource, SeededIds, SequentialIds, Timestamp};
pub use lineage_core::{ENGINE_VERSION, SPEC_VERSION};

#[cfg(test)]
mod history_tests;
#[cfg(test)]
mod testfamily;
#[cfg(test)]
mod tests;
