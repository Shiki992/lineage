// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What crosses the boundary, as JSON. Outputs are camelCase and mirror the
//! panel view models in `lineage` and `lineage-core`, which stay free of serde.
//! Inputs refuse unknown fields and check every id, so a malformed request from
//! JavaScript is refused here rather than half-understood. The TypeScript side
//! validates outputs against the same shapes (`packages/shell/src/schemas.ts`),
//! and `tests/golden.rs` keeps the two in step (decision 0010).

use lineage::Id;
use lineage::commands::{Outcome, UpdateGroup, Updates};
use lineage::event::{self, Role};
use lineage::state::FamilyState;
use lineage::views::{CheckpointCard, MasterPanel, SentList, VersionPanel};
use lineage_core::check::Written;
use lineage_core::diff::{Segment, SegmentKind};
use lineage_core::docx::props::LineageIds;
use lineage_core::merge::Decision;
use lineage_core::view::{ChangeCard, Choice, OverlapCard, WhatChanged};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------- inputs

/// A decision, as the panel sends it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DecisionDto {
    BringIn,
    Decline,
    KeepMine,
    UseMasters,
    KeepBothMineFirst,
    KeepBothMastersFirst,
}

impl From<DecisionDto> for Decision {
    fn from(d: DecisionDto) -> Self {
        match d {
            DecisionDto::BringIn => Self::BringIn,
            DecisionDto::Decline => Self::Decline,
            DecisionDto::KeepMine => Self::KeepMine,
            DecisionDto::UseMasters => Self::UseMasters,
            DecisionDto::KeepBothMineFirst => Self::KeepBoth {
                masters_first: false,
            },
            DecisionDto::KeepBothMastersFirst => Self::KeepBoth {
                masters_first: true,
            },
        }
    }
}

impl From<Decision> for DecisionDto {
    fn from(d: Decision) -> Self {
        match d {
            Decision::BringIn => Self::BringIn,
            Decision::Decline => Self::Decline,
            Decision::KeepMine => Self::KeepMine,
            Decision::UseMasters => Self::UseMasters,
            Decision::KeepBoth {
                masters_first: false,
            } => Self::KeepBothMineFirst,
            Decision::KeepBoth {
                masters_first: true,
            } => Self::KeepBothMastersFirst,
        }
    }
}

/// One update card's decision.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CardDecision {
    pub card: String,
    pub decision: DecisionDto,
}

/// "Send a copy".
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SendArgs {
    pub recipient: String,
    pub purpose: Option<String>,
    pub embed_id: bool,
}

/// The two Lineage ids, in and out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct IdsDto {
    pub family: Id,
    pub checkpoint: Id,
}

impl IdsDto {
    /// Ids read from a file. Ids that aren't Lineage's shape mean "not tracked".
    pub fn from_file(ids: &LineageIds) -> Option<Self> {
        Some(Self {
            family: Id::parse(&ids.family).ok()?,
            checkpoint: Id::parse(&ids.checkpoint).ok()?,
        })
    }

    pub fn to_file(&self) -> LineageIds {
        LineageIds {
            family: self.family.to_string(),
            checkpoint: self.checkpoint.to_string(),
        }
    }
}

// ---------------------------------------------------------------- outputs

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RoleDto {
    Master,
    Variant,
    WorkingCopy,
}

impl From<Role> for RoleDto {
    fn from(role: Role) -> Self {
        match role {
            Role::Master => Self::Master,
            Role::Variant => Self::Variant,
            Role::WorkingCopy => Self::WorkingCopy,
        }
    }
}

/// What a command produced. The ledger lines are appended as they are; the
/// blobs' bytes and any file bytes travel beside the JSON.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomeDto {
    /// JSON Lines to append, already numbered after the ledger's last record.
    pub ledger_lines: String,
    pub records: usize,
    /// Hashes of the blobs to store before appending.
    pub blobs: Vec<String>,
    /// The ids the open file should now carry.
    pub ids: Option<IdsDto>,
    /// True when the command produced new bytes for the file it acted on.
    pub has_file: bool,
    /// The suggested name of a new file (a tailored version or a sent copy).
    pub new_file_name: Option<String>,
    pub created: Option<Id>,
    pub nothing_changed: bool,
    /// The family, when the command started one.
    pub family_id: Option<Id>,
}

impl OutcomeDto {
    pub fn new(outcome: &Outcome, ledger_lines: String, family_id: Option<Id>) -> Self {
        Self {
            ledger_lines,
            records: outcome.records.len(),
            blobs: outcome
                .blobs
                .iter()
                .map(|b| b.hash.as_str().to_owned())
                .collect(),
            ids: outcome.ids.as_ref().and_then(IdsDto::from_file),
            has_file: outcome.file.is_some(),
            new_file_name: outcome.new_file.as_ref().map(|f| f.name.clone()),
            created: outcome.created.clone(),
            nothing_changed: outcome.nothing_changed,
            family_id,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeDto {
    pub id: Id,
    pub role: RoleDto,
    pub display_name: String,
    pub storage_ref: String,
    pub follows_master: bool,
    pub archived: bool,
    pub latest: Option<CheckpointCardDto>,
}

/// The family: who is in it, for "Send a copy" and for opening files.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyDto {
    pub family_id: Id,
    pub name: String,
    pub format: String,
    pub nodes: Vec<NodeDto>,
}

impl FamilyDto {
    pub fn new(state: &FamilyState) -> Self {
        Self {
            family_id: state.family_id.clone(),
            name: state.name.clone(),
            format: state.format.clone(),
            nodes: state
                .nodes
                .iter()
                .map(|n| NodeDto {
                    id: n.id.clone(),
                    role: n.role.into(),
                    display_name: n.display_name.clone(),
                    storage_ref: n.storage_ref.clone(),
                    follows_master: n.follows_master,
                    archived: n.archived,
                    latest: state.latest(&n.id).map(|c| CheckpointCardDto {
                        id: c.id.clone(),
                        label: state.label(&c.id),
                        note: c.note.clone(),
                        when: c.at.day_label(),
                    }),
                })
                .collect(),
        }
    }
}

/// Which file of the family is open.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenFileDto {
    pub family_id: Id,
    pub node_id: Id,
    pub role: RoleDto,
    pub display_name: String,
    pub storage_ref: String,
    /// "v3".
    pub checkpoint_label: String,
    /// False when the file is an older checkpoint than the file's latest, such
    /// as a copy kept from before.
    pub is_latest: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckpointCardDto {
    pub id: Id,
    pub label: String,
    pub note: String,
    pub when: String,
}

impl From<&CheckpointCard> for CheckpointCardDto {
    fn from(c: &CheckpointCard) -> Self {
        Self {
            id: c.id.clone(),
            label: c.label.clone(),
            note: c.note.clone(),
            when: c.when.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalloutDto {
    pub text: String,
    pub button: String,
    pub versions: Vec<Id>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRowDto {
    pub id: Id,
    pub name: String,
    pub status: String,
    pub button: String,
    /// Where the host keeps the file, for "Open".
    pub storage_ref: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterPanelDto {
    pub node_id: Id,
    pub file_name: String,
    pub chip: String,
    pub follows: String,
    pub latest: CheckpointCardDto,
    pub callout: Option<CalloutDto>,
    pub versions: Vec<VersionRowDto>,
    pub actions: Vec<String>,
    pub footer: String,
}

impl MasterPanelDto {
    pub fn new(panel: &MasterPanel, state: &FamilyState, node_id: Id) -> Self {
        Self {
            node_id,
            file_name: panel.file_name.clone(),
            chip: panel.chip.clone(),
            follows: panel.follows.clone(),
            latest: (&panel.latest).into(),
            callout: panel.callout.as_ref().map(|c| CalloutDto {
                text: c.text.clone(),
                button: c.button.clone(),
                versions: c.versions.clone(),
            }),
            versions: panel
                .versions
                .iter()
                .map(|v| VersionRowDto {
                    id: v.id.clone(),
                    name: v.name.clone(),
                    status: v.status.clone(),
                    button: v.button.clone(),
                    storage_ref: state
                        .node(&v.id)
                        .map(|n| n.storage_ref.clone())
                        .unwrap_or_default(),
                })
                .collect(),
            actions: panel.actions.clone(),
            footer: panel.footer.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionPanelDto {
    pub node_id: Id,
    pub file_name: String,
    pub chip: String,
    pub status: String,
    pub latest: CheckpointCardDto,
    pub updates: Option<String>,
    pub actions: Vec<String>,
    pub footer: String,
}

impl VersionPanelDto {
    pub fn new(panel: &VersionPanel, node_id: Id) -> Self {
        Self {
            node_id,
            file_name: panel.file_name.clone(),
            chip: panel.chip.clone(),
            status: panel.status.clone(),
            latest: (&panel.latest).into(),
            updates: panel.updates.clone(),
            actions: panel.actions.clone(),
            footer: panel.footer.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SegmentKindDto {
    Same,
    Removed,
    Added,
    Reformatted,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentDto {
    pub kind: SegmentKindDto,
    pub text: String,
}

fn segments(segments: &[Segment]) -> Vec<SegmentDto> {
    segments
        .iter()
        .map(|s| SegmentDto {
            kind: match s.kind {
                SegmentKind::Same => SegmentKindDto::Same,
                SegmentKind::Removed => SegmentKindDto::Removed,
                SegmentKind::Added => SegmentKindDto::Added,
                SegmentKind::Reformatted => SegmentKindDto::Reformatted,
            },
            text: s.text.clone(),
        })
        .collect()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChoiceDto {
    pub decision: DecisionDto,
    pub label: String,
}

fn choices(choices: &[Choice]) -> Vec<ChoiceDto> {
    choices
        .iter()
        .map(|c| ChoiceDto {
            decision: c.decision.into(),
            label: c.label.clone(),
        })
        .collect()
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeCardDto {
    pub id: String,
    pub summary: String,
    pub place: Option<String>,
    pub text: Vec<SegmentDto>,
    pub choices: Vec<ChoiceDto>,
    pub by_hand: Option<String>,
}

impl From<&ChangeCard> for ChangeCardDto {
    fn from(c: &ChangeCard) -> Self {
        Self {
            id: c.id.clone(),
            summary: c.summary.clone(),
            place: c.place.clone(),
            text: segments(&c.text),
            choices: choices(&c.choices),
            by_hand: c.by_hand.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlapCardDto {
    pub id: String,
    pub summary: String,
    pub place: Option<String>,
    pub before: Option<String>,
    pub mine: Vec<SegmentDto>,
    pub masters: Vec<SegmentDto>,
    pub choices: Vec<ChoiceDto>,
}

impl From<&OverlapCard> for OverlapCardDto {
    fn from(o: &OverlapCard) -> Self {
        Self {
            id: o.id.clone(),
            summary: o.summary.clone(),
            place: o.place.clone(),
            before: o.before.clone(),
            mine: segments(&o.mine),
            masters: segments(&o.masters),
            choices: choices(&o.choices),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WhatChangedDto {
    pub headline: String,
    pub changes: Vec<ChangeCardDto>,
    pub formatting: Vec<ChangeCardDto>,
    pub overlaps: Vec<OverlapCardDto>,
    pub by_hand: Vec<ChangeCardDto>,
    pub already_there: Vec<ChangeCardDto>,
}

impl From<&WhatChanged> for WhatChangedDto {
    fn from(w: &WhatChanged) -> Self {
        let cards = |list: &[ChangeCard]| list.iter().map(ChangeCardDto::from).collect();
        Self {
            headline: w.headline.clone(),
            changes: cards(&w.changes),
            formatting: cards(&w.formatting),
            overlaps: w.overlaps.iter().map(OverlapCardDto::from).collect(),
            by_hand: cards(&w.by_hand),
            already_there: cards(&w.already_there),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateGroupDto {
    pub checkpoint: Id,
    pub label: String,
    pub view: WhatChangedDto,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdatesDto {
    pub headline: String,
    pub groups: Vec<UpdateGroupDto>,
}

impl From<&Updates> for UpdatesDto {
    fn from(u: &Updates) -> Self {
        Self {
            headline: u.headline.clone(),
            groups: u
                .groups
                .iter()
                .map(|g: &UpdateGroup| UpdateGroupDto {
                    checkpoint: g.checkpoint.clone(),
                    label: g.label.clone(),
                    view: (&g.view).into(),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentRowDto {
    pub id: Id,
    pub recipient: String,
    pub version: String,
    pub purpose: Option<String>,
    pub format: String,
    pub date: String,
    pub chip: Option<String>,
    pub button: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentGroupDto {
    pub month: String,
    pub rows: Vec<SentRowDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentListDto {
    pub title: String,
    pub groups: Vec<SentGroupDto>,
    pub note: String,
}

impl From<&SentList> for SentListDto {
    fn from(s: &SentList) -> Self {
        Self {
            title: s.title.clone(),
            groups: s
                .groups
                .iter()
                .map(|g| SentGroupDto {
                    month: g.month.clone(),
                    rows: g
                        .rows
                        .iter()
                        .map(|r| SentRowDto {
                            id: r.id.clone(),
                            recipient: r.recipient.clone(),
                            version: r.version.clone(),
                            purpose: r.purpose.clone(),
                            format: r.format.clone(),
                            date: r.date.clone(),
                            chip: r.chip.clone(),
                            button: r.button.clone(),
                        })
                        .collect(),
                })
                .collect(),
            note: s.note.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WrittenDto {
    pub ok: bool,
    pub same_accepted: bool,
    pub same_rejected: bool,
    pub lineage_expected: usize,
    pub lineage_found: usize,
    pub other_added: usize,
}

impl From<&Written> for WrittenDto {
    fn from(w: &Written) -> Self {
        Self {
            ok: w.ok(),
            same_accepted: w.same_accepted,
            same_rejected: w.same_rejected,
            lineage_expected: w.lineage_expected,
            lineage_found: w.lineage_found,
            other_added: w.other_added,
        }
    }
}

/// The ledger lines for an outcome's records.
///
/// # Errors
/// When a record cannot be encoded (never for records the commands made).
pub fn ledger_lines(outcome: &Outcome) -> lineage::Result<String> {
    let bytes = event::encode(&outcome.records)?;
    String::from_utf8(bytes).map_err(|e| lineage::Error::Ledger(e.to_string()))
}
