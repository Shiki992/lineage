// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeSet;

use super::*;
use crate::testfamily::everything;

fn edges_of(graph: &FamilyGraph, kind: EdgeKind) -> Vec<&GraphEdge> {
    graph.edges.iter().filter(|e| e.kind == kind).collect()
}

#[test]
fn the_tree_has_a_lane_per_file_then_sent_and_returned() {
    let family = everything();
    let graph = family_graph(family.host.state());
    let lanes: Vec<(&str, LaneKind, bool)> = graph
        .lanes
        .iter()
        .map(|l| (l.title.as_str(), l.kind, l.archived))
        .collect();
    assert_eq!(
        lanes,
        [
            ("Resume.docx", LaneKind::Master, false),
            ("Resume – data roles.docx", LaneKind::Version, false),
            ("Resume – platform roles.docx", LaneKind::Version, false),
            ("Resume – old.docx", LaneKind::Version, true),
            ("Copies sent", LaneKind::Sent, false),
            ("Files that came back", LaneKind::Returned, false),
        ]
    );
    assert_eq!(graph.lanes[0].id, family.master.to_string());
    assert!(
        graph
            .nodes
            .iter()
            .filter(|n| n.lane == family.archived.to_string())
            .all(|n| n.archived)
    );
}

#[test]
fn every_edge_joins_nodes_that_exist_and_nodes_are_in_time_order() {
    let family = everything();
    let graph = family_graph(family.host.state());
    let ids: BTreeSet<&Id> = graph.nodes.iter().map(|n| &n.id).collect();
    assert_eq!(ids.len(), graph.nodes.len(), "node ids are unique");
    for edge in &graph.edges {
        assert!(
            ids.contains(&edge.from) && ids.contains(&edge.to),
            "{edge:?}"
        );
    }
    let lanes: BTreeSet<&str> = graph.lanes.iter().map(|l| l.id.as_str()).collect();
    assert!(graph.nodes.iter().all(|n| lanes.contains(n.lane.as_str())));
    assert!(graph.nodes.windows(2).all(|w| w[0].at <= w[1].at));
}

#[test]
fn the_edges_say_where_each_version_came_from_and_what_it_took() {
    let family = everything();
    let state = family.host.state();
    let graph = family_graph(state);
    let made: Vec<_> = edges_of(&graph, EdgeKind::MadeFrom);
    assert_eq!(made.len(), 3, "three versions made from master v1");
    assert!(
        made.iter()
            .all(|e| e.label.as_deref() == Some("Made from master v1"))
    );
    let brought = edges_of(&graph, EdgeKind::BroughtIn);
    assert_eq!(brought.len(), 1, "only data roles brought changes in");
    assert_eq!(
        state.checkpoint(&brought[0].to).expect("checkpoint").node,
        family.data
    );
    assert_eq!(
        brought[0].label.as_deref(),
        Some("Brought in 2 changes from master v2")
    );
    assert!(
        brought
            .iter()
            .all(|e| state.checkpoint(&e.to).map(|c| &c.node) != Some(&family.platform)),
        "platform roles declined them"
    );
    let sent = edges_of(&graph, EdgeKind::Sent);
    assert_eq!(sent.len(), 2);
    assert!(sent.iter().any(|e| e.to == family.northgate));
    let back = edges_of(&graph, EdgeKind::CameBack);
    assert_eq!(back.len(), 1);
    assert_eq!(
        back[0].from, family.mentor,
        "from the copy, not the checkpoint"
    );
    let kept = edges_of(&graph, EdgeKind::KeptInto);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].label.as_deref(), Some("Kept 1 change"));
    let next = edges_of(&graph, EdgeKind::Next);
    assert!(next.iter().all(|e| {
        state.checkpoint(&e.from).map(|c| &c.node) == state.checkpoint(&e.to).map(|c| &c.node)
    }));
}

#[test]
fn nodes_carry_their_words() {
    let family = everything();
    let graph = family_graph(family.host.state());
    let node = |id: &Id| graph.nodes.iter().find(|n| &n.id == id).expect("node");
    let sent = node(&family.mentor);
    assert_eq!(sent.label, "Mentor");
    assert_eq!(sent.detail, "Word · v2 · Lineage ID inside");
    let back = node(&family.returned);
    assert_eq!(back.label, "From Mentor");
    assert_eq!(back.kind, NodeKind::Returned);
    assert!(back.detail.ends_with(" · partly reviewed") || back.detail.ends_with(" · reviewed"));
    let milestone: Vec<_> = graph
        .nodes
        .iter()
        .filter_map(|n| n.milestone.as_deref())
        .collect();
    assert_eq!(milestone, ["Sent to ten companies"]);
}

#[test]
fn a_family_with_one_checkpoint_is_one_node_and_no_edges() {
    let mut host = crate::tests::Host::new();
    host.start(
        &lineage_testkit::resume::package("t", &lineage_testkit::resume::master_v1_lines())
            .to_bytes(),
    );
    let graph = family_graph(host.state());
    assert_eq!(graph.lanes.len(), 1);
    assert_eq!(graph.nodes.len(), 1);
    assert!(graph.edges.is_empty());
}
