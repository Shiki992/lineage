// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The current state of a family, folded from its ledger. Folding checks every
//! record: sequence numbers without gaps, references to things that exist, one
//! master. A ledger that breaks a rule is reported, never half-read.

use std::collections::BTreeMap;

use crate::blob::BlobHash;
use crate::error::{Error, Result};
use crate::event::{
    ChangeState, CleanSummary, Event, How, MILESTONE_MAX, MatchedBy, RECORD_VERSION, Record, Role,
};
use crate::identity::Fingerprint;
use crate::ids::{Id, Timestamp};

/// A real file in the family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub id: Id,
    pub role: Role,
    pub display_name: String,
    pub storage_ref: String,
    pub made_from: Option<Id>,
    pub follows_master: bool,
    /// Its checkpoints, oldest first.
    pub checkpoints: Vec<Id>,
    pub archived: bool,
}

/// A saved state of one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub id: Id,
    pub node: Id,
    pub parents: Vec<Id>,
    pub snapshot: BlobHash,
    pub fingerprint: Fingerprint,
    pub note: String,
    pub author: Option<String>,
    pub at: Timestamp,
    pub changes: Option<BlobHash>,
    /// 1 for the node's first checkpoint, and so on: shown as "v1".
    pub number: usize,
    /// "Sent to ten companies", once marked.
    pub milestone: Option<String>,
    /// For a workbook: whether it holds macros; `None` before Phase 5B.
    pub macros: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Offer {
    pub id: Id,
    pub from: Id,
    pub to: Id,
    pub change_ids: Vec<String>,
    pub at: Timestamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChangeRecord {
    pub state: ChangeState,
    pub how: How,
    pub at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentCopy {
    pub id: Id,
    pub from: Id,
    pub recipient: String,
    pub purpose: Option<String>,
    pub format: String,
    pub file: BlobHash,
    pub id_embedded: bool,
    pub at: Timestamp,
    pub cleaned: CleanSummary,
}

/// A file that came back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Returned {
    pub id: Id,
    pub sent: Option<Id>,
    pub from: Id,
    pub display_name: String,
    pub file: BlobHash,
    pub matched_by: MatchedBy,
    pub confidence: u8,
    pub change_count: usize,
    pub formatting_count: usize,
    /// Came back with new or changed macro code (Phase 5B).
    pub new_macro_code: bool,
    pub at: Timestamp,
    /// Each sitting in which the person kept or skipped its changes.
    pub reviews: Vec<Reviewed>,
}

impl Returned {
    /// Every change already kept or skipped.
    pub fn decided(&self) -> impl Iterator<Item = &String> {
        self.reviews
            .iter()
            .flat_map(|r| r.kept.iter().chain(&r.skipped))
    }

    /// True once nothing is left to decide.
    #[must_use]
    pub fn finished(&self) -> bool {
        self.reviews.iter().any(|r| r.finished)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reviewed {
    pub into: Id,
    pub result_checkpoint: Option<Id>,
    pub kept: Vec<String>,
    pub skipped: Vec<String>,
    pub finished: bool,
    pub at: Timestamp,
}

/// Key of a change state: (version, master checkpoint, change id).
pub type ChangeKey = (Id, Id, String);

/// Everything known about a family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FamilyState {
    pub family_id: Id,
    pub name: String,
    pub format: String,
    pub nodes: Vec<Node>,
    /// Every checkpoint, in the order they were saved.
    pub checkpoints: Vec<Checkpoint>,
    pub offers: Vec<Offer>,
    /// The latest state of each (version, checkpoint, change).
    pub change_states: BTreeMap<ChangeKey, ChangeRecord>,
    /// (version checkpoint, master checkpoint) descendant links found by content.
    pub links: Vec<(Id, Id)>,
    pub sent: Vec<SentCopy>,
    pub returned: Vec<Returned>,
    pub last_seq: u64,
    pub last_at: Timestamp,
}

fn bad(detail: impl Into<String>) -> Error {
    Error::Ledger(detail.into())
}

impl FamilyState {
    /// Folds a whole ledger.
    ///
    /// # Errors
    /// [`Error::Ledger`] when any record breaks a rule.
    pub fn from_records(records: &[Record]) -> Result<Self> {
        let first = records.first().ok_or_else(|| bad("the ledger is empty"))?;
        let Event::FamilyStarted {
            family_id,
            name,
            format,
        } = &first.event
        else {
            return Err(bad("the ledger does not start with FamilyStarted"));
        };
        if first.seq != 1 || first.v != RECORD_VERSION {
            return Err(bad("the first record must be number 1"));
        }
        let mut state = Self {
            family_id: family_id.clone(),
            name: name.clone(),
            format: format.clone(),
            nodes: Vec::new(),
            checkpoints: Vec::new(),
            offers: Vec::new(),
            change_states: BTreeMap::new(),
            links: Vec::new(),
            sent: Vec::new(),
            returned: Vec::new(),
            last_seq: 1,
            last_at: first.at,
        };
        for record in &records[1..] {
            state.apply(record)?;
        }
        Ok(state)
    }

    /// The next sequence number to write.
    #[must_use]
    pub fn next_seq(&self) -> u64 {
        self.last_seq + 1
    }

    /// Applies one more record.
    ///
    /// # Errors
    /// [`Error::Ledger`] when the record breaks a rule. The state is unchanged then.
    #[allow(clippy::too_many_lines)]
    pub fn apply(&mut self, record: &Record) -> Result<()> {
        if record.v != RECORD_VERSION {
            return Err(bad(format!("record {} has an unknown version", record.seq)));
        }
        if record.seq != self.last_seq + 1 {
            return Err(bad(format!(
                "record {} follows {}: a record is missing or repeated",
                record.seq, self.last_seq
            )));
        }
        let at = record.at;
        match &record.event {
            Event::FamilyStarted { .. } => return Err(bad("a second FamilyStarted")),
            Event::NodeAdded {
                node_id,
                role,
                display_name,
                storage_ref,
                made_from_checkpoint_id,
                follows_master,
            } => {
                if self.node(node_id).is_some() {
                    return Err(bad("a node id is used twice"));
                }
                match role {
                    Role::Master if self.nodes.iter().any(|n| n.role == Role::Master) => {
                        return Err(bad("a family has one master"));
                    }
                    Role::Master => {}
                    _ if self.nodes.is_empty() => {
                        return Err(bad("the first node must be the master"));
                    }
                    _ => {}
                }
                if let Some(from) = made_from_checkpoint_id
                    && self.checkpoint(from).is_none()
                {
                    return Err(bad("a node is made from an unknown checkpoint"));
                }
                self.nodes.push(Node {
                    id: node_id.clone(),
                    role: *role,
                    display_name: display_name.clone(),
                    storage_ref: storage_ref.clone(),
                    made_from: made_from_checkpoint_id.clone(),
                    follows_master: *follows_master,
                    checkpoints: Vec::new(),
                    archived: false,
                });
            }
            Event::CheckpointSaved {
                checkpoint_id,
                node_id,
                parent_ids,
                snapshot,
                fingerprint,
                note,
                author,
                changes,
                engine_version: _,
                macros,
            } => {
                if self.checkpoint(checkpoint_id).is_some() {
                    return Err(bad("a checkpoint id is used twice"));
                }
                if parent_ids.iter().any(|p| self.checkpoint(p).is_none()) {
                    return Err(bad("a checkpoint has an unknown parent"));
                }
                let node = self
                    .nodes
                    .iter_mut()
                    .find(|n| &n.id == node_id)
                    .ok_or_else(|| bad("a checkpoint of an unknown node"))?;
                node.checkpoints.push(checkpoint_id.clone());
                let number = node.checkpoints.len();
                self.checkpoints.push(Checkpoint {
                    id: checkpoint_id.clone(),
                    node: node_id.clone(),
                    parents: parent_ids.clone(),
                    snapshot: snapshot.clone(),
                    fingerprint: fingerprint.clone(),
                    note: note.clone(),
                    author: author.clone(),
                    at,
                    changes: changes.clone(),
                    number,
                    milestone: None,
                    macros: *macros,
                });
            }
            Event::UpdateOffered {
                offer_id,
                from_checkpoint_id,
                to_node_id,
                change_ids,
            } => {
                let from = self
                    .checkpoint(from_checkpoint_id)
                    .ok_or_else(|| bad("an offer from an unknown checkpoint"))?;
                if self.node(&from.node).map(|n| n.role) != Some(Role::Master) {
                    return Err(bad("offers come from master checkpoints"));
                }
                if self.node(to_node_id).map(|n| n.role) != Some(Role::Variant) {
                    return Err(bad("offers go to tailored versions"));
                }
                self.offers.push(Offer {
                    id: offer_id.clone(),
                    from: from_checkpoint_id.clone(),
                    to: to_node_id.clone(),
                    change_ids: change_ids.clone(),
                    at,
                });
            }
            Event::ChangeStateRecorded {
                node_id,
                change_id,
                from_checkpoint_id,
                state,
                how,
            } => {
                if self.node(node_id).is_none() || self.checkpoint(from_checkpoint_id).is_none() {
                    return Err(bad("a change state for an unknown node or checkpoint"));
                }
                self.change_states.insert(
                    (
                        node_id.clone(),
                        from_checkpoint_id.clone(),
                        change_id.clone(),
                    ),
                    ChangeRecord {
                        state: *state,
                        how: *how,
                        at,
                    },
                );
            }
            Event::CheckpointLinked {
                checkpoint_id,
                master_checkpoint_id,
                how: _,
            } => {
                if self.checkpoint(checkpoint_id).is_none()
                    || self.checkpoint(master_checkpoint_id).is_none()
                {
                    return Err(bad("a link between unknown checkpoints"));
                }
                self.links
                    .push((checkpoint_id.clone(), master_checkpoint_id.clone()));
            }
            Event::UpdateApplied { node_id, .. } => {
                if self.node(node_id).is_none() {
                    return Err(bad("an update for an unknown node"));
                }
            }
            Event::CopySent {
                sent_copy_id,
                from_checkpoint_id,
                recipient,
                purpose,
                format,
                file,
                id_embedded,
                cleaned,
            } => {
                if self.checkpoint(from_checkpoint_id).is_none() {
                    return Err(bad("a copy sent from an unknown checkpoint"));
                }
                if self.sent.iter().any(|s| &s.id == sent_copy_id) {
                    return Err(bad("a sent copy id is used twice"));
                }
                self.sent.push(SentCopy {
                    id: sent_copy_id.clone(),
                    from: from_checkpoint_id.clone(),
                    recipient: recipient.clone(),
                    purpose: purpose.clone(),
                    format: format.clone(),
                    file: file.clone(),
                    id_embedded: *id_embedded,
                    at,
                    cleaned: cleaned.clone(),
                });
            }
            Event::FileReturned {
                returned_id,
                sent_copy_id,
                from_checkpoint_id,
                display_name,
                file,
                matched_by,
                confidence,
                change_count,
                formatting_count,
                new_macro_code,
            } => {
                if self.checkpoint(from_checkpoint_id).is_none() {
                    return Err(bad("a returned file from an unknown checkpoint"));
                }
                if let Some(sent) = sent_copy_id
                    && !self
                        .sent
                        .iter()
                        .any(|s| &s.id == sent && &s.from == from_checkpoint_id)
                {
                    return Err(bad(
                        "a returned file for an unknown sent copy, or not the copy's checkpoint",
                    ));
                }
                if self.returned.iter().any(|r| &r.id == returned_id) {
                    return Err(bad("a returned file id is used twice"));
                }
                if *confidence > 100 {
                    return Err(bad("a match above 100%"));
                }
                self.returned.push(Returned {
                    id: returned_id.clone(),
                    sent: sent_copy_id.clone(),
                    from: from_checkpoint_id.clone(),
                    display_name: display_name.clone(),
                    file: file.clone(),
                    matched_by: *matched_by,
                    confidence: *confidence,
                    change_count: *change_count,
                    formatting_count: *formatting_count,
                    new_macro_code: *new_macro_code,
                    at,
                    reviews: Vec::new(),
                });
            }
            Event::ReturnReviewed {
                returned_id,
                into_node_id,
                result_checkpoint_id,
                kept,
                skipped,
                finished,
            } => {
                if self.node(into_node_id).is_none() {
                    return Err(bad("a review into an unknown node"));
                }
                if let Some(result) = result_checkpoint_id
                    && self
                        .checkpoint(result)
                        .is_none_or(|c| &c.node != into_node_id)
                {
                    return Err(bad("a review's checkpoint is not of the file it went into"));
                }
                let returned = self
                    .returned
                    .iter_mut()
                    .find(|r| &r.id == returned_id)
                    .ok_or_else(|| bad("a review of an unknown returned file"))?;
                returned.reviews.push(Reviewed {
                    into: into_node_id.clone(),
                    result_checkpoint: result_checkpoint_id.clone(),
                    kept: kept.clone(),
                    skipped: skipped.clone(),
                    finished: *finished,
                    at,
                });
            }
            Event::NodeArchived { node_id } => {
                let node = self
                    .nodes
                    .iter_mut()
                    .find(|n| &n.id == node_id)
                    .ok_or_else(|| bad("archiving an unknown node"))?;
                node.archived = true;
            }
            Event::MilestoneMarked {
                checkpoint_id,
                label,
            } => {
                if let Some(label) = label
                    && (label.trim().is_empty()
                        || label.trim() != label
                        || label.chars().count() > MILESTONE_MAX)
                {
                    return Err(bad("a milestone label that is empty, padded or too long"));
                }
                let checkpoint = self
                    .checkpoints
                    .iter_mut()
                    .find(|c| &c.id == checkpoint_id)
                    .ok_or_else(|| bad("a milestone on an unknown checkpoint"))?;
                checkpoint.milestone.clone_from(label);
            }
            Event::NodeRelinked {
                node_id,
                storage_ref,
            } => {
                if storage_ref.trim().is_empty() {
                    return Err(bad("a file relinked to nowhere"));
                }
                let node = self
                    .nodes
                    .iter_mut()
                    .find(|n| &n.id == node_id)
                    .ok_or_else(|| bad("relinking an unknown node"))?;
                node.storage_ref.clone_from(storage_ref);
            }
        }
        self.last_seq = record.seq;
        self.last_at = at;
        Ok(())
    }

    #[must_use]
    pub fn node(&self, id: &Id) -> Option<&Node> {
        self.nodes.iter().find(|n| &n.id == id)
    }

    #[must_use]
    pub fn checkpoint(&self, id: &Id) -> Option<&Checkpoint> {
        self.checkpoints.iter().find(|c| &c.id == id)
    }

    /// The master.
    ///
    /// # Errors
    /// [`Error::Ledger`] when the family has no master yet.
    pub fn master(&self) -> Result<&Node> {
        self.nodes
            .iter()
            .find(|n| n.role == Role::Master)
            .ok_or_else(|| bad("the family has no master"))
    }

    /// Tailored versions, not archived, in the order they were made.
    pub fn variants(&self) -> impl Iterator<Item = &Node> {
        self.nodes
            .iter()
            .filter(|n| n.role == Role::Variant && !n.archived)
    }

    /// A node's newest checkpoint.
    #[must_use]
    pub fn latest(&self, node: &Id) -> Option<&Checkpoint> {
        self.node(node)?
            .checkpoints
            .last()
            .and_then(|c| self.checkpoint(c))
    }

    /// The master's checkpoints, oldest first.
    #[must_use]
    pub fn master_checkpoints(&self) -> Vec<&Checkpoint> {
        self.master()
            .map(|m| {
                m.checkpoints
                    .iter()
                    .filter_map(|c| self.checkpoint(c))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// "v3" for a checkpoint.
    #[must_use]
    pub fn label(&self, checkpoint: &Id) -> String {
        self.checkpoint(checkpoint)
            .map_or_else(|| "?".to_owned(), |c| format!("v{}", c.number))
    }

    /// The latest recorded state of a change for a version.
    #[must_use]
    pub fn change_state(&self, node: &Id, checkpoint: &Id, change: &str) -> Option<&ChangeRecord> {
        self.change_states
            .get(&(node.clone(), checkpoint.clone(), change.to_owned()))
    }

    #[must_use]
    pub fn sent_copy(&self, id: &Id) -> Option<&SentCopy> {
        self.sent.iter().find(|s| &s.id == id)
    }

    #[must_use]
    pub fn returned_file(&self, id: &Id) -> Option<&Returned> {
        self.returned.iter().find(|r| &r.id == id)
    }

    /// Sent copies made from any checkpoint of this node.
    pub fn sent_from<'a>(&'a self, node: &'a Id) -> impl Iterator<Item = &'a SentCopy> + 'a {
        self.sent
            .iter()
            .filter(move |s| self.checkpoint(&s.from).is_some_and(|c| &c.node == node))
    }
}
