// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Bring in on a tailored deck open in PowerPoint (decision 0019), through the
// real wasm engine and a fake PowerPoint: the steps are carried out, what
// PowerPoint then holds is checked and recorded once, and when anything goes
// wrong the deck is put back and nothing is recorded.

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { EngineError, loadEngine } from "./engine.js";
import type { Engine } from "./engine.js";
import { Lineage, LineageError } from "./lineage.js";
import { MemoryStorage, StorageConflict } from "./storage.js";
import { FakeHost, FakePowerPoint } from "./testing/fake-host.js";

const wasm = readFileSync(new URL("../wasm-pkg/lineage_wasm_bg.wasm", import.meta.url));
const deck = (name: string): Uint8Array =>
  new Uint8Array(
    readFileSync(new URL(`../../../fixtures/pptx/synthetic/${name}`, import.meta.url)),
  );
const URL_ = "https://d.docs.live.net/0000000000000000/Documents/Pitch deck.pptx";

function clock(): () => number {
  let now = 1_792_056_600_000;
  return () => (now += 60_000);
}

let seed = 200;
function engine(): Engine {
  seed += 1;
  return loadEngine(wasm, new Uint8Array(32).fill(seed));
}

/** Refuses the next ledger appends, as when another window wrote first. */
class ConflictingStorage extends MemoryStorage {
  conflicts = 0;

  override async appendLedger(
    family: string,
    expected: string,
    lines: Uint8Array,
  ): Promise<string> {
    if (this.conflicts > 0) {
      this.conflicts -= 1;
      await Promise.resolve();
      throw new StorageConflict("the ledger changed");
    }
    return super.appendLedger(family, expected, lines);
  }
}

/**
 * A master deck with the traction update saved, and the Seed fund version
 * (made before it) open in PowerPoint.
 */
async function openVersion(storage: MemoryStorage, version = "pitch-deck.pptx") {
  const e = engine();
  const master = new FakeHost(e, deck("pitch-deck.pptx"), { name: "Pitch deck.pptx", url: URL_ });
  const lineage = new Lineage({ engine: e, storage, host: master, now: clock() });
  await lineage.open();
  await lineage.startTracking();
  const made = await lineage.makeTailoredVersion("Pitch deck – Seed fund");
  master.edit(deck("pitch-deck-traction-update.pptx"));
  await lineage.saveCheckpoint("New traction numbers");

  const stored = storage.files.get(made.ref);
  if (!stored) throw new Error("no version file");
  const e2 = engine();
  const host = new FakePowerPoint(e2, stored, { name: made.name, url: made.ref });
  if (version !== "pitch-deck.pptx") {
    host.edit(deck(version));
  }
  const pane = new Lineage({ engine: e2, storage, host, now: clock() });
  const opened = await pane.open();
  expect(opened.kind).toBe("version");
  const family = pane.file?.familyId ?? "";
  const ledger = () => new Uint8Array(storage.ledgers.get(family) ?? []);
  return { e: e2, host, pane, made, ledger };
}

describe("bringing changes into an open deck", () => {
  it("carries out the steps, checks the deck and records it once", async () => {
    const storage = new MemoryStorage();
    const { host, pane, ledger } = await openVersion(storage);
    expect(pane.bringsIntoOpenDeck).toBe(true);
    const before = await host.readIds();
    const appends = ledger().length;

    const line = await pane.bringInOpenDeck();
    expect(line.text).toBe("Brought in 2 changes");
    expect(host.applied).toHaveLength(1);
    expect(host.applied[0]?.map((o) => o.kind)).toEqual(["delete", "insert"]);
    expect(ledger().length).toBeGreaterThan(appends);
    const after = await host.readIds();
    expect(after?.checkpoint).not.toBe(before?.checkpoint);

    // The panel now opens on the new checkpoint, with nothing left to bring in.
    const opened = await pane.refresh();
    expect(opened.kind === "version" && opened.panel.updates).toBeNull();
    const again = await pane.bringInOpenDeck();
    expect(again.text).toBe("Already up to date");
    expect(host.applied).toHaveLength(1);
  });

  it("puts the deck back and records nothing when PowerPoint holds something else", async () => {
    const storage = new MemoryStorage();
    const { host, pane, ledger } = await openVersion(storage);
    const ids = await host.readIds();
    const original = await host.readBytes();
    const stored = ledger();
    // PowerPoint holds only part of the change: the text, not the chart.
    host.lands = () => deck("pitch-deck-northgate.pptx");

    const error = await pane.bringInOpenDeck().catch((e: unknown) => e);
    expect(error).toBeInstanceOf(LineageError);
    expect((error as LineageError).userMessage).toBe(
      "PowerPoint didn't take the changes exactly as Lineage made them, so this deck was put back as it was. Nothing was changed.",
    );
    expect(host.applied).toHaveLength(2);
    expect(ledger()).toEqual(stored);
    expect(await host.readIds()).toEqual(ids);
    expect(await host.readBytes()).toEqual(original);
  });

  it("puts the deck back when a step fails halfway", async () => {
    const storage = new MemoryStorage();
    const { host, pane, ledger } = await openVersion(storage);
    const original = await host.readBytes();
    const stored = ledger();
    host.failAt = 1;
    host.halfway = deck("pitch-deck-team-removed.pptx");

    await expect(pane.bringInOpenDeck()).rejects.toThrow(LineageError);
    // Putting back has its own steps: Team goes back in.
    expect(host.applied[1]?.some((o) => o.kind === "insert")).toBe(true);
    expect(await host.readBytes()).toEqual(original);
    expect(ledger()).toEqual(stored);
  });

  it("says plainly when putting the deck back fails too", async () => {
    const storage = new MemoryStorage();
    const { host, pane, ledger } = await openVersion(storage);
    const stored = ledger();
    host.failAt = 1;
    host.halfway = deck("pitch-deck-team-removed.pptx");
    host.failPutBack = true;

    const error = await pane.bringInOpenDeck().catch((e: unknown) => e);
    expect((error as LineageError).userMessage).toBe(
      "PowerPoint didn't take the changes exactly as Lineage made them, and putting this deck back didn't work either. Close it without saving to keep it as it was.",
    );
    expect(ledger()).toEqual(stored);
  });

  it("runs once more when another window wrote first", async () => {
    const storage = new ConflictingStorage();
    const { host, pane } = await openVersion(storage);
    storage.conflicts = 1;
    const line = await pane.bringInOpenDeck();
    expect(line.text).toBe("Brought in 2 changes");
    expect(host.applied).toHaveLength(1);
  });

  it("puts the deck back when the history can't be written", async () => {
    const storage = new ConflictingStorage();
    const { host, pane, ledger } = await openVersion(storage);
    const original = await host.readBytes();
    const ids = await host.readIds();
    const stored = ledger();
    storage.conflicts = 2;
    const error = await pane.bringInOpenDeck().catch((e: unknown) => e);
    expect((error as LineageError).userMessage).toBe(
      "Lineage couldn't record the change in this deck's history, so this deck was put back as it was. Nothing was changed.",
    );
    expect(await host.readBytes()).toEqual(original);
    expect(await host.readIds()).toEqual(ids);
    expect(ledger()).toEqual(stored);
  });

  it("keeps an overlap until the person chooses", async () => {
    // This version's own chart figures, and the master's new ones.
    const storage = new MemoryStorage();
    const { pane } = await openVersion(storage, "pitch-deck-chart-edited.pptx");
    const first = await pane.bringInOpenDeck();
    expect(first.text).toBe("Brought in 1 change · 1 to choose");
    const overlap = first.left.overlaps[0];
    if (!overlap || !pane.file) throw new Error("no overlap");
    const chosen = await pane.bringInOpenDeck([
      { node: pane.file.nodeId, card: overlap.id, decision: "useMasters" },
    ]);
    expect(chosen.text).toBe("Brought in 1 change");
    const done = await pane.bringInOpenDeck();
    expect(done.text).toBe("Already up to date");
  });

  it("records Keep mine without changing a slide", async () => {
    const storage = new MemoryStorage();
    const { host, pane, ledger } = await openVersion(storage, "pitch-deck-chart-edited.pptx");
    const first = await pane.bringInOpenDeck();
    const overlap = first.left.overlaps[0];
    if (!overlap || !pane.file) throw new Error("no overlap");
    expect(host.applied).toHaveLength(1);
    const before = ledger().length;
    const kept = await pane.bringInOpenDeck([
      { node: pane.file.nodeId, card: overlap.id, decision: "keepMine" },
    ]);
    expect(kept.text).toBe("Nothing brought in");
    expect(host.applied).toHaveLength(1);
    expect(ledger().length).toBeGreaterThan(before);
    expect((await pane.bringInOpenDeck()).text).toBe("Already up to date");
  });

  it("leaves a PowerPoint that can't change slides to the master deck", async () => {
    const storage = new MemoryStorage();
    const { host, pane } = await openVersion(storage);
    host.supported = false;
    expect(pane.bringsIntoOpenDeck).toBe(false);
    await expect(pane.bringInOpenDeck()).rejects.toThrow(LineageError);
    expect(host.applied).toHaveLength(0);
  });

  it("checks the merged deck in the engine, not in TypeScript", async () => {
    const storage = new MemoryStorage();
    const { e, host, pane } = await openVersion(storage);
    const file = pane.file;
    if (!file) throw new Error("no file");
    const original = await host.readBytes();
    const ledger = new Uint8Array(storage.ledgers.get(file.familyId) ?? []);
    expect(() =>
      e.finishOpenDeck(ledger, file.nodeId, original, original, [], 1_792_100_000_000),
    ).toThrow(EngineError);
  });
});
