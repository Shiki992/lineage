// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The founder's deck through the real wasm engine and the TypeScript storage
// interface: start tracking a master deck, make two tailored versions, change
// the traction numbers in the master, and "Update 2 versions" from the deck
// panel. When a version's file can't be replaced, every file already replaced
// is put back and nothing is recorded (decision 0018).

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { loadEngine } from "./engine.js";
import type { Engine } from "./engine.js";
import { Lineage } from "./lineage.js";
import { FileChanged, MemoryStorage } from "./storage.js";
import { FakeHost } from "./testing/fake-host.js";

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

let seed = 100;
function engine(): Engine {
  seed += 1;
  return loadEngine(wasm, new Uint8Array(32).fill(seed));
}

/** Refuses to replace one file, as OneDrive does when it changed meanwhile. */
class RefusingStorage extends MemoryStorage {
  refuse: string | undefined;

  override async replaceFile(ref: string, bytes: Uint8Array, expected: string): Promise<string> {
    if (ref === this.refuse) {
      await Promise.resolve();
      throw new FileChanged(ref);
    }
    return super.replaceFile(ref, bytes, expected);
  }
}

async function founder(storage: MemoryStorage) {
  const e = engine();
  const host = new FakeHost(e, deck("pitch-deck.pptx"), { name: "Pitch deck.pptx", url: URL_ });
  const lineage = new Lineage({ engine: e, storage, host, now: clock() });
  await lineage.open();
  const started = await lineage.startTracking();
  expect(started.kind).toBe("master");
  const seedFund = await lineage.makeTailoredVersion("Pitch deck – Seed fund");
  const northgate = await lineage.makeTailoredVersion("Pitch deck – Northgate");
  expect(seedFund.name).toBe("Pitch deck – Seed fund.pptx");
  host.edit(deck("pitch-deck-traction-update.pptx"));
  return { e, host, lineage, seedFund, northgate };
}

describe("the pitch deck panel", () => {
  it("shows what just changed and updates the chosen versions", async () => {
    const storage = new MemoryStorage();
    const { lineage, seedFund, northgate } = await founder(storage);
    const panel = await lineage.deckPanel();
    expect(panel.master.chip).toBe("Master deck");
    expect(panel.justChanged?.headline).toBe("You just changed Slide 6 · Traction");
    expect(panel.justChanged?.cards.map((c) => c.summary)).toEqual([
      "The chart changed: '1200' changed to '2000'",
      "'$1.2M' changed to '$2.0M'",
    ]);
    expect(panel.button).toBe("Update 2 versions");

    const before = storage.files.get(seedFund.ref);
    const run = await lineage.updateVersions([seedFund.nodeId, northgate.nodeId]);
    expect(run.headline).toBe("Updated all 2 versions");
    expect(run.versions.map((v) => v.text)).toEqual([
      "Brought in 2 changes",
      "Brought in 2 changes",
    ]);
    expect(storage.files.get(seedFund.ref)).not.toEqual(before);

    const after = await lineage.deckPanel();
    expect(after.justChanged).toBeNull();
    expect(after.targets.map((t) => t.context)).toEqual(["Up to date", "Up to date"]);
    expect(after.button).toBeNull();
  });

  it("puts every file back when one can't be replaced", async () => {
    const storage = new RefusingStorage();
    const { lineage, seedFund, northgate } = await founder(storage);
    // The checkpoint of the master is its own step; take it first.
    await lineage.saveCheckpoint("New traction numbers");
    const ledger = new Uint8Array(storage.ledgers.get(lineage.file?.familyId ?? "") ?? []);
    const seedBefore = storage.files.get(seedFund.ref);
    const northBefore = storage.files.get(northgate.ref);
    storage.refuse = northgate.ref;
    await expect(
      lineage.updateVersions([seedFund.nodeId, northgate.nodeId]),
    ).rejects.toBeInstanceOf(FileChanged);
    expect(storage.files.get(seedFund.ref)).toEqual(seedBefore);
    expect(storage.files.get(northgate.ref)).toEqual(northBefore);
    expect(storage.ledgers.get(lineage.file?.familyId ?? "")).toEqual(ledger);
    // Once the file can be written, the same press works.
    storage.refuse = undefined;
    const run = await lineage.updateVersions([seedFund.nodeId, northgate.nodeId]);
    expect(run.headline).toBe("Updated all 2 versions");
  });
});
