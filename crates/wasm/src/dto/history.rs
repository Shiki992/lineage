// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What crosses the boundary for "My documents": the library, a document's
//! page and its family tree (decision 0015).

use lineage::Id;
use lineage::graph::{EdgeKind, FamilyGraph, LaneKind, NodeKind};
use lineage::library::Library;
use lineage::state::FamilyState;
use lineage::views::DocumentPage;
use serde::{Deserialize, Serialize};

use super::{RoleDto, SentListDto};

/// One family folder and its ledger's version now, for checking the index.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ObservedDto {
    pub family: Id,
    pub version: String,
}

/// One row of "My documents".
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryEntryDto {
    pub family_id: Id,
    pub name: String,
    pub format: String,
    /// "Word document".
    pub kind: String,
    pub master_file: Option<String>,
    pub tailored: usize,
    pub checkpoints: usize,
    pub sent: usize,
    pub returned: usize,
    pub to_review: usize,
    /// Milliseconds since 1970, for sorting.
    pub last_at: i64,
    pub summary: String,
    pub last_activity: String,
    pub attention: Option<String>,
    pub can_open: bool,
    pub damage: Option<String>,
    /// "Contains macros".
    pub badges: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryDto {
    pub entries: Vec<LibraryEntryDto>,
    pub empty: Option<String>,
}

impl From<&Library> for LibraryDto {
    fn from(l: &Library) -> Self {
        Self {
            entries: l
                .entries
                .iter()
                .map(|e| LibraryEntryDto {
                    family_id: e.facts.family_id.clone(),
                    name: e.facts.name.clone(),
                    format: e.facts.format.clone(),
                    kind: e.kind.clone(),
                    master_file: e.facts.master_file.clone(),
                    tailored: e.facts.tailored,
                    checkpoints: e.facts.checkpoints,
                    sent: e.facts.sent,
                    returned: e.facts.returned,
                    to_review: e.facts.to_review,
                    last_at: e.facts.last_at.0,
                    summary: e.summary.clone(),
                    last_activity: e.last_activity.clone(),
                    attention: e.attention.clone(),
                    can_open: e.can_open,
                    damage: e.facts.damage.clone(),
                    badges: e.badges.clone(),
                })
                .collect(),
            empty: l.empty.clone(),
        }
    }
}

/// The library after checking the index. The index to store, when it changed,
/// travels as the reply's file.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshedDto {
    pub library: LibraryDto,
    pub index_changed: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimelineEntryDto {
    pub id: Id,
    pub label: String,
    pub note: String,
    pub date: String,
    pub milestone: Option<String>,
    pub author: Option<String>,
    pub sent: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileHistoryDto {
    pub node: Id,
    pub name: String,
    pub role: RoleDto,
    pub chip: String,
    pub status: String,
    pub archived: bool,
    pub storage_ref: String,
    pub entries: Vec<TimelineEntryDto>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPageDto {
    pub family_id: Id,
    pub name: String,
    pub format: String,
    pub kind: String,
    pub can_change: bool,
    pub note: Option<String>,
    pub files: Vec<FileHistoryDto>,
    pub sent: SentListDto,
    /// "Contains macros".
    pub badges: Vec<String>,
}

impl DocumentPageDto {
    pub fn new(page: &DocumentPage, state: &FamilyState) -> Self {
        Self {
            family_id: page.family_id.clone(),
            name: page.name.clone(),
            format: page.format.clone(),
            kind: page.kind.clone(),
            can_change: page.can_change,
            note: page.note.clone(),
            files: page
                .files
                .iter()
                .map(|f| FileHistoryDto {
                    node: f.node.clone(),
                    name: f.name.clone(),
                    role: state
                        .node(&f.node)
                        .map_or(RoleDto::Variant, |n| n.role.into()),
                    chip: f.chip.clone(),
                    status: f.status.clone(),
                    archived: f.archived,
                    storage_ref: f.storage_ref.clone(),
                    entries: f
                        .entries
                        .iter()
                        .map(|e| TimelineEntryDto {
                            id: e.id.clone(),
                            label: e.label.clone(),
                            note: e.note.clone(),
                            date: e.date.clone(),
                            milestone: e.milestone.clone(),
                            author: e.author.clone(),
                            sent: e.sent.clone(),
                        })
                        .collect(),
                })
                .collect(),
            sent: SentListDto::from(&page.sent),
            badges: page.badges.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LaneKindDto {
    Master,
    Version,
    Sent,
    Returned,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeKindDto {
    Checkpoint,
    Sent,
    Returned,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum EdgeKindDto {
    Next,
    MadeFrom,
    BroughtIn,
    Detected,
    Restored,
    Sent,
    CameBack,
    KeptInto,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneDto {
    pub id: String,
    pub kind: LaneKindDto,
    pub title: String,
    pub chip: Option<String>,
    pub archived: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNodeDto {
    pub id: Id,
    pub lane: String,
    pub kind: NodeKindDto,
    pub label: String,
    pub detail: String,
    pub at: i64,
    pub when: String,
    pub milestone: Option<String>,
    pub archived: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphEdgeDto {
    pub from: Id,
    pub to: Id,
    pub kind: EdgeKindDto,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FamilyGraphDto {
    pub lanes: Vec<LaneDto>,
    pub nodes: Vec<GraphNodeDto>,
    pub edges: Vec<GraphEdgeDto>,
}

fn lane_kind(kind: LaneKind) -> LaneKindDto {
    match kind {
        LaneKind::Master => LaneKindDto::Master,
        LaneKind::Version => LaneKindDto::Version,
        LaneKind::Sent => LaneKindDto::Sent,
        LaneKind::Returned => LaneKindDto::Returned,
    }
}

fn node_kind(kind: NodeKind) -> NodeKindDto {
    match kind {
        NodeKind::Checkpoint => NodeKindDto::Checkpoint,
        NodeKind::Sent => NodeKindDto::Sent,
        NodeKind::Returned => NodeKindDto::Returned,
    }
}

fn edge_kind(kind: EdgeKind) -> EdgeKindDto {
    match kind {
        EdgeKind::Next => EdgeKindDto::Next,
        EdgeKind::MadeFrom => EdgeKindDto::MadeFrom,
        EdgeKind::BroughtIn => EdgeKindDto::BroughtIn,
        EdgeKind::Detected => EdgeKindDto::Detected,
        EdgeKind::Restored => EdgeKindDto::Restored,
        EdgeKind::Sent => EdgeKindDto::Sent,
        EdgeKind::CameBack => EdgeKindDto::CameBack,
        EdgeKind::KeptInto => EdgeKindDto::KeptInto,
    }
}

impl From<&FamilyGraph> for FamilyGraphDto {
    fn from(g: &FamilyGraph) -> Self {
        Self {
            lanes: g
                .lanes
                .iter()
                .map(|l| LaneDto {
                    id: l.id.clone(),
                    kind: lane_kind(l.kind),
                    title: l.title.clone(),
                    chip: l.chip.clone(),
                    archived: l.archived,
                })
                .collect(),
            nodes: g
                .nodes
                .iter()
                .map(|n| GraphNodeDto {
                    id: n.id.clone(),
                    lane: n.lane.clone(),
                    kind: node_kind(n.kind),
                    label: n.label.clone(),
                    detail: n.detail.clone(),
                    at: n.at.0,
                    when: n.when.clone(),
                    milestone: n.milestone.clone(),
                    archived: n.archived,
                })
                .collect(),
            edges: g
                .edges
                .iter()
                .map(|e| GraphEdgeDto {
                    from: e.from.clone(),
                    to: e.to.clone(),
                    kind: edge_kind(e.kind),
                    label: e.label.clone(),
                })
                .collect(),
        }
    }
}

/// A document's page and its tree, from one read of its ledger.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentDto {
    pub page: DocumentPageDto,
    pub graph: FamilyGraphDto,
}
