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
use lineage::event::{self, MatchedBy, Role};
use lineage::returned::{KeepFormatting, RecognitionView, ReturnReview, Verdict};
use lineage::state::FamilyState;
use lineage::views::{CheckpointCard, MasterPanel, ReturnedRow, SentList, VersionPanel};
use lineage_core::check::Written;
use lineage_core::diff::{Segment, SegmentKind};
use lineage_core::docx::props::LineageIds;
use lineage_core::merge::Decision;
use lineage_core::view::{ChangeCard, Choice, OverlapCard, WhatChanged};
use serde::{Deserialize, Serialize};

mod history;
mod workbook;
pub use history::*;
pub use workbook::*;

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

/// "Share a link": what the storage made (decision 0032).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LinkArgs {
    pub id: String,
    pub access: LinkAccessDto,
    pub url: String,
    pub file_ref: String,
    pub scope: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkAccessDto {
    View,
    Edit,
}

impl From<LinkAccessDto> for lineage::event::LinkAccess {
    fn from(a: LinkAccessDto) -> Self {
        match a {
            LinkAccessDto::View => Self::View,
            LinkAccessDto::Edit => Self::Edit,
        }
    }
}

impl From<lineage::event::LinkAccess> for LinkAccessDto {
    fn from(a: lineage::event::LinkAccess) -> Self {
        match a {
            lineage::event::LinkAccess::View => Self::View,
            lineage::event::LinkAccess::Edit => Self::Edit,
        }
    }
}

/// One family's ledger, for recognising a file across families.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct LedgerDto {
    pub family: Id,
    /// The stored JSON Lines, as text.
    pub ledger: String,
}

/// How the person confirmed which copy a returned file is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MatchedByDto {
    Ids,
    Content,
    Person,
}

impl From<MatchedByDto> for MatchedBy {
    fn from(m: MatchedByDto) -> Self {
        match m {
            MatchedByDto::Ids => Self::Ids,
            MatchedByDto::Content => Self::Content,
            MatchedByDto::Person => Self::Person,
        }
    }
}

/// "Review the changes" on a recognised file.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RecordArgs {
    /// The file's name as it came back.
    pub name: String,
    pub checkpoint: Id,
    pub sent: Option<Id>,
    pub matched_by: MatchedByDto,
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

/// The sent copy a file opened in Word is, when its ids name one.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyOfDto {
    pub sent_copy_id: Id,
    pub recipient: String,
    /// "2 Oct".
    pub date: String,
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
    /// The family's format: "docx" or "pptx".
    pub format: String,
    /// "v3".
    pub checkpoint_label: String,
    /// False when the file is an older checkpoint than the file's latest, such
    /// as a copy kept from before.
    pub is_latest: bool,
    /// Set when the open file is a copy that was sent (it came back): the file
    /// above is the one it was made from.
    pub copy_of: Option<CopyOfDto>,
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

/// A file that came back, on a panel.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnedRowDto {
    pub id: Id,
    pub text: String,
    pub detail: String,
    pub button: String,
}

fn returned_rows(rows: &[ReturnedRow]) -> Vec<ReturnedRowDto> {
    rows.iter()
        .map(|r| ReturnedRowDto {
            id: r.id.clone(),
            text: r.text.clone(),
            detail: r.detail.clone(),
            button: r.button.clone(),
        })
        .collect()
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
    pub returned: Vec<ReturnedRowDto>,
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
            returned: returned_rows(&panel.returned),
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
    pub returned: Vec<ReturnedRowDto>,
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
            returned: returned_rows(&panel.returned),
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

pub(crate) fn choices(choices: &[Choice]) -> Vec<ChoiceDto> {
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
    /// A workbook's changes in the five categories (decision 0022).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workbook: Option<WorkbookViewDto>,
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
            workbook: w.workbook.as_deref().map(WorkbookViewDto::from),
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
    /// The latest file that came back from this copy.
    pub returned: Option<Id>,
    pub button: String,
    /// Its sharing link, if it was ever shared (decision 0032).
    pub link: Option<SentLinkDto>,
    /// While an edit link works: the shared file, and the SHA-256 of every
    /// version of it Lineage has seen.
    pub shared: Option<SharedFileDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SentLinkDto {
    pub line: String,
    pub who: String,
    pub url: String,
    pub access: LinkAccessDto,
    pub live: bool,
    pub id: String,
    pub file_ref: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedFileDto {
    pub file_ref: String,
    pub known: Vec<String>,
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
                            returned: r.returned.clone(),
                            button: r.button.clone(),
                            link: r.link.as_ref().map(|l| SentLinkDto {
                                line: l.line.clone(),
                                who: l.who.clone(),
                                url: l.url.clone(),
                                access: l.access.into(),
                                live: l.live,
                                id: l.id.clone(),
                                file_ref: l.file_ref.clone(),
                            }),
                            shared: r.shared.as_ref().map(|s| SharedFileDto {
                                file_ref: s.file_ref.clone(),
                                known: s.known.clone(),
                            }),
                        })
                        .collect(),
                })
                .collect(),
            note: s.note.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VerdictDto {
    Certain,
    Choose,
    Unknown,
}

/// One copy a returned file could be.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandidateDto {
    pub family: Id,
    pub checkpoint: Id,
    pub sent: Option<Id>,
    pub node: Id,
    pub node_name: String,
    pub title: String,
    pub detail: String,
    pub score: String,
}

/// "Which copy is this?"
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionDto {
    pub verdict: VerdictDto,
    pub by_ids: bool,
    pub headline: String,
    pub note: Option<String>,
    pub candidates: Vec<CandidateDto>,
}

impl From<&RecognitionView> for RecognitionDto {
    fn from(r: &RecognitionView) -> Self {
        Self {
            verdict: match r.verdict {
                Verdict::Certain => VerdictDto::Certain,
                Verdict::Choose => VerdictDto::Choose,
                Verdict::Unknown => VerdictDto::Unknown,
            },
            by_ids: r.by_ids,
            headline: r.headline.clone(),
            note: r.note.clone(),
            candidates: r
                .candidates
                .iter()
                .map(|c| CandidateDto {
                    family: c.family.clone(),
                    checkpoint: c.checkpoint.clone(),
                    sent: c.sent.clone(),
                    node: c.node.clone(),
                    node_name: c.node_name.clone(),
                    title: c.title.clone(),
                    detail: c.detail.clone(),
                    score: c.score.clone(),
                })
                .collect(),
        }
    }
}

/// Whether kept changes bring only their words, and why (decision 0014).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeepFormattingDto {
    pub words_only: bool,
    pub note: Option<String>,
    pub formatting_note: Option<String>,
}

impl From<&KeepFormatting> for KeepFormattingDto {
    fn from(k: &KeepFormatting) -> Self {
        Self {
            words_only: k.words_only,
            note: k.note.clone(),
            formatting_note: k.formatting_note.clone(),
        }
    }
}

/// The review of a file that came back.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReturnReviewDto {
    pub returned: Id,
    pub headline: String,
    pub from: String,
    pub notes: Vec<String>,
    pub theirs: String,
    pub from_node: Id,
    pub from_name: String,
    pub view: WhatChangedDto,
    pub formatting: KeepFormattingDto,
}

impl From<&ReturnReview> for ReturnReviewDto {
    fn from(r: &ReturnReview) -> Self {
        Self {
            returned: r.returned.clone(),
            headline: r.headline.clone(),
            from: r.from.clone(),
            notes: r.notes.clone(),
            theirs: r.theirs.clone(),
            from_node: r.from_node.clone(),
            from_name: r.from_name.clone(),
            view: (&r.view).into(),
            formatting: (&r.formatting).into(),
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

/// One version's line after "Update N versions".
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRunDto {
    pub node: Id,
    pub name: String,
    /// "updated", "upToDate", "needsChoice" or "notUpdated".
    pub result: &'static str,
    pub text: String,
    pub brought_in: usize,
    pub saved_edits: bool,
    pub left: WhatChangedDto,
    /// True when the reply has new bytes for this version's file
    /// (`takeVersionFile`).
    pub has_file: bool,
    /// Where the version's file is, for writing it back.
    pub storage_ref: String,
}

/// "Update N versions": what to append, and a line per version.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRunDto {
    pub outcome: OutcomeDto,
    pub headline: String,
    pub versions: Vec<VersionRunDto>,
}

impl VersionRunDto {
    pub fn new(v: &lineage::decks::VersionRun, state: &FamilyState) -> Self {
        use lineage::decks::RunResult;
        Self {
            node: v.node.clone(),
            name: v.name.clone(),
            result: match v.result {
                RunResult::Updated => "updated",
                RunResult::UpToDate => "upToDate",
                RunResult::NeedsChoice => "needsChoice",
                RunResult::NotUpdated => "notUpdated",
            },
            text: v.text.clone(),
            brought_in: v.brought_in,
            saved_edits: v.saved_edits,
            left: WhatChangedDto::from(&v.left),
            has_file: v.file.is_some(),
            storage_ref: state
                .node(&v.node)
                .map(|n| n.storage_ref.clone())
                .unwrap_or_default(),
        }
    }
}

impl UpdateRunDto {
    pub fn new(run: &lineage::decks::UpdateRun, lines: String, state: &FamilyState) -> Self {
        Self {
            outcome: OutcomeDto::new(&run.outcome, lines, None),
            headline: run.headline.clone(),
            versions: run
                .versions
                .iter()
                .map(|v| VersionRunDto::new(v, state))
                .collect(),
        }
    }
}

/// One step PowerPoint takes on the open deck (decision 0019). Slides are
/// named as Office.js names them, `"<sldId>#<creationId>"`.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SlideOpDto {
    Delete {
        slide: String,
    },
    /// From the source deck, after `after` (null: first).
    Insert {
        slide: String,
        after: Option<String>,
    },
    Move {
        slide: String,
        index: usize,
    },
}

/// The steps from the open deck to another, and the order they lead to.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideOpsDto {
    pub ops: Vec<SlideOpDto>,
    pub order: Vec<String>,
}

impl From<&lineage_core::pptx::slides::SlideOps> for SlideOpsDto {
    fn from(ops: &lineage_core::pptx::slides::SlideOps) -> Self {
        use lineage_core::pptx::slides::SlideOp;
        Self {
            ops: ops
                .ops
                .iter()
                .map(|op| match op {
                    SlideOp::Delete { slide } => SlideOpDto::Delete {
                        slide: slide.clone(),
                    },
                    SlideOp::Insert { slide, after } => SlideOpDto::Insert {
                        slide: slide.clone(),
                        after: after.clone(),
                    },
                    SlideOp::Move { slide, index } => SlideOpDto::Move {
                        slide: slide.clone(),
                        index: *index,
                    },
                })
                .collect(),
            order: ops.order.clone(),
        }
    }
}

/// Bring in for the open deck, first half: the version's line and the slide
/// steps. The merged deck, when there is one, is the reply's file.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenDeckPlanDto {
    pub version: VersionRunDto,
    pub steps: SlideOpsDto,
    /// True when the reply's file is the merged deck to insert slides from.
    pub has_file: bool,
    /// True when there is something to record, slides changing or not.
    pub records: bool,
}

/// A node and a card decision, for "Update N versions".
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionChoice {
    pub node: Id,
    pub card: String,
    pub decision: DecisionDto,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JustChangedDto {
    pub headline: String,
    pub cards: Vec<ChangeCardDto>,
    pub unsaved: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetDto {
    pub id: Id,
    pub name: String,
    pub checked: bool,
    pub context: String,
    pub storage_ref: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastSentDto {
    pub sent: Id,
    pub text: String,
    pub button: String,
}

/// The pitch deck panel.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckPanelDto {
    pub master: MasterPanelDto,
    pub just_changed: Option<JustChangedDto>,
    pub targets: Vec<TargetDto>,
    pub button: Option<String>,
    pub last_sent: Option<LastSentDto>,
}

impl DeckPanelDto {
    pub fn new(panel: &lineage::decks::DeckPanel, state: &FamilyState, master: Id) -> Self {
        Self {
            master: MasterPanelDto::new(&panel.master, state, master),
            just_changed: panel.just_changed.as_ref().map(|j| JustChangedDto {
                headline: j.headline.clone(),
                cards: j.cards.iter().map(ChangeCardDto::from).collect(),
                unsaved: j.unsaved,
            }),
            targets: panel
                .targets
                .iter()
                .map(|t| TargetDto {
                    id: t.id.clone(),
                    name: t.name.clone(),
                    checked: t.checked,
                    context: t.context.clone(),
                    storage_ref: state
                        .node(&t.id)
                        .map(|n| n.storage_ref.clone())
                        .unwrap_or_default(),
                })
                .collect(),
            button: panel.button.clone(),
            last_sent: panel.last_sent.as_ref().map(|l| LastSentDto {
                sent: l.sent.clone(),
                text: l.text.clone(),
                button: l.button.clone(),
            }),
        }
    }
}
