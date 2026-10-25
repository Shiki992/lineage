// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The ledger: one append-only log of events per family, as JSON Lines
//! (`Lineage/<familyId>/ledger.jsonl`). Current state is folded from the events
//! ([`crate::state::FamilyState`]). Records are versioned; a record this version
//! does not understand is an error, never silently skipped.

use serde::{Deserialize, Serialize};

use crate::blob::BlobHash;
use crate::error::{Error, Result};
use crate::identity::Fingerprint;
use crate::ids::{Id, Timestamp};

/// The record format version this crate writes and reads.
pub const RECORD_VERSION: u32 = 1;

/// One line of the ledger.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub v: u32,
    /// 1, 2, 3, … with no gaps.
    pub seq: u64,
    pub at: Timestamp,
    pub event: Event,
}

/// The role of a file in its family.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Master,
    Variant,
    /// Enterprise phases; kept in the data model now.
    WorkingCopy,
}

/// What one version did with one master change (decision 0002).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeState {
    Merged,
    Declined,
    Pending,
}

/// How a change state was decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum How {
    /// The person clicked.
    Clicked,
    /// Lineage found it in the content.
    Detected,
}

/// How a returned file was matched to what left.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchedBy {
    /// By the Lineage ids inside it.
    Ids,
    /// By its content, sure enough to need no question.
    Content,
    /// The person chose it from the copies it could be.
    Person,
}

/// What cleaning removed from a sent copy, kept with the record.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CleanSummary {
    pub revisions: usize,
    pub comments: usize,
    pub hidden_runs: usize,
    /// Hidden slides left out of a deck. Older records have none.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub hidden_slides: usize,
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero(n: &usize) -> bool {
    *n == 0
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(b: &bool) -> bool {
    !*b
}

/// What a sharing link lets its holder do (decision 0032).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkAccess {
    View,
    Edit,
}

/// Every kind of event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum Event {
    FamilyStarted {
        family_id: Id,
        name: String,
        format: String,
    },
    NodeAdded {
        node_id: Id,
        role: Role,
        display_name: String,
        /// Where the host keeps the file (a path, or a OneDrive item id).
        storage_ref: String,
        made_from_checkpoint_id: Option<Id>,
        follows_master: bool,
    },
    CheckpointSaved {
        checkpoint_id: Id,
        node_id: Id,
        parent_ids: Vec<Id>,
        /// The snapshot manifest blob.
        snapshot: BlobHash,
        fingerprint: Fingerprint,
        note: String,
        author: Option<String>,
        /// For a master checkpoint after the first: the change list (spec v0
        /// JSON) from its parent. The ids in it are what change states refer to.
        changes: Option<BlobHash>,
        engine_version: String,
        /// For a workbook: whether it holds macros (Phase 5B), for My
        /// documents' "Contains macros". Absent for other formats and in
        /// older ledgers.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        macros: Option<bool>,
    },
    UpdateOffered {
        offer_id: Id,
        from_checkpoint_id: Id,
        to_node_id: Id,
        change_ids: Vec<String>,
    },
    ChangeStateRecorded {
        node_id: Id,
        change_id: String,
        from_checkpoint_id: Id,
        state: ChangeState,
        how: How,
    },
    /// A version found to contain a master checkpoint's changes is linked as
    /// its descendant, which repairs the family tree (decision 0002).
    CheckpointLinked {
        checkpoint_id: Id,
        master_checkpoint_id: Id,
        how: How,
    },
    UpdateApplied {
        node_id: Id,
        /// The checkpoint saved with the changes brought in; none when every
        /// change was declined.
        result_checkpoint_id: Option<Id>,
        merged: Vec<String>,
        declined: Vec<String>,
    },
    CopySent {
        sent_copy_id: Id,
        from_checkpoint_id: Id,
        recipient: String,
        purpose: Option<String>,
        format: String,
        /// The exact bytes that were sent.
        file: BlobHash,
        id_embedded: bool,
        cleaned: CleanSummary,
    },
    /// A file came back: a sent copy, or a copy of a checkpoint that was not
    /// recorded as sent (decision 0012).
    FileReturned {
        returned_id: Id,
        sent_copy_id: Option<Id>,
        from_checkpoint_id: Id,
        /// The file's name when it came back.
        display_name: String,
        /// The exact bytes that came back.
        file: BlobHash,
        matched_by: MatchedBy,
        /// How much of the text matches what left, 0 to 100 (100 by ids).
        confidence: u8,
        /// Their changes to the words, and their formatting-only changes.
        change_count: usize,
        formatting_count: usize,
        /// It came back with macro code that is new or different from what
        /// left (Phase 5B, decision 0027). Absent in older ledgers.
        #[serde(default, skip_serializing_if = "is_false")]
        new_macro_code: bool,
    },
    /// The person kept or skipped changes in a returned file. A file can be
    /// reviewed in more than one sitting.
    ReturnReviewed {
        returned_id: Id,
        into_node_id: Id,
        result_checkpoint_id: Option<Id>,
        kept: Vec<String>,
        skipped: Vec<String>,
        /// True when nothing was left to decide.
        finished: bool,
    },
    NodeArchived {
        node_id: Id,
    },
    /// A checkpoint marked as a milestone, such as "Sent to ten companies", or
    /// the mark taken off (`label: None`).
    MilestoneMarked {
        checkpoint_id: Id,
        label: Option<String>,
    },
    /// The file was found somewhere else (moved or renamed): where it is now.
    NodeRelinked {
        node_id: Id,
        storage_ref: String,
    },
    /// A sent copy shared as a link instead of, or as well as, attached
    /// (decision 0032). The file at `file_ref` is the clean copy that left;
    /// with `edit` access its holders may change it, and what they change
    /// comes back as a returned file. The recorded copy never changes.
    LinkShared {
        sent_copy_id: Id,
        /// The storage's own id for the link (OneDrive: the permission's id).
        link_id: String,
        access: LinkAccess,
        url: String,
        /// Where the shared file is in Lineage's storage.
        file_ref: String,
        /// Who the storage says can open it: "anonymous", "organization" or
        /// "users", as it reported.
        scope: String,
    },
    /// The link was removed: nobody can open the shared file through it.
    LinkStopped {
        sent_copy_id: Id,
        link_id: String,
    },
}

/// The longest link or file reference a ledger keeps, in bytes.
pub const LINK_MAX: usize = 2048;

/// The longest milestone label, in characters.
pub const MILESTONE_MAX: usize = 60;

/// Encodes records as JSON Lines.
///
/// # Errors
/// Never in practice; serialisation of these types cannot fail.
pub fn encode(records: &[Record]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    for record in records {
        serde_json::to_writer(&mut out, record).map_err(|e| Error::Ledger(e.to_string()))?;
        out.push(b'\n');
    }
    Ok(out)
}

/// Decodes a JSON Lines ledger. Blank lines are allowed (a host may append a
/// trailing new line); anything else that is not a record is an error.
///
/// # Errors
/// [`Error::Ledger`] for unreadable lines or unknown record versions.
pub fn decode(bytes: &[u8]) -> Result<Vec<Record>> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| Error::Ledger("ledger is not UTF-8".to_owned()))?;
    let mut records = Vec::new();
    for (number, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let value: serde_json::Value = serde_json::from_str(line)
            .map_err(|e| Error::Ledger(format!("line {}: {e}", number + 1)))?;
        let v = value
            .get("v")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if v != u64::from(RECORD_VERSION) {
            return Err(Error::Ledger(format!(
                "line {}: record version {v} is not supported by this version of Lineage",
                number + 1
            )));
        }
        let record: Record = serde_json::from_value(value)
            .map_err(|e| Error::Ledger(format!("line {}: {e}", number + 1)))?;
        records.push(record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u8) -> Id {
        Id::parse(&format!("{n:032x}")).expect("id")
    }

    #[test]
    fn records_round_trip_as_json_lines() {
        let records = vec![
            Record {
                v: RECORD_VERSION,
                seq: 1,
                at: Timestamp(1_792_056_600_000),
                event: Event::FamilyStarted {
                    family_id: id(1),
                    name: "Resume".into(),
                    format: "docx".into(),
                },
            },
            Record {
                v: RECORD_VERSION,
                seq: 2,
                at: Timestamp(1_792_056_600_000),
                event: Event::ChangeStateRecorded {
                    node_id: id(2),
                    change_id: "c0123456789abcdef".into(),
                    from_checkpoint_id: id(3),
                    state: ChangeState::Declined,
                    how: How::Clicked,
                },
            },
        ];
        let bytes = encode(&records).expect("encodes");
        let text = String::from_utf8(bytes.clone()).expect("utf-8");
        assert_eq!(text.lines().count(), 2);
        assert!(text.contains(r#""type":"FamilyStarted""#));
        assert_eq!(decode(&bytes).expect("decodes"), records);
    }

    #[test]
    fn unknown_versions_and_fields_are_refused() {
        assert!(decode(br#"{"v":2,"seq":1,"at":0,"event":{"type":"NodeArchived","node_id":"00000000000000000000000000000001"}}"#).is_err());
        assert!(decode(br#"{"v":1,"seq":1,"at":0,"event":{"type":"Mystery"}}"#).is_err());
        assert!(decode(br#"{"v":1,"seq":1,"at":0,"extra":1,"event":{"type":"NodeArchived","node_id":"00000000000000000000000000000001"}}"#).is_err());
        assert!(decode(b"not json\n").is_err());
        assert_eq!(decode(b"\n\n").expect("blank lines"), []);
    }
}
