// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Every golden reply written by crates/wasm/tests/golden.rs passes the schemas,
// so a field added or renamed on one side of the boundary fails here.

import { readdirSync, readFileSync } from "node:fs";
import * as v from "valibot";
import { describe, expect, it } from "vitest";
import * as s from "./schemas.js";

const dir = new URL("../test/golden/", import.meta.url);
const files = readdirSync(dir)
  .filter((n) => n.endsWith(".json"))
  .sort();

function golden(name: string): unknown {
  return JSON.parse(readFileSync(new URL(`${name}.json`, dir), "utf8"));
}

describe("golden replies", () => {
  it("are all here", () => {
    expect(files.length).toBeGreaterThanOrEqual(25);
  });

  it.each(files)("%s is a valid envelope with a valid value", (file) => {
    const name = file.replace(/\.json$/, "");
    const envelope = v.parse(s.Envelope, golden(name));
    if (envelope.ok) {
      const schema = s.goldenValues[name];
      expect(schema, `no schema listed for ${name}`).toBeDefined();
      if (schema) {
        const result = v.safeParse(schema, envelope.value);
        expect(result.issues ?? []).toEqual([]);
      }
    } else {
      expect(name.startsWith("error-")).toBe(true);
    }
  });

  it("has every kind of error", () => {
    const kinds = files
      .filter((f) => f.startsWith("error-"))
      .map(
        (f) =>
          (v.parse(s.Envelope, golden(f.replace(/\.json$/, ""))) as { error: { kind: string } })
            .error.kind,
      );
    expect(new Set(kinds)).toEqual(
      new Set(["missingBlobs", "wrongFile", "badInput", "notAllowed", "engine"]),
    );
    const missing = golden("error-missing-blobs") as { error: { hashes: string[] } };
    expect(missing.error.hashes.length).toBeGreaterThan(0);
    const elsewhere = v.parse(s.Envelope, golden("error-missing-blobs-family"));
    expect(!elsewhere.ok && elsewhere.error.family).toMatch(/^[0-9a-f]{32}$/);
  });
});

describe("schemas", () => {
  const panel = (golden("master-panel-callout") as { value: Record<string, unknown> }).value;

  it("refuse unknown fields", () => {
    expect(v.safeParse(s.MasterPanel, { ...panel, extra: 1 }).success).toBe(false);
  });

  it("refuse the wrong types and malformed ids", () => {
    expect(v.safeParse(s.MasterPanel, { ...panel, versions: "none" }).success).toBe(false);
    expect(v.safeParse(s.MasterPanel, { ...panel, nodeId: "../x" }).success).toBe(false);
    expect(v.safeParse(s.Ids, { family: "A".repeat(32), checkpoint: "a".repeat(32) }).success).toBe(
      false,
    );
  });

  it("refuse a decision the engine doesn't know", () => {
    expect(v.safeParse(s.CardDecision, { card: "c1", decision: "maybe" }).success).toBe(false);
  });

  it("trim and require a recipient", () => {
    expect(v.safeParse(s.SendArgs, { recipient: "  ", purpose: null, embedId: true }).success).toBe(
      false,
    );
    expect(
      v.parse(s.SendArgs, { recipient: " Northgate ", purpose: null, embedId: false }).recipient,
    ).toBe("Northgate");
  });

  it("say which copy came back, and when Lineage must ask", () => {
    const value = (name: string) =>
      v.parse(s.Recognition, (golden(name) as { value: unknown }).value);
    const certain = value("recognise-certain");
    expect(certain.verdict).toBe("certain");
    expect(certain.candidates).toHaveLength(1);
    const choose = value("recognise-choose");
    expect(choose.verdict).toBe("choose");
    expect(choose.candidates.length).toBeGreaterThan(1);
    const unknown = value("recognise-unknown");
    expect(unknown.candidates).toEqual([]);
    expect(
      v.safeParse(s.RecordArgs, {
        name: "x.docx",
        checkpoint: "a".repeat(32),
        sent: null,
        matchedBy: "guess",
      }).success,
    ).toBe(false);
  });

  it("draw the review of a returned file with keep and skip", () => {
    const review = v.parse(s.ReturnReview, (golden("returned-review") as { value: unknown }).value);
    expect(review.view.changes[0]?.choices.map((c) => c.label)).toEqual(["Keep", "Skip"]);
  });

  it("draw the update cards the engine sends", () => {
    const updates = v.parse(s.Updates, (golden("updates") as { value: unknown }).value);
    const views = updates.groups.map((g) => g.view);
    expect(views.some((w) => w.overlaps.length > 0)).toBe(true);
    const byHand = views.flatMap((w) => w.byHand);
    expect(byHand.length).toBeGreaterThan(0);
    expect(
      byHand.every(
        (c) => c.byHand !== null && c.choices.map((x) => x.decision).join() === "decline",
      ),
    ).toBe(true);
  });
});
