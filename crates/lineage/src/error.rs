// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Errors from family logic, each with a plain-language message for the panel.

use std::fmt;

/// What went wrong.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The document engine refused the file or the edit.
    Engine(lineage_core::Error),
    /// The history (ledger) is damaged or from a newer version.
    Ledger(String),
    /// A stored blob is missing.
    MissingBlob(String),
    /// The file belongs to a different family or version than the one asked for.
    WrongFile(String),
    /// The request does not fit the current state, such as offering to a master.
    NotAllowed(String),
}

impl Error {
    /// The message the panel shows.
    #[must_use]
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::Engine(e) => e.user_message(),
            Self::Ledger(_) | Self::MissingBlob(_) => {
                "Lineage's history for this file is incomplete. Nothing was changed."
            }
            Self::WrongFile(_) => {
                "This file isn't the version Lineage expected. Open the right file and try again."
            }
            Self::NotAllowed(_) => "That can't be done for this file.",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Engine(e) => write!(f, "{e}"),
            Self::Ledger(d) => write!(f, "ledger: {d}"),
            Self::MissingBlob(h) => write!(f, "missing blob {h}"),
            Self::WrongFile(d) => write!(f, "wrong file: {d}"),
            Self::NotAllowed(d) => write!(f, "not allowed: {d}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<lineage_core::Error> for Error {
    fn from(e: lineage_core::Error) -> Self {
        Self::Engine(e)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
