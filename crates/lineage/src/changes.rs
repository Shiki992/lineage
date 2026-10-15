// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Change states (decision 0002): what each tailored version did with each
//! master change, the merge base, and content detection.
//!
//! - Every change in a master checkpoint after the one a version was made from
//!   is merged, declined or pending for that version.
//! - The merge base is the newest master checkpoint whose changes are all merged
//!   or declined (or the one the version was made from). Changes after it are
//!   judged one by one, so a version can take v3 without v2.
//! - Pending changes newer than the merge base are offered. Pending changes older
//!   than it are quiet: still in history, but not offered and never notified.
//! - Detection: a pending change the version already contains is tagged merged
//!   (detected) and the version is linked to that checkpoint. A merged change the
//!   version no longer contains (rejected in Word) is tagged declined (detected).

use lineage_core::Docx;
use lineage_core::diff::spec::ChangeList;
use lineage_core::merge::{self, DETECTED_CONFIDENCE, Status};

use crate::blob::{BlobSource, restore};
use crate::error::{Error, Result};
use crate::event::{ChangeState, Event, How};
use crate::ids::Id;
use crate::state::{Checkpoint, FamilyState};

/// One master change, as stored with its checkpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredChange {
    pub id: String,
    pub summary: Option<String>,
    pub formatting_only: bool,
}

/// The changes a master checkpoint made since its parent.
///
/// # Errors
/// When the change list blob is missing or damaged.
pub fn stored_changes(
    checkpoint: &Checkpoint,
    blobs: &dyn BlobSource,
) -> Result<Vec<StoredChange>> {
    let Some(hash) = &checkpoint.changes else {
        return Ok(Vec::new());
    };
    let json = blobs
        .blob(hash)
        .ok_or_else(|| Error::MissingBlob(hash.as_str().to_owned()))?;
    let list: ChangeList =
        serde_json::from_slice(&json).map_err(|e| Error::Ledger(e.to_string()))?;
    Ok(list
        .changes
        .into_iter()
        .map(|c| StoredChange {
            id: c.id,
            summary: c.summary,
            formatting_only: c.formatting_only,
        })
        .collect())
}

/// A master change not yet decided for a version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pending {
    pub checkpoint: Id,
    pub change: StoredChange,
    /// True once it has been offered to the version.
    pub offered: bool,
}

/// Where a version stands against the master.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VariantStatus {
    /// The merge base: the newest master checkpoint this version is caught up with.
    pub base: Id,
    /// Pending changes newer than the base: offered and notified.
    pub offerable: Vec<Pending>,
    /// Pending changes older than the base: quiet.
    pub quiet: Vec<Pending>,
    pub merged: Vec<(Id, String)>,
    pub declined: Vec<(Id, String)>,
}

fn decided(
    state: &FamilyState,
    variant: &Id,
    checkpoint: &Id,
    change: &str,
) -> Option<ChangeState> {
    state
        .change_state(variant, checkpoint, change)
        .map(|r| r.state)
        .filter(|s| *s != ChangeState::Pending)
}

/// The index of the master checkpoint a version was made from.
fn origin(state: &FamilyState, variant: &Id) -> Result<usize> {
    let node = state
        .node(variant)
        .ok_or_else(|| Error::NotAllowed("unknown version".to_owned()))?;
    let from = node
        .made_from
        .as_ref()
        .ok_or_else(|| Error::NotAllowed("this file was not made from the master".to_owned()))?;
    state
        .master_checkpoints()
        .iter()
        .position(|c| &c.id == from)
        .ok_or_else(|| {
            Error::NotAllowed("this version was not made from a master checkpoint".to_owned())
        })
}

/// Where a version stands: its merge base and every master change after the
/// checkpoint it was made from.
///
/// # Errors
/// When a change list blob is missing.
pub fn variant_status(
    state: &FamilyState,
    variant: &Id,
    blobs: &dyn BlobSource,
) -> Result<VariantStatus> {
    let masters = state.master_checkpoints();
    let start = origin(state, variant)?;
    let mut lists = Vec::new();
    for checkpoint in &masters[start + 1..] {
        lists.push(((*checkpoint).clone(), stored_changes(checkpoint, blobs)?));
    }
    let mut base_index = start;
    for (offset, (checkpoint, changes)) in lists.iter().enumerate() {
        if changes
            .iter()
            .all(|c| decided(state, variant, &checkpoint.id, &c.id).is_some())
        {
            base_index = start + 1 + offset;
        }
    }
    let base = masters[base_index].id.clone();
    let mut status = VariantStatus {
        base,
        offerable: Vec::new(),
        quiet: Vec::new(),
        merged: Vec::new(),
        declined: Vec::new(),
    };
    for (offset, (checkpoint, changes)) in lists.into_iter().enumerate() {
        let index = start + 1 + offset;
        for change in changes {
            match decided(state, variant, &checkpoint.id, &change.id) {
                Some(ChangeState::Merged) => status.merged.push((checkpoint.id.clone(), change.id)),
                Some(ChangeState::Declined) => {
                    status.declined.push((checkpoint.id.clone(), change.id))
                }
                _ => {
                    let offered = state.offers.iter().any(|o| {
                        o.to == *variant
                            && o.from == checkpoint.id
                            && o.change_ids.contains(&change.id)
                    });
                    let pending = Pending {
                        checkpoint: checkpoint.id.clone(),
                        change,
                        offered,
                    };
                    if index > base_index {
                        status.offerable.push(pending);
                    } else {
                        status.quiet.push(pending);
                    }
                }
            }
        }
    }
    Ok(status)
}

/// The base and later document of a master checkpoint's changes.
pub(crate) fn checkpoint_pair(
    state: &FamilyState,
    checkpoint: &Checkpoint,
    blobs: &dyn BlobSource,
) -> Result<(Docx, Docx)> {
    let parent = checkpoint
        .parents
        .first()
        .and_then(|p| state.checkpoint(p))
        .ok_or_else(|| {
            Error::Ledger("a master checkpoint without a parent has no changes".to_owned())
        })?;
    let before = Docx::open(&restore(&parent.snapshot, blobs)?)?;
    let after = Docx::open(&restore(&checkpoint.snapshot, blobs)?)?;
    Ok((before, after))
}

/// Detection for a version's new content (decision 0002 and 0003). `checkpoint`
/// is the version's checkpoint being saved, which links are recorded against.
///
/// # Errors
/// When snapshots are missing or a file cannot be read.
pub fn detect(
    state: &FamilyState,
    variant: &Id,
    ours: &Docx,
    checkpoint: &Id,
    blobs: &dyn BlobSource,
) -> Result<Vec<Event>> {
    let masters = state.master_checkpoints();
    let start = origin(state, variant)?;
    let mut events = Vec::new();
    for master in &masters[start + 1..] {
        let changes = stored_changes(master, blobs)?;
        let interesting: Vec<&StoredChange> = changes
            .iter()
            .filter(|c| decided(state, variant, &master.id, &c.id) != Some(ChangeState::Declined))
            .collect();
        if interesting.is_empty() {
            continue;
        }
        let (before, after) = checkpoint_pair(state, master, blobs)?;
        let plan = merge::plan(&before, ours, &after);
        let mut all_merged = true;
        let mut detected_any = false;
        for change in &changes {
            let current = decided(state, variant, &master.id, &change.id);
            let item = plan.item(&change.id);
            let found = item.is_some_and(|i| {
                matches!(i.status, Status::AlreadyThere { confidence } if confidence >= DETECTED_CONFIDENCE)
            });
            let gone = item.is_some_and(|i| i.status == Status::Clean);
            match current {
                None if found => {
                    events.push(Event::ChangeStateRecorded {
                        node_id: variant.clone(),
                        change_id: change.id.clone(),
                        from_checkpoint_id: master.id.clone(),
                        state: ChangeState::Merged,
                        how: How::Detected,
                    });
                    detected_any = true;
                }
                Some(ChangeState::Merged) if gone => {
                    events.push(Event::ChangeStateRecorded {
                        node_id: variant.clone(),
                        change_id: change.id.clone(),
                        from_checkpoint_id: master.id.clone(),
                        state: ChangeState::Declined,
                        how: How::Detected,
                    });
                    all_merged = false;
                }
                Some(ChangeState::Merged) => {}
                _ => all_merged = false,
            }
        }
        if detected_any && all_merged {
            events.push(Event::CheckpointLinked {
                checkpoint_id: checkpoint.clone(),
                master_checkpoint_id: master.id.clone(),
                how: How::Detected,
            });
        }
    }
    Ok(events)
}
