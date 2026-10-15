// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Test-only harness for end-to-end scenarios (decision 0001).
//!
//! [`Panel`] has one method per button on the personal screens. Each method
//! does what the task pane will do: read the file and the family's history from
//! storage, call the pure command in `lineage`, store the blobs, append the
//! records, write the files, and return what the panel then shows. This crate is
//! never shipped, and there is no user-facing command line.

#![forbid(unsafe_code)]

pub mod storage;

use std::fmt;

use lineage::blob::{BlobHash, BlobSource};
use lineage::commands::{self, Context, Outcome, Updates};
use lineage::event::{self, MatchedBy};
use lineage::returned::{self, Candidate, Family, Recognition, RecognitionView, ReturnReview};
use lineage::state::FamilyState;
use lineage::views::{self, MasterPanel, SentList, VersionPanel};
use lineage::{Id, SequentialIds, Timestamp};
use lineage_core::merge::Decision;
use lineage_core::view::WhatChanged;
pub use lineage_core::write::Bring;

pub use storage::{LocalFolderStorage, MemoryStorage, Storage, StorageError};

/// The engine version the harness was built against.
#[must_use]
pub fn engine_version() -> &'static str {
    lineage::ENGINE_VERSION
}

/// What can go wrong when pressing a button.
#[derive(Debug)]
pub enum HarnessError {
    Lineage(lineage::Error),
    Storage(StorageError),
    /// The scenario did something the panel would not allow.
    Panel(String),
}

impl fmt::Display for HarnessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lineage(e) => write!(f, "{e} ({})", e.user_message()),
            Self::Storage(e) => write!(f, "{e}"),
            Self::Panel(d) => write!(f, "{d}"),
        }
    }
}

impl std::error::Error for HarnessError {}

impl From<lineage::Error> for HarnessError {
    fn from(e: lineage::Error) -> Self {
        Self::Lineage(e)
    }
}

impl From<StorageError> for HarnessError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}

pub type Result<T> = std::result::Result<T, HarnessError>;

struct StoredBlobs<'a, S: Storage> {
    storage: &'a S,
    family: &'a Id,
}

impl<S: Storage> BlobSource for StoredBlobs<'_, S> {
    fn blob(&self, hash: &BlobHash) -> Option<Vec<u8>> {
        self.storage.get_blob(self.family, hash)
    }
}

/// The personal panel, driven like a person would.
pub struct Panel<S: Storage> {
    pub storage: S,
    ids: SequentialIds,
    now: Timestamp,
    family: Option<Id>,
}

impl<S: Storage> Panel<S> {
    /// A panel over `storage`, with the clock starting at `now` and moving on a
    /// minute per button.
    pub fn new(storage: S, now: Timestamp) -> Self {
        Self {
            storage,
            ids: SequentialIds::new(1),
            now,
            family: None,
        }
    }

    /// Opens the family a file belongs to (as the task pane does on open).
    ///
    /// # Errors
    /// When the file is not tracked.
    pub fn open_family_of(&mut self, file: &str) -> Result<FamilyState> {
        let bytes = self.storage.read_file(file)?;
        let package = lineage_core::Package::read(&bytes).map_err(lineage::Error::from)?;
        let ids = lineage_core::docx::props::read_ids(&package)
            .map_err(lineage::Error::from)?
            .ok_or_else(|| HarnessError::Panel(format!("{file} is not tracked")))?;
        let family = Id::parse(&ids.family).map_err(HarnessError::Panel)?;
        self.family = Some(family);
        self.state()
    }

    fn family(&self) -> Result<&Id> {
        self.family
            .as_ref()
            .ok_or_else(|| HarnessError::Panel("no family is open".to_owned()))
    }

    /// The family's state, folded from the stored ledger.
    ///
    /// # Errors
    /// When the ledger cannot be read or is damaged.
    pub fn state(&self) -> Result<FamilyState> {
        let (bytes, _) = self.storage.read_ledger(self.family()?)?;
        Ok(FamilyState::from_records(&event::decode(&bytes)?)?)
    }

    fn node_of(state: &FamilyState, file: &str) -> Result<Id> {
        state
            .nodes
            .iter()
            .find(|n| n.storage_ref == file)
            .map(|n| n.id.clone())
            .ok_or_else(|| HarnessError::Panel(format!("{file} is not in this family")))
    }

    /// Runs a command against the latest state and stores what it produced.
    /// If the ledger moved on while the command ran (another device appended),
    /// the command runs once more on the new state.
    fn press<F>(&mut self, mut command: F) -> Result<Outcome>
    where
        F: FnMut(&mut Context<'_>, &FamilyState, &dyn BlobSource) -> lineage::Result<Outcome>,
    {
        for _attempt in 0..2 {
            let family = self.family()?.clone();
            let (bytes, version) = self.storage.read_ledger(&family)?;
            let state = FamilyState::from_records(&event::decode(&bytes)?)?;
            let outcome = {
                let blobs = StoredBlobs {
                    storage: &self.storage,
                    family: &family,
                };
                let mut ctx = Context {
                    now: self.now,
                    ids: &mut self.ids,
                    author: None,
                };
                command(&mut ctx, &state, &blobs)?
            };
            for blob in &outcome.blobs {
                self.storage.put_blob(&family, blob)?;
            }
            let lines = event::encode(&outcome.records)?;
            match self.storage.append_ledger(&family, version, &lines) {
                Ok(_) => {
                    self.now = Timestamp(self.now.0 + 60_000);
                    return Ok(outcome);
                }
                Err(StorageError::Conflict) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Err(HarnessError::Storage(StorageError::Conflict))
    }

    /// "Start tracking" the file: it becomes the master of a new family.
    ///
    /// # Errors
    /// When the file is already tracked or cannot be read.
    pub fn start_tracking(&mut self, file: &str) -> Result<MasterPanel> {
        let bytes = self.storage.read_file(file)?;
        let empty = lineage::blob::MemoryBlobs::default();
        let mut ctx = Context {
            now: self.now,
            ids: &mut self.ids,
            author: None,
        };
        let (state, outcome) = commands::start_tracking(&mut ctx, &bytes, file, file, &empty)?;
        let family = state.family_id.clone();
        for blob in &outcome.blobs {
            self.storage.put_blob(&family, blob)?;
        }
        self.storage
            .append_ledger(&family, 0, &event::encode(&outcome.records)?)?;
        if let Some(bytes) = &outcome.file {
            self.storage.write_file(file, bytes)?;
        }
        self.family = Some(family);
        self.now = Timestamp(self.now.0 + 60_000);
        self.master_panel()
    }

    /// The master panel.
    ///
    /// # Errors
    /// When the history cannot be read.
    pub fn master_panel(&self) -> Result<MasterPanel> {
        let state = self.state()?;
        let family = self.family()?;
        let blobs = StoredBlobs {
            storage: &self.storage,
            family,
        };
        Ok(views::master_panel(&state, &blobs)?)
    }

    /// A tailored version's panel.
    ///
    /// # Errors
    /// When the file is not a tailored version of this family.
    pub fn version_panel(&self, file: &str) -> Result<VersionPanel> {
        let state = self.state()?;
        let node = Self::node_of(&state, file)?;
        let family = self.family()?;
        let blobs = StoredBlobs {
            storage: &self.storage,
            family,
        };
        Ok(views::version_panel(&state, &node, &blobs)?)
    }

    fn write_outcome(&mut self, file: &str, outcome: &Outcome) -> Result<()> {
        if let Some(bytes) = &outcome.file {
            self.storage.write_file(file, bytes)?;
        }
        Ok(())
    }

    /// "Save checkpoint" for the open file.
    ///
    /// # Errors
    /// When the file is not this family's.
    pub fn save_checkpoint(&mut self, file: &str, note: &str) -> Result<Outcome> {
        let bytes = self.storage.read_file(file)?;
        let state = self.state()?;
        let node = Self::node_of(&state, file)?;
        let outcome = self.press(|ctx, state, blobs| {
            commands::save_checkpoint(ctx, state, &node, &bytes, note, blobs)
        })?;
        self.write_outcome(file, &outcome)?;
        Ok(outcome)
    }

    /// "Make a tailored version". The new file is saved next to the master as `name`.
    ///
    /// # Errors
    /// When a file of that name exists, or the master cannot be restored.
    pub fn make_tailored_version(&mut self, name: &str) -> Result<String> {
        if self.storage.file_names()?.iter().any(|n| n == name) {
            return Err(HarnessError::Panel(format!("{name} already exists")));
        }
        let outcome = self.press(|ctx, state, blobs| {
            commands::make_tailored_version(ctx, state, name, name, blobs)
        })?;
        let new = outcome
            .new_file
            .ok_or_else(|| HarnessError::Panel("no file made".to_owned()))?;
        self.storage.write_file(name, &new.bytes)?;
        Ok(name.to_owned())
    }

    /// "What changed" since the file's last checkpoint.
    ///
    /// # Errors
    /// When the file is not this family's.
    pub fn show_what_changed(&self, file: &str) -> Result<WhatChanged> {
        let bytes = self.storage.read_file(file)?;
        let state = self.state()?;
        let node = Self::node_of(&state, file)?;
        let family = self.family()?;
        let blobs = StoredBlobs {
            storage: &self.storage,
            family,
        };
        Ok(commands::what_changed(&state, &node, &bytes, &blobs)?)
    }

    /// The master panel's "Offer it to …" button.
    ///
    /// # Errors
    /// When there is nothing to offer.
    pub fn offer_to_versions(&mut self) -> Result<MasterPanel> {
        let panel = self.master_panel()?;
        let callout = panel
            .callout
            .ok_or_else(|| HarnessError::Panel("nothing to offer".to_owned()))?;
        let versions = callout.versions;
        self.press(|ctx, state, blobs| commands::offer_to_versions(ctx, state, &versions, blobs))?;
        self.master_panel()
    }

    /// The updates a tailored version has been offered.
    ///
    /// # Errors
    /// When the file is not a tailored version of this family.
    pub fn updates(&self, file: &str) -> Result<Updates> {
        let bytes = self.storage.read_file(file)?;
        let state = self.state()?;
        let node = Self::node_of(&state, file)?;
        let family = self.family()?;
        let blobs = StoredBlobs {
            storage: &self.storage,
            family,
        };
        Ok(commands::updates(&state, &node, &bytes, &blobs)?)
    }

    /// Presses buttons on update cards: `decisions` are (card id, decision).
    ///
    /// # Errors
    /// When a card is not on the panel, or a change cannot be written safely.
    pub fn decide(&mut self, file: &str, decisions: &[(&str, Decision)]) -> Result<Outcome> {
        let cards: Vec<(String, Decision)> = decisions
            .iter()
            .map(|(card, decision)| ((*card).to_owned(), *decision))
            .collect();
        let bytes = self.storage.read_file(file)?;
        let state = self.state()?;
        let node = Self::node_of(&state, file)?;
        let outcome = self.press(|ctx, state, blobs| {
            commands::decide_cards(ctx, state, &node, &bytes, &cards, blobs)
        })?;
        self.write_outcome(file, &outcome)?;
        Ok(outcome)
    }

    /// "Bring in" on every listed card.
    ///
    /// # Errors
    /// See [`Panel::decide`].
    pub fn bring_in(&mut self, file: &str, cards: &[&str]) -> Result<Outcome> {
        let decisions: Vec<(&str, Decision)> =
            cards.iter().map(|c| (*c, Decision::BringIn)).collect();
        self.decide(file, &decisions)
    }

    /// "Decline" on every listed card.
    ///
    /// # Errors
    /// See [`Panel::decide`].
    pub fn decline(&mut self, file: &str, cards: &[&str]) -> Result<Outcome> {
        let decisions: Vec<(&str, Decision)> =
            cards.iter().map(|c| (*c, Decision::Decline)).collect();
        self.decide(file, &decisions)
    }

    /// "Send a copy" → "Save copy and record it". Returns the saved copy's file name.
    ///
    /// # Errors
    /// When the file is not this family's, or a clean copy cannot be made.
    pub fn send_a_copy(
        &mut self,
        file: &str,
        recipient: &str,
        purpose: Option<&str>,
        include_id: bool,
    ) -> Result<String> {
        let bytes = self.storage.read_file(file)?;
        let state = self.state()?;
        let node = Self::node_of(&state, file)?;
        let outcome = self.press(|ctx, state, blobs| {
            commands::send_copy(
                ctx, state, &node, &bytes, recipient, purpose, include_id, blobs,
            )
        })?;
        self.write_outcome(file, &outcome)?;
        let copy = outcome
            .new_file
            .ok_or_else(|| HarnessError::Panel("no copy made".to_owned()))?;
        self.storage.write_file(&copy.name, &copy.bytes)?;
        Ok(copy.name)
    }

    /// "What I've sent".
    ///
    /// # Errors
    /// When the history cannot be read.
    pub fn what_ive_sent(&self, search: Option<&str>) -> Result<SentList> {
        Ok(views::sent_list(&self.state()?, search))
    }

    /// "A file came back": which copy `file` is, looking in every family in
    /// storage. The panel draws the view; nothing is stored yet.
    ///
    /// # Errors
    /// When the file can't be read or a history is damaged.
    pub fn a_file_came_back(&self, file: &str) -> Result<(Recognition, RecognitionView)> {
        let bytes = self.storage.read_file(file)?;
        let mut states = Vec::new();
        for family in self.storage.families()? {
            let (ledger, _) = self.storage.read_ledger(&family)?;
            states.push(FamilyState::from_records(&event::decode(&ledger)?)?);
        }
        let sources: Vec<StoredBlobs<'_, S>> = states
            .iter()
            .map(|s| StoredBlobs {
                storage: &self.storage,
                family: &s.family_id,
            })
            .collect();
        let families: Vec<Family<'_>> = states
            .iter()
            .zip(&sources)
            .map(|(state, blobs)| Family { state, blobs })
            .collect();
        let found = returned::recognise(&bytes, &families)?;
        let refs: Vec<&FamilyState> = states.iter().collect();
        let view = returned::describe(&found, &refs);
        Ok((found, view))
    }

    /// "Review the changes" on the copy the panel recognised, or the one the
    /// person chose: keeps `file` exactly as it came back. Opens that family.
    ///
    /// # Errors
    /// When the copy is unknown, or `how` is ids but the file's ids don't name it.
    pub fn record_returned(
        &mut self,
        file: &str,
        candidate: &Candidate,
        how: MatchedBy,
    ) -> Result<Id> {
        let bytes = self.storage.read_file(file)?;
        self.family = Some(candidate.family.clone());
        let outcome = self.press(|ctx, state, blobs| {
            returned::record(
                ctx,
                state,
                &bytes,
                file,
                &candidate.checkpoint,
                candidate.sent.as_ref(),
                how,
                blobs,
            )
        })?;
        outcome
            .created
            .ok_or_else(|| HarnessError::Panel("nothing recorded".to_owned()))
    }

    /// The review of a returned file, in `into` (the file open in Word).
    ///
    /// # Errors
    /// When `into` is not a file of this family, or blobs are missing.
    pub fn returned_review(&self, into: &str, returned: &Id) -> Result<ReturnReview> {
        let bytes = self.storage.read_file(into)?;
        let state = self.state()?;
        let node = Self::node_of(&state, into)?;
        let family = self.family()?;
        let blobs = StoredBlobs {
            storage: &self.storage,
            family,
        };
        Ok(returned::review(&state, returned, &node, &bytes, &blobs)?)
    }

    /// "Apply" on a review: `decisions` are (card id, decision). Kept changes
    /// go into `into` as tracked changes by Lineage, words only when the review
    /// says so (the panel's "Bring in their formatting too" left unticked).
    ///
    /// # Errors
    /// When a card isn't waiting for a decision, or a change can't be written safely.
    pub fn keep_and_skip(
        &mut self,
        into: &str,
        returned: &Id,
        decisions: &[(&str, Decision)],
    ) -> Result<Outcome> {
        let bring = if self.returned_review(into, returned)?.formatting.words_only {
            Bring::WordsOnly
        } else {
            Bring::Everything
        };
        self.keep_and_skip_with(into, returned, decisions, bring)
    }

    /// [`Panel::keep_and_skip`], with "Bring in their formatting too" set by `bring`.
    ///
    /// # Errors
    /// As [`Panel::keep_and_skip`].
    pub fn keep_and_skip_with(
        &mut self,
        into: &str,
        returned: &Id,
        decisions: &[(&str, Decision)],
        bring: Bring,
    ) -> Result<Outcome> {
        let cards: Vec<(String, Decision)> = decisions
            .iter()
            .map(|(card, decision)| ((*card).to_owned(), *decision))
            .collect();
        let bytes = self.storage.read_file(into)?;
        let state = self.state()?;
        let node = Self::node_of(&state, into)?;
        let outcome = self.press(|ctx, state, blobs| {
            returned::decide(ctx, state, returned, &node, &bytes, &cards, bring, blobs)
        })?;
        self.write_outcome(into, &outcome)?;
        Ok(outcome)
    }

    /// "Open what came back": the exact bytes of a returned file.
    ///
    /// # Errors
    /// When the returned file is unknown or its stored bytes are missing.
    pub fn open_what_came_back(&self, returned: &Id) -> Result<Vec<u8>> {
        let state = self.state()?;
        let family = self.family()?;
        let blobs = StoredBlobs {
            storage: &self.storage,
            family,
        };
        Ok(returned::open(&state, returned, &blobs)?)
    }

    /// "Open" on a sent copy: the exact bytes they received.
    ///
    /// # Errors
    /// When the copy is unknown or its stored bytes are missing.
    pub fn open_what_they_received(&self, sent: &Id) -> Result<Vec<u8>> {
        let state = self.state()?;
        let family = self.family()?;
        let blobs = StoredBlobs {
            storage: &self.storage,
            family,
        };
        Ok(commands::open_sent_copy(&state, sent, &blobs)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_against_the_family_crate() {
        assert_eq!(engine_version(), lineage::ENGINE_VERSION);
    }
}
