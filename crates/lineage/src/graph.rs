// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The family tree: lanes, nodes and edges for the master, each tailored
//! version, the copies sent and the files that came back. Layout stays in the
//! screen; this says only what is there and how it connects.

use std::collections::BTreeMap;

use crate::event::{ChangeState, Role};
use crate::ids::{Id, Timestamp};
use crate::state::FamilyState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneKind {
    Master,
    Version,
    Sent,
    Returned,
}

/// One row of the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lane {
    /// The file's node id, or "sent" and "returned".
    pub id: String,
    pub kind: LaneKind,
    /// "Resume.docx", "Copies sent", "Files that came back".
    pub title: String,
    /// "Master", "Tailored version".
    pub chip: Option<String>,
    pub archived: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Checkpoint,
    Sent,
    Returned,
}

/// One point on the tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphNode {
    pub id: Id,
    pub lane: String,
    pub kind: NodeKind,
    /// "v3", "Northgate Analytics", "From Mentor".
    pub label: String,
    /// The checkpoint's note, "Word · id inside", "3 changes · reviewed".
    pub detail: String,
    pub at: Timestamp,
    /// "15 Oct".
    pub when: String,
    pub milestone: Option<String>,
    /// On an archived file (hidden unless asked for).
    pub archived: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EdgeKind {
    /// The next checkpoint of the same file.
    Next,
    /// A tailored version made from a master checkpoint.
    MadeFrom,
    /// Master changes brought into a version.
    BroughtIn,
    /// A version found to have a master checkpoint's changes already.
    Detected,
    /// A new version restored from an older checkpoint.
    Restored,
    /// A copy sent from a checkpoint.
    Sent,
    /// A file that came back from a copy (or a checkpoint).
    CameBack,
    /// Changes kept from a returned file, into a checkpoint.
    KeptInto,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphEdge {
    pub from: Id,
    pub to: Id,
    pub kind: EdgeKind,
    /// "Brought in 2 changes", "Has v3's changes".
    pub label: Option<String>,
}

/// The whole tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FamilyGraph {
    pub lanes: Vec<Lane>,
    /// Oldest first.
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// The family tree.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn family_graph(state: &FamilyState) -> FamilyGraph {
    let mut graph = FamilyGraph::default();
    let mut lane_of: BTreeMap<&Id, String> = BTreeMap::new();
    let mut archived_node: BTreeMap<&Id, bool> = BTreeMap::new();
    let mut files: Vec<_> = state.nodes.iter().collect();
    files.sort_by_key(|n| n.role != Role::Master);
    for node in files {
        let (kind, chip) = match node.role {
            Role::Master => (LaneKind::Master, "Master"),
            Role::Variant => (LaneKind::Version, "Tailored version"),
            Role::WorkingCopy => (LaneKind::Version, "Working copy"),
        };
        graph.lanes.push(Lane {
            id: node.id.to_string(),
            kind,
            title: node.display_name.clone(),
            chip: Some(chip.to_owned()),
            archived: node.archived,
        });
        lane_of.insert(&node.id, node.id.to_string());
        archived_node.insert(&node.id, node.archived);
    }
    let checkpoint_node: BTreeMap<&Id, &Id> =
        state.checkpoints.iter().map(|c| (&c.id, &c.node)).collect();
    let is_master = |checkpoint: &Id| {
        checkpoint_node
            .get(checkpoint)
            .and_then(|n| state.node(n))
            .is_some_and(|n| n.role == Role::Master)
    };
    let mut events: Vec<(Timestamp, usize, GraphNode)> = Vec::new();

    for c in &state.checkpoints {
        let archived = archived_node.get(&c.node).copied().unwrap_or(false);
        events.push((
            c.at,
            events.len(),
            GraphNode {
                id: c.id.clone(),
                lane: lane_of.get(&c.node).cloned().unwrap_or_default(),
                kind: NodeKind::Checkpoint,
                label: format!("v{}", c.number),
                detail: c.note.clone(),
                at: c.at,
                when: c.at.day_label(),
                milestone: c.milestone.clone(),
                archived,
            },
        ));
        let node = state.node(&c.node);
        for parent in &c.parents {
            let parent_node = checkpoint_node.get(parent).copied();
            let (kind, label) = if parent_node == Some(&c.node) {
                (EdgeKind::Next, None)
            } else if is_master(parent)
                && node.and_then(|n| n.made_from.as_ref()) == Some(parent)
                && node.and_then(|n| n.checkpoints.first()) == Some(&c.id)
            {
                (
                    EdgeKind::MadeFrom,
                    Some(format!("Made from master {}", state.label(parent))),
                )
            } else if is_master(parent) {
                let merged = state
                    .change_states
                    .iter()
                    .filter(|((n, from, _), r)| {
                        n == &c.node && from == parent && r.state == ChangeState::Merged
                    })
                    .count();
                let label = if merged == 0 {
                    format!("Brought in from master {}", state.label(parent))
                } else {
                    format!(
                        "Brought in {} from master {}",
                        plural(merged, "change", "changes"),
                        state.label(parent)
                    )
                };
                (EdgeKind::BroughtIn, Some(label))
            } else {
                let from = parent_node
                    .and_then(|n| state.node(n))
                    .map_or_else(String::new, |n| {
                        format!("{} ", n.display_name.trim_end_matches(".docx"))
                    });
                (
                    EdgeKind::Restored,
                    Some(format!("Restored from {from}{}", state.label(parent))),
                )
            };
            graph.edges.push(GraphEdge {
                from: parent.clone(),
                to: c.id.clone(),
                kind,
                label,
            });
        }
    }
    for (checkpoint, master) in &state.links {
        graph.edges.push(GraphEdge {
            from: master.clone(),
            to: checkpoint.clone(),
            kind: EdgeKind::Detected,
            label: Some(format!("Has master {}'s changes", state.label(master))),
        });
    }

    if !state.sent.is_empty() {
        graph.lanes.push(Lane {
            id: "sent".to_owned(),
            kind: LaneKind::Sent,
            title: "Copies sent".to_owned(),
            chip: None,
            archived: false,
        });
    }
    for s in &state.sent {
        let archived = checkpoint_node
            .get(&s.from)
            .and_then(|n| archived_node.get(n))
            .copied()
            .unwrap_or(false);
        let format = if s.format == "pdf" { "PDF" } else { "Word" };
        let id = if s.id_embedded {
            "Lineage ID inside"
        } else {
            "no Lineage ID"
        };
        events.push((
            s.at,
            events.len(),
            GraphNode {
                id: s.id.clone(),
                lane: "sent".to_owned(),
                kind: NodeKind::Sent,
                label: s.recipient.clone(),
                detail: format!("{format} · {} · {id}", state.label(&s.from)),
                at: s.at,
                when: s.at.day_label(),
                milestone: None,
                archived,
            },
        ));
        graph.edges.push(GraphEdge {
            from: s.from.clone(),
            to: s.id.clone(),
            kind: EdgeKind::Sent,
            label: None,
        });
    }

    if !state.returned.is_empty() {
        graph.lanes.push(Lane {
            id: "returned".to_owned(),
            kind: LaneKind::Returned,
            title: "Files that came back".to_owned(),
            chip: None,
            archived: false,
        });
    }
    for r in &state.returned {
        let sent = r.sent.as_ref().and_then(|s| state.sent_copy(s));
        let label = sent.map_or_else(
            || format!("A copy of {}", state.label(&r.from)),
            |s| format!("From {}", s.recipient),
        );
        let changes = match (r.change_count, r.formatting_count) {
            (0, 0) => "Unchanged".to_owned(),
            (0, _) => "Formatting changes only".to_owned(),
            (n, _) => plural(n, "change", "changes"),
        };
        let review = if r.finished() {
            " · reviewed"
        } else if !r.reviews.is_empty() {
            " · partly reviewed"
        } else {
            ""
        };
        events.push((
            r.at,
            events.len(),
            GraphNode {
                id: r.id.clone(),
                lane: "returned".to_owned(),
                kind: NodeKind::Returned,
                label,
                detail: format!("{changes}{review}"),
                at: r.at,
                when: r.at.day_label(),
                milestone: None,
                archived: false,
            },
        ));
        graph.edges.push(GraphEdge {
            from: sent.map_or_else(|| r.from.clone(), |s| s.id.clone()),
            to: r.id.clone(),
            kind: EdgeKind::CameBack,
            label: None,
        });
        for review in &r.reviews {
            if let Some(result) = &review.result_checkpoint {
                graph.edges.push(GraphEdge {
                    from: r.id.clone(),
                    to: result.clone(),
                    kind: EdgeKind::KeptInto,
                    label: Some(format!(
                        "Kept {}",
                        plural(review.kept.len(), "change", "changes")
                    )),
                });
            }
        }
    }

    events.sort_by_key(|(at, order, _)| (*at, *order));
    graph.nodes = events.into_iter().map(|(_, _, node)| node).collect();
    graph
}

#[cfg(test)]
mod tests;
