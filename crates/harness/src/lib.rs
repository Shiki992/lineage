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

pub mod accounts;
pub mod storage;

use std::fmt;

use lineage::blob::{BlobHash, BlobSource};
use lineage::commands::{self, Context, Outcome, Updates};
use lineage::event::{self, MatchedBy};
use lineage::graph::{self, FamilyGraph};
use lineage::index;
use lineage::library::Library;
use lineage::read::read_lenient;
use lineage::returned::{self, Candidate, Family, Recognition, RecognitionView, ReturnReview};
use lineage::state::FamilyState;
use lineage::views::{self, DocumentPage, MasterPanel, SentList, VersionPanel};
use lineage::{Id, SequentialIds, Timestamp};
use lineage_core::merge::Decision;
use lineage_core::view::WhatChanged;
pub use lineage_core::write::Bring;
pub use lineage_core::xlsx::apply::Step;

pub use storage::{LocalFolderStorage, MemoryStorage, Storage, StorageError, Tamper};

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

    /// The panel's clock: the time the next button press is recorded at.
    #[must_use]
    pub fn now(&self) -> Timestamp {
        self.now
    }

    /// Sets the panel's clock, for accounts whose families were made at
    /// different times.
    pub fn set_now(&mut self, now: Timestamp) {
        self.now = now;
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
                Ok(written) => {
                    self.index_after(&family, written, &[bytes.as_slice(), &lines].concat());
                    self.now = Timestamp(self.now.0 + 60_000);
                    return Ok(outcome);
                }
                Err(StorageError::Conflict) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Err(HarnessError::Storage(StorageError::Conflict))
    }

    /// Brings the library index up to date after a ledger write, as the task
    /// pane does. Only a copy: a failure is left for the next "My documents".
    fn index_after(&mut self, family: &Id, version: storage::Version, ledger: &[u8]) {
        for _attempt in 0..2 {
            let Ok(current) = self.storage.read_index() else {
                return;
            };
            let next = index::update(
                current.as_ref().map(|(b, _)| b.as_slice()),
                family,
                &version.to_string(),
                ledger,
            );
            match self
                .storage
                .write_index(&next, current.as_ref().map(|(_, v)| v.as_str()))
            {
                Err(StorageError::Conflict) => {}
                _ => return,
            }
        }
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
        let lines = event::encode(&outcome.records)?;
        let written = self.storage.append_ledger(&family, 0, &lines)?;
        self.index_after(&family, written, &lines);
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

    /// The pitch deck panel of the master deck. With `open`, the master file as
    /// it is now (the deck open in PowerPoint), to show what just changed.
    ///
    /// # Errors
    /// When the family isn't a deck, or the history can't be read.
    pub fn deck_panel(&self, open: Option<&str>) -> Result<lineage::decks::DeckPanel> {
        let state = self.state()?;
        let bytes = match open {
            Some(file) => Some(self.storage.read_file(file)?),
            None => None,
        };
        let family = self.family()?;
        let blobs = StoredBlobs {
            storage: &self.storage,
            family,
        };
        Ok(lineage::decks::deck_panel(
            &state,
            bytes.as_deref(),
            &blobs,
        )?)
    }

    /// "Update N versions" on the pitch deck panel, for the tailored versions
    /// in `files`, with the person's `choices` on cards a previous run left
    /// (file, card id, decision). Each version's file is written only if it
    /// hasn't changed since it was read, then the history is appended once; if
    /// that append fails, every file is put back as it was.
    ///
    /// # Errors
    /// When a file changed while the update ran, or the history can't be written.
    pub fn update_versions(
        &mut self,
        files: &[&str],
        choices: &[(&str, &str, Decision)],
    ) -> Result<lineage::decks::UpdateRun> {
        let family = self.family()?.clone();
        let (ledger, version) = self.storage.read_ledger(&family)?;
        let state = FamilyState::from_records(&event::decode(&ledger)?)?;
        let mut inputs = Vec::new();
        for file in files {
            inputs.push((Self::node_of(&state, file)?, self.storage.read_file(file)?));
        }
        let mut picked = Vec::new();
        for (file, card, decision) in choices {
            picked.push((Self::node_of(&state, file)?, (*card).to_owned(), *decision));
        }
        let run = {
            let blobs = StoredBlobs {
                storage: &self.storage,
                family: &family,
            };
            let mut ctx = Context {
                now: self.now,
                ids: &mut self.ids,
                author: None,
            };
            lineage::decks::update_versions(&mut ctx, &state, &inputs, &picked, &blobs)?
        };
        // Files first, each only if nobody changed it meanwhile.
        let mut written: Vec<(&str, Vec<u8>)> = Vec::new();
        for ((file, (_, before)), line) in files.iter().zip(&inputs).zip(&run.versions) {
            let Some(bytes) = &line.file else {
                continue;
            };
            if self.storage.read_file(file)? != *before {
                for (done, original) in &written {
                    self.storage.write_file(done, original)?;
                }
                return Err(HarnessError::Storage(StorageError::Conflict));
            }
            self.storage.write_file(file, bytes)?;
            written.push((file, before.clone()));
        }
        for blob in &run.outcome.blobs {
            self.storage.put_blob(&family, blob)?;
        }
        let lines = event::encode(&run.outcome.records)?;
        match self.storage.append_ledger(&family, version, &lines) {
            Ok(at) => {
                self.index_after(&family, at, &[ledger.as_slice(), &lines].concat());
                self.now = Timestamp(self.now.0 + 60_000);
                Ok(run)
            }
            Err(e) => {
                for (done, original) in &written {
                    self.storage.write_file(done, original)?;
                }
                Err(e.into())
            }
        }
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
        // "Tailored versions/Resume.docx" is shown as "Resume.docx".
        let display = name.rsplit('/').next().unwrap_or(name);
        let outcome = self.press(|ctx, state, blobs| {
            commands::make_tailored_version(ctx, state, display, name, blobs)
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

    // -------------------------------------------------------------- Workbooks

    /// "Bring in" on a tailored workbook's updates, the way the Excel pane does
    /// it (decision 0023): the engine plans, `excel` carries the steps out on
    /// the open workbook, and the engine checks what Excel then holds before
    /// anything is recorded. If the check fails, the workbook is put back as it
    /// was, and the error says so.
    ///
    /// # Errors
    /// When a card isn't on the panel, the changes didn't land, or the history
    /// can't be written.
    pub fn bring_into_workbook<E>(
        &mut self,
        file: &str,
        decisions: &[(&str, Decision)],
        excel: E,
    ) -> Result<lineage::workbooks::WorkbookPlan>
    where
        E: FnOnce(&[u8], &[Step]) -> std::result::Result<Vec<u8>, String>,
    {
        let decisions = owned(decisions);
        let original = self.storage.read_file(file)?;
        let state = self.state()?;
        let node = Self::node_of(&state, file)?;
        let plan = {
            let family = self.family()?;
            let blobs = StoredBlobs {
                storage: &self.storage,
                family,
            };
            lineage::workbooks::prepare(&state, &node, &original, &decisions, &blobs)?
        };
        let held = self.carry_out(file, &original, &plan, excel)?;
        let outcome = self.press(|ctx, state, blobs| {
            lineage::workbooks::finish(ctx, state, &node, &original, &held, &decisions, blobs)
        });
        self.landed(file, &original, outcome)?;
        Ok(plan)
    }

    /// "Keep" on a returned workbook's review, into the open workbook `into`:
    /// as [`Panel::bring_into_workbook`].
    ///
    /// # Errors
    /// As [`Panel::bring_into_workbook`].
    pub fn keep_into_workbook<E>(
        &mut self,
        into: &str,
        returned: &Id,
        decisions: &[(&str, Decision)],
        excel: E,
    ) -> Result<lineage::workbooks::WorkbookPlan>
    where
        E: FnOnce(&[u8], &[Step]) -> std::result::Result<Vec<u8>, String>,
    {
        let cards = owned(decisions);
        let original = self.storage.read_file(into)?;
        let state = self.state()?;
        let node = Self::node_of(&state, into)?;
        let plan = {
            let family = self.family()?;
            let blobs = StoredBlobs {
                storage: &self.storage,
                family,
            };
            lineage::workbooks::prepare_keep(&state, returned, &node, &original, &cards, &blobs)?
        };
        let held = self.carry_out(into, &original, &plan, excel)?;
        let outcome = self.press(|ctx, state, blobs| {
            lineage::workbooks::finish_keep(
                ctx, state, returned, &node, &original, &held, &cards, blobs,
            )
        });
        self.landed(into, &original, outcome)?;
        Ok(plan)
    }

    /// Excel carries out `plan` on the open workbook `file`.
    fn carry_out<E>(
        &mut self,
        file: &str,
        original: &[u8],
        plan: &lineage::workbooks::WorkbookPlan,
        excel: E,
    ) -> Result<Vec<u8>>
    where
        E: FnOnce(&[u8], &[Step]) -> std::result::Result<Vec<u8>, String>,
    {
        if plan.steps.is_empty() {
            return Ok(original.to_vec());
        }
        match excel(original, &plan.steps) {
            Ok(held) => {
                self.storage.write_file(file, &held)?;
                Ok(held)
            }
            Err(why) => {
                // The pane runs the plan's undo; here, the file as it was.
                self.storage.write_file(file, original)?;
                Err(HarnessError::Panel(format!("Excel stopped: {why}")))
            }
        }
    }

    /// After `finish`: the checkpoint's file (with its ids) is written, or on
    /// any failure the workbook is put back as it was.
    fn landed(&mut self, file: &str, original: &[u8], outcome: Result<Outcome>) -> Result<()> {
        match outcome {
            Ok(outcome) => self.write_outcome(file, &outcome),
            Err(e) => {
                self.storage.write_file(file, original)?;
                Err(e)
            }
        }
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

    // ------------------------------------------------------------ My documents

    /// "My documents": every family, from the index where it is right and from
    /// the ledgers where it isn't; a wrong or missing index is rewritten.
    ///
    /// # Errors
    /// When storage can't be read.
    pub fn my_documents(&mut self) -> Result<Library> {
        let index = self.storage.read_index()?;
        let families = self.storage.families()?;
        let observed: std::collections::BTreeMap<Id, String> = self
            .storage
            .ledger_versions(&families)?
            .into_iter()
            .map(|(f, v)| (f, v.to_string()))
            .collect();
        let stale = index::stale(index.as_ref().map(|(b, _)| b.as_slice()), &observed);
        let mut ledgers = Vec::new();
        for family in stale {
            let (bytes, _) = self.storage.read_ledger(&family)?;
            ledgers.push((family, bytes));
        }
        let given: Vec<(Id, &[u8])> = ledgers
            .iter()
            .map(|(f, b)| (f.clone(), b.as_slice()))
            .collect();
        let refreshed =
            index::refresh(index.as_ref().map(|(b, _)| b.as_slice()), &observed, &given).map_err(
                |missing| HarnessError::Panel(format!("{} ledgers not read", missing.len())),
            )?;
        if refreshed.changed {
            match self
                .storage
                .write_index(&refreshed.index, index.as_ref().map(|(_, v)| v.as_str()))
            {
                Ok(_) | Err(StorageError::Conflict) => {}
                Err(e) => return Err(e.into()),
            }
        }
        Ok(refreshed.library)
    }

    /// A document's page and family tree, from as much of its ledger as can be read.
    ///
    /// # Errors
    /// When nothing of the ledger can be read.
    pub fn document(&self, family: &Id) -> Result<(DocumentPage, FamilyGraph)> {
        let (bytes, _) = self.storage.read_ledger(family)?;
        let read = read_lenient(&bytes);
        let state = read
            .state
            .as_ref()
            .ok_or_else(|| HarnessError::Panel("the history can't be read".to_owned()))?;
        Ok((
            views::document_page(state, read.note()),
            graph::family_graph(state),
        ))
    }

    fn folded(&self, family: &Id) -> Result<FamilyState> {
        let (bytes, _) = self.storage.read_ledger(family)?;
        Ok(FamilyState::from_records(&event::decode(&bytes)?)?)
    }

    /// "Compare" two checkpoints of a family.
    ///
    /// # Errors
    /// When a checkpoint is unknown or its snapshot missing.
    pub fn compare(&self, family: &Id, a: &Id, b: &Id) -> Result<WhatChanged> {
        let state = self.folded(family)?;
        let blobs = StoredBlobs {
            storage: &self.storage,
            family,
        };
        Ok(commands::compare(&state, a, b, &blobs)?)
    }

    /// "Open as sent" in My documents: the exact bytes of a sent copy.
    ///
    /// # Errors
    /// When the copy is unknown or its blob missing.
    pub fn open_as_sent(&self, family: &Id, sent: &Id) -> Result<Vec<u8>> {
        let state = self.folded(family)?;
        let blobs = StoredBlobs {
            storage: &self.storage,
            family,
        };
        Ok(commands::open_sent_copy(&state, sent, &blobs)?)
    }

    /// "Restore as a new version": saved as `name` (which may be in
    /// `Tailored versions/`). Returns the new file's node.
    ///
    /// # Errors
    /// When a file of that name exists, or the checkpoint can't be restored.
    pub fn restore_as_new_version(
        &mut self,
        family: &Id,
        checkpoint: &Id,
        name: &str,
    ) -> Result<Id> {
        if self.storage.read_file(name).is_ok() {
            return Err(HarnessError::Panel(format!("{name} already exists")));
        }
        self.family = Some(family.clone());
        let display = name.rsplit('/').next().unwrap_or(name);
        let outcome = self.press(|ctx, state, blobs| {
            commands::restore_as_new_version(ctx, state, checkpoint, display, name, blobs)
        })?;
        let new = outcome
            .new_file
            .ok_or_else(|| HarnessError::Panel("no file made".to_owned()))?;
        self.storage.write_file(name, &new.bytes)?;
        outcome
            .created
            .ok_or_else(|| HarnessError::Panel("no version made".to_owned()))
    }

    /// "Mark as milestone"; `None` takes the mark off.
    ///
    /// # Errors
    /// When the checkpoint is unknown or the label too long.
    pub fn mark_milestone(
        &mut self,
        family: &Id,
        checkpoint: &Id,
        label: Option<&str>,
    ) -> Result<()> {
        self.family = Some(family.clone());
        self.press(|ctx, state, _| commands::mark_milestone(ctx, state, checkpoint, label))?;
        Ok(())
    }

    /// "Choose the file" for a version whose file moved: it is `file` now.
    ///
    /// # Errors
    /// When `file` isn't one of the version's own checkpoints.
    pub fn relink(&mut self, family: &Id, node: &Id, file: &str) -> Result<()> {
        let bytes = self.storage.read_file(file)?;
        self.family = Some(family.clone());
        self.press(|ctx, state, _| commands::relink(ctx, state, node, file, &bytes))?;
        Ok(())
    }
}

fn owned(decisions: &[(&str, Decision)]) -> Vec<(String, Decision)> {
    decisions
        .iter()
        .map(|(card, decision)| ((*card).to_owned(), *decision))
        .collect()
}

/// Excel, as the harness has it: the Rust writer carries out the plan's steps
/// the way Excel's own row insert and cell writes do (decision 0023).
///
/// # Errors
/// When the writer refuses a step.
pub fn fake_excel(bytes: &[u8], steps: &[Step]) -> std::result::Result<Vec<u8>, String> {
    lineage_core::xlsx::write::apply_steps(bytes, steps).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_against_the_family_crate() {
        assert_eq!(engine_version(), lineage::ENGINE_VERSION);
    }
}
