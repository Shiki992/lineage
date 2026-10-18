// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Bring in on a tailored workbook open in Excel (decision 0023), through the
// real wasm engine and a fake Excel: the plan is carried out, what Excel then
// holds is checked and recorded once, and when anything goes wrong the steps
// that ran are undone, the engine checks the workbook is back, and nothing is
// recorded.

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { loadEngine } from "./engine.js";
import type { Engine } from "./engine.js";
import { Lineage, LineageError } from "./lineage.js";
import type * as s from "./schemas.js";
import { MemoryStorage, StorageConflict } from "./storage.js";
import { FakeExcel, FakeHost } from "./testing/fake-host.js";

const wasm = readFileSync(new URL("../wasm-pkg/lineage_wasm_bg.wasm", import.meta.url));
const workbook = (name: string): Uint8Array =>
  new Uint8Array(
    readFileSync(new URL(`../../../fixtures/xlsx/synthetic/${name}`, import.meta.url)),
  );
const URL_ = "https://d.docs.live.net/0000000000000000/Documents/Startup model.xlsx";

function clock(): () => number {
  let now = 1_792_314_000_000;
  return () => (now += 60_000);
}

let seed = 300;
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
 * The master model with its new price saved and offered, and a tailored
 * version (made before it, with no edits of its own) open in Excel.
 */
async function openVersion(storage: MemoryStorage) {
  const e = engine();
  const master = new FakeHost(e, workbook("model.xlsx"), { name: "Startup model.xlsx", url: URL_ });
  const lineage = new Lineage({ engine: e, storage, host: master, now: clock() });
  await lineage.open();
  await lineage.startTracking();
  const made = await lineage.makeTailoredVersion("Startup model – Northwind");
  master.edit(workbook("model-price-update.xlsx"));
  await lineage.saveCheckpoint("New price");
  await lineage.offerToVersions([made.nodeId]);

  const stored = storage.files.get(made.ref);
  if (!stored) throw new Error("no version file");
  const e2 = engine();
  const host = new FakeExcel(e2, stored, { name: made.name, url: made.ref });
  const pane = new Lineage({ engine: e2, storage, host, now: clock() });
  const opened = await pane.open();
  expect(opened.kind).toBe("version");
  const family = pane.file?.familyId ?? "";
  const ledger = () => new Uint8Array(storage.ledgers.get(family) ?? []);
  const updates = await pane.updates();
  const card = updates.groups[0]?.view.workbook?.groups[0]?.cards[0];
  if (!card) throw new Error("no update card");
  const cards: s.CardDecision[] = card.ids.map((id) => ({ card: id, decision: "bringIn" }));
  return { host, pane, made, ledger, cards, original: await host.readBytes() };
}

describe("bringing changes into an open workbook", () => {
  it("names a tailored workbook with its own extension", async () => {
    const { made } = await openVersion(new MemoryStorage());
    expect(made.name).toBe("Startup model – Northwind.xlsx");
  });

  it("carries out the plan, checks the workbook and records it once", async () => {
    const storage = new MemoryStorage();
    const { host, pane, ledger, cards } = await openVersion(storage);
    expect(pane.bringsIntoOpenWorkbook).toBe(true);
    const before = await host.readIds();
    const appends = ledger().length;
    host.holds.push(workbook("model-price-update.xlsx"));

    const plan = await pane.bringInWorkbook(cards);
    expect(plan.text).toBe("Brings in 1 change");
    expect(host.applied).toEqual([
      [
        {
          kind: "set",
          sheet: "Assumptions",
          address: "B4",
          value: { kind: "number", text: "55" },
        },
      ],
    ]);
    expect(ledger().length).toBeGreaterThan(appends);
    expect((await host.readIds())?.checkpoint).not.toBe(before?.checkpoint);
    expect((await pane.updates()).headline).toBe("You're up to date with the master");
  });

  it("puts the workbook back and records nothing when Excel holds something else", async () => {
    const storage = new MemoryStorage();
    const { host, pane, ledger, cards, original } = await openVersion(storage);
    const ids = await host.readIds();
    const stored = ledger();
    // Excel holds a different change; the way back brings the original back.
    host.holds.push(workbook("model-hosting.xlsx"), original);

    const error = await pane.bringInWorkbook(cards).catch((e: unknown) => e);
    expect(error).toBeInstanceOf(LineageError);
    expect((error as LineageError).userMessage).toBe(
      "Excel didn't take the changes exactly as Lineage made them, so this workbook was put back as it was. Nothing was changed.",
    );
    expect(host.applied).toHaveLength(2);
    expect(host.applied[1]).toEqual([
      {
        kind: "set",
        sheet: "Assumptions",
        address: "B4",
        value: { kind: "number", text: "49" },
      },
    ]);
    expect(ledger()).toEqual(stored);
    expect(await host.readIds()).toEqual(ids);
  });

  it("undoes only what ran when Excel stops partway", async () => {
    const storage = new MemoryStorage();
    const { host, pane, ledger, cards } = await openVersion(storage);
    const stored = ledger();
    host.stopAt = 0;

    await expect(pane.bringInWorkbook(cards)).rejects.toThrow(LineageError);
    expect(host.applied).toHaveLength(1);
    expect(ledger()).toEqual(stored);
  });

  it("says plainly when putting the workbook back fails too", async () => {
    const storage = new MemoryStorage();
    const { host, pane, ledger, cards } = await openVersion(storage);
    const stored = ledger();
    host.holds.push(workbook("model-hosting.xlsx"), workbook("model-hosting.xlsx"));

    const error = await pane.bringInWorkbook(cards).catch((e: unknown) => e);
    expect((error as LineageError).userMessage).toBe(
      "Excel didn't take the changes exactly as Lineage made them, and putting this workbook back didn't work either. Close it without saving to keep it as it was.",
    );
    expect(ledger()).toEqual(stored);
  });

  it("runs once more when another window wrote first", async () => {
    const storage = new ConflictingStorage();
    const { host, pane, cards } = await openVersion(storage);
    storage.conflicts = 1;
    host.holds.push(workbook("model-price-update.xlsx"));
    const plan = await pane.bringInWorkbook(cards);
    expect(plan.brought).toBe(1);
    expect(host.applied).toHaveLength(1);
  });

  it("puts the workbook back when the history can't be written", async () => {
    const storage = new ConflictingStorage();
    const { host, pane, ledger, cards, original } = await openVersion(storage);
    const ids = await host.readIds();
    const stored = ledger();
    storage.conflicts = 2;
    host.holds.push(workbook("model-price-update.xlsx"), original);
    const error = await pane.bringInWorkbook(cards).catch((e: unknown) => e);
    expect((error as LineageError).userMessage).toBe(
      "Lineage couldn't record the change in this workbook's history, so this workbook was put back as it was. Nothing was changed.",
    );
    expect(await host.readIds()).toEqual(ids);
    expect(ledger()).toEqual(stored);
  });

  it("asks for a newer Excel when this one can't carry out plans", async () => {
    const { host, pane, cards } = await openVersion(new MemoryStorage());
    host.supported = false;
    expect(pane.bringsIntoOpenWorkbook).toBe(false);
    const error = await pane.bringInWorkbook(cards).catch((e: unknown) => e);
    expect((error as LineageError).userMessage).toBe(
      "This version of Excel can't take the changes. Update Excel, then try again.",
    );
    expect(host.applied).toHaveLength(0);
  });
});

describe("what changed in an open workbook", () => {
  it("marks what the person edited in Excel this session", async () => {
    const storage = new MemoryStorage();
    const e = engine();
    const host = new FakeExcel(e, workbook("model.xlsx"), {
      name: "Startup model.xlsx",
      url: URL_,
    });
    const pane = new Lineage({ engine: e, storage, host, now: clock() });
    await pane.open();
    await pane.startTracking();
    host.edit(workbook("model-price-update.xlsx"));
    host.hints = [{ sheet: "Assumptions", address: "B4" }];
    const view = await pane.whatChanged();
    const card = view.workbook?.groups[0]?.cards[0];
    expect(card?.summary).toBe("'$49' changed to '$55'");
    expect(card?.why).toBe("Edited in this session");
    const side = await pane.cellSideBySide("Assumptions", "B4");
    expect([side.before?.shown, side.after?.shown]).toEqual(["$49", "$55"]);
  });

  it("says what goes with a copy before it is sent", async () => {
    const e = engine();
    const host = new FakeExcel(e, workbook("awkward-cells.xlsx"), {
      name: "Awkward.xlsx",
      url: URL_,
    });
    const pane = new Lineage({ engine: e, storage: new MemoryStorage(), host, now: clock() });
    await pane.open();
    await pane.startTracking();
    expect(await pane.goesWithCopy()).toEqual(["2 hidden sheets go with this copy"]);
  });
});
