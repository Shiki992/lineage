// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The family tree: a row (lane) for the master, each tailored version, the
// copies sent and the files that came back, like the canvas's family-tree
// artboard. The engine says what is there and how it connects (FamilyGraph);
// this only lays it out: one column per event, in time order.
//
// Every point is a real button. The tree is one tab stop; the arrow keys move
// along a row (left, right) and between rows (up, down) to the nearest point,
// Home and End go to a row's ends, and Enter or Space shows that point.

import type { FamilyGraph, GraphEdge, GraphNode, Lane } from "@lineage/shell";
import type { KeyboardEvent } from "react";
import { useMemo, useRef } from "react";

const COLUMN = 56;
const ROW = 64;
const LEFT = 28;
const TOP = 32;
const SIZE = 34;

export interface Layout {
  readonly lanes: readonly Lane[];
  readonly nodes: readonly (GraphNode & {
    readonly x: number;
    readonly y: number;
    readonly row: number;
  })[];
  readonly edges: readonly (GraphEdge & { readonly d: string })[];
  readonly width: number;
  readonly height: number;
}

/** Positions for every point and edge that is shown. */
export function layout(graph: FamilyGraph, showArchived: boolean): Layout {
  const lanes = graph.lanes.filter((l) => showArchived || !l.archived);
  const row = new Map(lanes.map((l, i) => [l.id, i]));
  const shown = graph.nodes.filter((n) => (showArchived || !n.archived) && row.has(n.lane));
  const nodes = shown.map((n, column) => {
    const r = row.get(n.lane) ?? 0;
    return { ...n, row: r, x: LEFT + column * COLUMN, y: TOP + r * ROW };
  });
  const at = new Map(nodes.map((n) => [n.id, n]));
  const edges = graph.edges.flatMap((e) => {
    const from = at.get(e.from);
    const to = at.get(e.to);
    if (!from || !to) {
      return [];
    }
    const d =
      from.y === to.y
        ? `M ${String(from.x)} ${String(from.y)} H ${String(to.x)}`
        : `M ${String(from.x)} ${String(from.y)} C ${String(from.x + COLUMN / 2)} ${String(from.y)}, ${String(to.x - COLUMN / 2)} ${String(to.y)}, ${String(to.x)} ${String(to.y)}`;
    return [{ ...e, d }];
  });
  return {
    lanes,
    nodes,
    edges,
    width: LEFT * 2 + Math.max(0, nodes.length - 1) * COLUMN,
    height: TOP * 2 + Math.max(0, lanes.length - 1) * ROW,
  };
}

/** The point the arrow key moves to, if any. */
export function step(
  shape: Layout,
  from: string,
  key: "ArrowLeft" | "ArrowRight" | "ArrowUp" | "ArrowDown" | "Home" | "End",
): string | undefined {
  const here = shape.nodes.find((n) => n.id === from);
  if (!here) {
    return shape.nodes[0]?.id;
  }
  const inRow = (r: number) => shape.nodes.filter((n) => n.row === r);
  const same = inRow(here.row);
  const i = same.findIndex((n) => n.id === from);
  switch (key) {
    case "ArrowLeft":
      return same[i - 1]?.id;
    case "ArrowRight":
      return same[i + 1]?.id;
    case "Home":
      return same[0]?.id;
    case "End":
      return same.at(-1)?.id;
    default: {
      const direction = key === "ArrowDown" ? 1 : -1;
      for (let r = here.row + direction; r >= 0 && r < shape.lanes.length; r += direction) {
        const candidates = inRow(r);
        if (candidates.length > 0) {
          return candidates.reduce((best, n) =>
            Math.abs(n.x - here.x) < Math.abs(best.x - here.x) ? n : best,
          ).id;
        }
      }
      return undefined;
    }
  }
}

const KEYS = new Set(["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"]);

function laneOf(lanes: readonly Lane[], id: string): Lane | undefined {
  return lanes.find((l) => l.id === id);
}

/** What a screen reader hears for a point. */
export function describeNode(node: GraphNode, lanes: readonly Lane[]): string {
  const lane = laneOf(lanes, node.lane);
  const where =
    node.kind === "checkpoint"
      ? `${lane?.title ?? ""} ${node.label}`
      : node.kind === "sent"
        ? `Copy sent to ${node.label}`
        : `File that came back, ${node.label}`;
  const milestone = node.milestone ? `, milestone: ${node.milestone}` : "";
  const archived = node.archived ? ", archived" : "";
  return `${where}, ${node.when}, ${node.detail}${milestone}${archived}`;
}

export function Tree({
  graph,
  showArchived,
  selected,
  onSelect,
}: {
  graph: FamilyGraph;
  showArchived: boolean;
  selected: string | undefined;
  onSelect: (id: string) => void;
}) {
  const shape = useMemo(() => layout(graph, showArchived), [graph, showArchived]);
  const buttons = useRef(new Map<string, HTMLButtonElement>());
  const current = shape.nodes.some((n) => n.id === selected) ? selected : shape.nodes.at(-1)?.id;

  const onKey = (event: KeyboardEvent<HTMLButtonElement>, id: string) => {
    if (!KEYS.has(event.key)) {
      return;
    }
    event.preventDefault();
    const next = step(shape, id, event.key as Parameters<typeof step>[2]);
    if (next) {
      onSelect(next);
      buttons.current.get(next)?.focus();
    }
  };

  if (shape.nodes.length === 0) {
    return <p className="panel">Nothing to show yet.</p>;
  }

  return (
    <div className="tree">
      <p id="tree-help" className="visually-hidden">
        Use the arrow keys to move along a row or between rows, and Home or End for a row&apos;s
        first or last point.
      </p>
      <div className="tree__lanes" aria-hidden="true" style={{ height: shape.height }}>
        {shape.lanes.map((l, i) => (
          <div
            key={l.id}
            className={`tree__lane tree__lane--${l.kind}`}
            style={{ top: TOP + i * ROW - 20 }}
          >
            <span className="tree__lane-title">{l.title}</span>
            {l.chip && <span className="secondary small">{l.chip}</span>}
            {l.archived && <span className="secondary small"> · archived</span>}
          </div>
        ))}
      </div>
      <div className="tree__scroll">
        <div
          className="tree__canvas"
          role="group"
          aria-label="Family tree"
          aria-describedby="tree-help"
          style={{ width: shape.width, height: shape.height }}
        >
          <svg
            className="tree__edges"
            width={shape.width}
            height={shape.height}
            aria-hidden="true"
            focusable="false"
          >
            {shape.lanes.map((l, i) => (
              <line
                key={l.id}
                className="tree__rule"
                x1={0}
                x2={shape.width}
                y1={TOP + i * ROW}
                y2={TOP + i * ROW}
              />
            ))}
            {shape.edges.map((e) => (
              <path
                key={`${e.from}-${e.to}-${e.kind}`}
                className={`edge edge--${e.kind}`}
                d={e.d}
              />
            ))}
          </svg>
          {shape.nodes.map((n) => (
            <button
              key={n.id}
              ref={(el) => {
                if (el) {
                  buttons.current.set(n.id, el);
                } else {
                  buttons.current.delete(n.id);
                }
              }}
              type="button"
              className={`node node--${n.kind} node--${laneOf(shape.lanes, n.lane)?.kind ?? "version"}${n.milestone ? " node--milestone" : ""}`}
              style={{ left: n.x - SIZE / 2, top: n.y - SIZE / 2 }}
              tabIndex={n.id === current ? 0 : -1}
              aria-pressed={n.id === selected}
              aria-label={describeNode(n, shape.lanes)}
              title={describeNode(n, shape.lanes)}
              onClick={() => {
                onSelect(n.id);
              }}
              onKeyDown={(event) => {
                onKey(event, n.id);
              }}
            >
              {n.kind === "checkpoint" ? n.label.replace(/^v/, "") : n.kind === "sent" ? "→" : "←"}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}
