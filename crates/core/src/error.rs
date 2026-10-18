// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Errors the engine can return. Every error has a plain-language message for the
//! panel ([`Error::user_message`]) and a structural detail for logs. Details never
//! contain document text, because returned files and the maintainer's own
//! documents may hold personal data.

use std::fmt;

/// What went wrong, in terms the panel can act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// The bytes are not a zip archive at all.
    NotAZip(String),
    /// The archive or one of its parts is damaged (bad checksum, truncated, overlapping entries).
    Damaged(String),
    /// The file is password protected.
    Encrypted,
    /// The file is larger than the engine is willing to open.
    TooLarge(String),
    /// A real Word file that uses something this version cannot handle yet.
    Unsupported(String),
    /// A zip archive, but not a Word document.
    NotAWordDocument(String),
    /// A zip archive, but not a PowerPoint deck.
    NotADeck(String),
    /// A zip archive, but not an Office file at all (no content types).
    NotAnOfficeFile(String),
    /// A part that must be XML could not be parsed.
    BadXml { part: String, detail: String },
    /// The engine refused to write, because it could not do so without risking the file.
    CannotEdit(String),
    /// What PowerPoint holds after Lineage changed its slides isn't what Lineage
    /// meant it to hold (decision 0019).
    NotLanded(String),
}

impl Error {
    /// The message the panel shows. Plain language, no technical words.
    #[must_use]
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::NotAWordDocument(_) => "This file isn't a Word document Lineage can open.",
            Self::NotADeck(_) => "This file isn't a PowerPoint deck Lineage can open.",
            Self::NotAZip(_) | Self::NotAnOfficeFile(_) => {
                "This file isn't a document Lineage can open."
            }
            Self::Damaged(_) | Self::BadXml { .. } => {
                "This file looks damaged. Try opening it and saving it again."
            }
            Self::Encrypted => {
                "This file is password protected. Remove the password, then try again."
            }
            Self::TooLarge(_) => "This file is too large for Lineage to open.",
            Self::Unsupported(_) => "This kind of file isn't supported yet.",
            Self::CannotEdit(_) => {
                "Lineage couldn't make this change safely, so it left the file as it was."
            }
            Self::NotLanded(_) => {
                "PowerPoint didn't take the changes exactly as Lineage wrote them."
            }
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAZip(detail) => write!(f, "not a zip archive: {detail}"),
            Self::Damaged(detail) => write!(f, "damaged package: {detail}"),
            Self::Encrypted => write!(f, "encrypted package"),
            Self::TooLarge(detail) => write!(f, "too large: {detail}"),
            Self::Unsupported(detail) => write!(f, "unsupported: {detail}"),
            Self::NotAWordDocument(detail) => write!(f, "not a Word document: {detail}"),
            Self::NotADeck(detail) => write!(f, "not a PowerPoint deck: {detail}"),
            Self::NotAnOfficeFile(detail) => write!(f, "not an Office file: {detail}"),
            Self::BadXml { part, detail } => write!(f, "bad XML in {part}: {detail}"),
            Self::CannotEdit(detail) => write!(f, "cannot edit safely: {detail}"),
            Self::NotLanded(detail) => write!(f, "did not land: {detail}"),
        }
    }
}

impl std::error::Error for Error {}

/// Shorthand used across the crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_error_has_a_plain_message() {
        let all = [
            Error::NotAZip(String::new()),
            Error::Damaged(String::new()),
            Error::Encrypted,
            Error::TooLarge(String::new()),
            Error::Unsupported(String::new()),
            Error::NotAWordDocument(String::new()),
            Error::NotADeck(String::new()),
            Error::NotAnOfficeFile(String::new()),
            Error::BadXml {
                part: String::new(),
                detail: String::new(),
            },
            Error::CannotEdit(String::new()),
            Error::NotLanded(String::new()),
        ];
        for error in all {
            let message = error.user_message();
            assert!(message.ends_with('.'), "{message}");
            for word in ["zip", "XML", "merge", "diff", "commit", "branch"] {
                assert!(!message.contains(word), "{message} uses {word}");
            }
        }
    }
}
