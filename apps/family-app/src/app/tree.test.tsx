// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import { render, screen, within } from "@testing-library/react";
import type { FamilyGraph } from "@lineage/shell";
import { describe, expect, it } from "vitest";
import { documentView } from "./fixtures";
import { Tree, layout, step } from "./tree";

function large(points: number): FamilyGraph {
  const lanes = ["master", "a", "b", "c", "d"].map((id, i) => ({
    id: id === "master" ? id : id.padEnd(32, "0"),
    kind: i === 0 ? ("master" as const) : ("version" as const),
    title: i === 0 ? "Resume.docx" : `Version ${id}.docx`,
    chip: null,
    archived: false,
  }));
  const nodes = Array.from({ length: points }, (_, n) => ({
    id: n.toString(16).padStart(32, "0"),
    lane: lanes[n % lanes.length]?.id ?? "master",
    kind: "checkpoint" as const,
    label: `v${String(Math.floor(n / lanes.length) + 1)}`,
    detail: `Draft ${String(n)}`,
    at: n * 1000,
    when: "15 Oct",
    milestone: null,
    archived: false,
  }));
  const edges = nodes.slice(lanes.length).map((n, i) => ({
    from: nodes[i]?.id ?? "",
    to: n.id,
    kind: "next" as const,
    label: null,
  }));
  return { lanes, nodes, edges };
}

describe("the family tree's layout", () => {
  it("puts one column per event and one row per lane", () => {
    const { graph } = documentView();
    const shape = layout(graph, false);
    expect(shape.nodes.map((n) => n.x)).toEqual([...shape.nodes.keys()].map((i) => 28 + i * 56));
    expect(new Set(shape.nodes.map((n) => n.row)).size).toBe(graph.lanes.length);
    expect(shape.edges.length).toBe(graph.edges.length);
  });

  it("hides archived lanes and their points unless asked", () => {
    const { graph } = documentView();
    const archived = {
      ...graph,
      lanes: graph.lanes.map((l, i) => (i === 2 ? { ...l, archived: true } : l)),
      nodes: graph.nodes.map((n) => (n.lane === graph.lanes[2]?.id ? { ...n, archived: true } : n)),
    };
    const hidden = layout(archived, false);
    const shown = layout(archived, true);
    expect(hidden.lanes.length).toBe(graph.lanes.length - 1);
    expect(hidden.nodes.length).toBeLessThan(shown.nodes.length);
    expect(hidden.edges.every((e) => hidden.nodes.some((n) => n.id === e.to))).toBe(true);
  });

  it("steps along a row and to the nearest point in the next row", () => {
    const shape = layout(large(20), false);
    const first = shape.nodes[0]?.id ?? "";
    expect(step(shape, first, "ArrowLeft")).toBeUndefined();
    expect(step(shape, first, "ArrowRight")).toBe(shape.nodes[5]?.id);
    expect(step(shape, first, "ArrowDown")).toBe(shape.nodes[1]?.id);
    expect(step(shape, first, "ArrowUp")).toBeUndefined();
    expect(step(shape, first, "End")).toBe(shape.nodes[15]?.id);
  });

  it("draws 350 points quickly, with one tab stop", () => {
    const started = performance.now();
    render(
      <Tree
        graph={large(350)}
        showArchived={false}
        selected={undefined}
        onSelect={() => undefined}
      />,
    );
    const elapsed = performance.now() - started;
    const buttons = within(screen.getByRole("group", { name: "Family tree" })).getAllByRole(
      "button",
    );
    expect(buttons).toHaveLength(350);
    expect(buttons.filter((b) => b.tabIndex === 0)).toHaveLength(1);
    // jsdom is far slower than a browser; this only catches something quadratic.
    expect(elapsed).toBeLessThan(3000);
  });
});
