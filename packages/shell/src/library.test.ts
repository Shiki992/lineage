// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// "My documents" through the real wasm engine and memory storage: the library
// and its index, a document's page and tree, compare, a copy exactly as sent,
// restore, milestones, a moved file, and a damaged ledger.

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { loadEngine } from "./engine.js";
import type { Engine } from "./engine.js";
import { Library } from "./library.js";
import { Lineage } from "./lineage.js";
import { MemoryStorage, sha256Hex } from "./storage.js";
import { FakeHost } from "./testing/fake-host.js";

const wasm = readFileSync(new URL("../wasm-pkg/lineage_wasm_bg.wasm", import.meta.url));
const fixture = (name: string): Uint8Array =>
  new Uint8Array(
    readFileSync(new URL(`../../../fixtures/docx/synthetic/${name}`, import.meta.url)),
  );

let seed = 100;
function engine(): Engine {
  seed += 1;
  return loadEngine(wasm, new Uint8Array(32).fill(seed));
}

function clock(): () => number {
  let now = 1_792_056_600_000;
  return () => (now += 60_000);
}

/** A resume family made in "Word": a master at v2, a tailored version, a copy sent. */
async function family(
  storage: MemoryStorage,
  now: () => number,
  versionName = "Resume – data roles",
) {
  const e = engine();
  const host = new FakeHost(e, fixture("resume-master.docx"), {
    name: "Resume.docx",
    url: "https://d.docs.live.net/0000000000000000/Documents/Resume.docx",
  });
  const master = new Lineage({ engine: e, storage, host, now });
  await master.open();
  const started = await master.startTracking();
  if (started.kind !== "master") {
    throw new Error("not a master");
  }
  const version = await master.makeTailoredVersion(versionName);
  host.edit(fixture("resume-master-new-job.docx"));
  await master.saveCheckpoint("New job");
  await master.sendCopy(started.file.nodeId, {
    recipient: "Northgate Analytics",
    purpose: null,
    embedId: true,
  });
  await master.indexed();
  return { familyId: started.file.familyId, version, master: started.file.nodeId };
}

describe("My documents", () => {
  it("lists families from the index, and rebuilds an index that is wrong", async () => {
    const storage = new MemoryStorage();
    const now = clock();
    const made = await family(storage, now);
    // Every write in Word kept the index up to date.
    expect(storage.index).toBeDefined();
    const library = new Library({ engine: engine(), storage, now });
    const quick: unknown[] = [];
    const list = await library.list((l) => quick.push(l));
    expect(quick).toEqual([list]);
    expect(list.entries).toHaveLength(1);
    const entry = list.entries[0];
    expect(entry?.familyId).toBe(made.familyId);
    expect(entry?.summary).toBe("1 tailored version · 3 checkpoints · 1 sent");
    expect(entry?.kind).toBe("Word document");
    const writes = storage.index?.version;

    // A broken index: drawn from nothing, then rebuilt from the ledgers.
    storage.index = { bytes: new TextEncoder().encode("{broken"), version: 41 };
    const drawn: unknown[] = [];
    const rebuilt = await new Library({ engine: engine(), storage, now }).list((l) =>
      drawn.push(l),
    );
    expect(drawn).toEqual([]);
    expect(rebuilt).toEqual(list);
    expect(storage.index.version).toBe(42);

    // No index at all, and a second family: both found.
    storage.index = undefined;
    await family(storage, now, "Resume – platform roles");
    const both = await new Library({ engine: engine(), storage, now }).list();
    expect(both.entries).toHaveLength(2);
    expect(writes).toBeDefined();
  });

  it("opens a document, compares versions, and gives a copy exactly as sent", async () => {
    const storage = new MemoryStorage();
    const now = clock();
    const made = await family(storage, now);
    const library = new Library({ engine: engine(), storage, now });
    const { view } = await library.open(made.familyId);
    expect(view.page.canChange).toBe(true);
    const master = view.page.files[0];
    expect(master?.entries.map((e) => e.label)).toEqual(["v2", "v1"]);
    const [v2, v1] = master?.entries ?? [];
    const changed = await library.compare(made.familyId, v1?.id ?? "", v2?.id ?? "");
    expect(changed.headline).toBe("2 changes from Resume v1 to Resume v2");
    expect(view.graph.lanes.map((l) => l.kind)).toEqual(["master", "version", "sent"]);

    const sent = view.page.sent.groups[0]?.rows[0];
    const bytes = await library.sentCopy(made.familyId, sent?.id ?? "");
    const stored = await storage.readFile("Sent copies/Resume (Northgate Analytics).docx");
    expect(await sha256Hex(bytes)).toBe(await sha256Hex(stored));
  });

  it("restores a checkpoint as a new version and marks a milestone, changing nothing else", async () => {
    const storage = new MemoryStorage();
    const now = clock();
    const made = await family(storage, now);
    const before = new Map(storage.files);
    const ledgerBefore = storage.ledgers.get(made.familyId) ?? new Uint8Array();
    const library = new Library({ engine: engine(), storage, now });
    const { view } = await library.open(made.familyId);
    const v1 = view.page.files[0]?.entries[1];
    expect(await library.freeName("Resume – data roles")).toBe("Resume – data roles (2).docx");
    const restored = await library.restoreAsNewVersion(made.familyId, v1?.id ?? "", "First draft");
    expect(restored.name).toBe("First draft.docx");
    for (const [ref, bytes] of before) {
      expect(storage.files.get(ref)).toEqual(bytes);
    }
    const ledgerAfter = storage.ledgers.get(made.familyId) ?? new Uint8Array();
    expect(
      Buffer.from(ledgerAfter.slice(0, ledgerBefore.length)).equals(Buffer.from(ledgerBefore)),
    ).toBe(true);
    await library.markMilestone(made.familyId, v1?.id ?? "", "First sent");
    const after = (await library.open(made.familyId)).view.page;
    expect(after.files.map((f) => f.name)).toContain("First draft.docx");
    expect(after.files.find((f) => f.name === "First draft.docx")?.status).toBe(
      "Restored from Resume v1",
    );
    expect(after.files[0]?.entries[1]?.milestone).toBe("First sent");
    await library.markMilestone(made.familyId, v1?.id ?? "", null);
    expect(
      (await library.open(made.familyId)).view.page.files[0]?.entries[1]?.milestone,
    ).toBeNull();
    // The index followed every write.
    const listed = await library.list();
    expect(listed.entries[0]?.tailored).toBe(2);
  });

  it("restores with a fresh engine when the master has moved on many times", async () => {
    const storage = new MemoryStorage();
    const now = clock();
    const e = engine();
    const host = new FakeHost(e, fixture("resume-master.docx"), {
      name: "Resume.docx",
      url: "https://d.docs.live.net/0000000000000000/Documents/Resume.docx",
    });
    const master = new Lineage({ engine: e, storage, host, now });
    await master.open();
    const started = await master.startTracking();
    const edits = [
      "resume-master-new-job.docx",
      "resume-master.docx",
      "resume-master-new-job.docx",
      "resume-master.docx",
      "resume-master-new-job.docx",
    ];
    for (const [i, name] of edits.entries()) {
      host.edit(fixture(name));
      await master.saveCheckpoint(`Edit ${String(i + 1)}`);
    }
    await master.indexed();
    const library = new Library({ engine: engine(), storage, now });
    const family = started.kind === "master" ? started.file.familyId : "";
    const { view } = await library.open(family);
    const v1 = view.page.files[0]?.entries.at(-1);
    expect(view.page.files[0]?.entries).toHaveLength(6);
    const made = await library.restoreAsNewVersion(family, v1?.id ?? "", "From the start");
    expect(made.name).toBe("From the start.docx");
  });

  it("finds a moved version and relinks it only to its own file", async () => {
    const storage = new MemoryStorage();
    const now = clock();
    const made = await family(storage, now);
    const library = new Library({ engine: engine(), storage, now });
    const moved = storage.files.get(made.version.ref) ?? new Uint8Array();
    storage.files.delete(made.version.ref);
    storage.files.set("Tailored versions/Data (old).docx", moved);
    storage.files.set(
      "Tailored versions/Someone else.docx",
      storage.files.get("Sent copies/Resume (Northgate Analytics).docx") ?? new Uint8Array(),
    );
    let page = (await library.open(made.familyId)).view.page;
    expect((await library.missingFiles(page)).map((m) => m.name)).toEqual([
      "Resume – data roles.docx",
    ]);
    expect(await library.unlinkedFiles(page)).toEqual(["Data (old).docx", "Someone else.docx"]);
    await expect(
      library.relink(made.familyId, made.version.nodeId, "Someone else.docx"),
    ).rejects.toMatchObject({ kind: "wrongFile" });
    await library.relink(made.familyId, made.version.nodeId, "Data (old).docx");
    page = (await library.open(made.familyId)).view.page;
    expect(await library.missingFiles(page)).toEqual([]);
  });

  it("shows a damaged history from what can be read, and adds nothing to it", async () => {
    const storage = new MemoryStorage();
    const now = clock();
    const made = await family(storage, now);
    const ledger = storage.ledgers.get(made.familyId) ?? new Uint8Array();
    storage.ledgers.set(made.familyId, ledger.slice(0, ledger.length - 20));
    const library = new Library({ engine: engine(), storage, now });
    const list = await library.list();
    expect(list.entries[0]?.canOpen).toBe(false);
    expect(list.entries[0]?.damage).toMatch(/^Lineage could read this history up to 15 Oct\./);
    const { view } = await library.open(made.familyId);
    expect(view.page.canChange).toBe(false);
    const v1 = view.page.files[0]?.entries.at(-1);
    await expect(library.markMilestone(made.familyId, v1?.id ?? "", "x")).rejects.toMatchObject({
      kind: "ledger",
    });
  });
});
