// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The Phase 3 feedback journey through the real wasm engine, the storage
// interface and a document host that behaves like Word: a clean copy goes to a
// mentor, the master moves on, the copy comes back edited, and the person keeps
// some of the changes. Then the ways a returned file reaches the panel when it
// is opened in Word itself.

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { loadEngine } from "./engine.js";
import type { Engine } from "./engine.js";
import { Lineage } from "./lineage.js";
import type { Opened } from "./lineage.js";
import { MemoryStorage } from "./storage.js";
import { FakeHost } from "./testing/fake-host.js";

const wasm = readFileSync(new URL("../wasm-pkg/lineage_wasm_bg.wasm", import.meta.url));
const fixture = (name: string): Uint8Array =>
  new Uint8Array(
    readFileSync(new URL(`../../../fixtures/docx/synthetic/${name}`, import.meta.url)),
  );
const MASTER_URL = "https://d.docs.live.net/0000000000000000/Documents/Resume.docx";

let seed = 100;
function engine(): Engine {
  seed += 1;
  return loadEngine(wasm, new Uint8Array(32).fill(seed));
}

function clock(): () => number {
  let now = 1_792_056_600_000;
  return () => (now += 60_000);
}

function expectKind<K extends Opened["kind"]>(
  opened: Opened,
  kind: K,
): Extract<Opened, { kind: K }> {
  expect(opened.kind).toBe(kind);
  return opened as Extract<Opened, { kind: K }>;
}

interface World {
  storage: MemoryStorage;
  now: () => number;
  host: FakeHost;
  master: Lineage;
  family: string;
  /** The copy sent to the mentor, with its ids. */
  copy: Uint8Array;
}

/** A tracked master, a copy sent to a mentor, and the master moved on. */
async function sentToMentor(embedId = true): Promise<World> {
  const storage = new MemoryStorage();
  const now = clock();
  const e = engine();
  const host = new FakeHost(e, fixture("resume-master.docx"), {
    name: "Resume.docx",
    url: MASTER_URL,
  });
  const master = new Lineage({ engine: e, storage, host, now });
  await master.open();
  const started = expectKind(await master.startTracking(), "master");
  const sent = await master.sendCopy(started.file.nodeId, {
    recipient: "Mentor",
    purpose: null,
    embedId,
  });
  const copy = await storage.readFile(sent.ref);
  host.edit(fixture("resume-master-new-job.docx"));
  expect(await master.saveCheckpoint("New job")).toBe(true);
  return { storage, now, host, master, family: started.file.familyId, copy };
}

/**
 * The mentor's edits: the tailored text (summary rewritten, a bullet removed,
 * Skills moved up), in a file that keeps the copy's ids, as Word keeps them.
 */
function edited(e: Engine, copy: Uint8Array): Uint8Array {
  const ids = e.readIds(copy);
  const body = fixture("resume-tailored-data.docx");
  return ids ? e.writeIds(body, ids) : body;
}

describe("a file that comes back", () => {
  it("is recognised, reviewed one change at a time, and kept as tracked changes", async () => {
    const { storage, host, master, family, copy } = await sentToMentor();
    const back = edited(engine(), copy);

    // "A file came back": recognised by its ids.
    const returning = await master.recognise(back, "Resume - comments from Sam.docx");
    expect(returning.recognition.verdict).toBe("certain");
    expect(returning.recognition.headline).toBe("This came back from Mentor");
    const candidate = returning.recognition.candidates[0];
    if (!candidate) {
      throw new Error("no candidate");
    }
    const recorded = await master.recordReturned(returning, candidate, "ids");
    expect(recorded.family).toBe(family);

    // What I've sent, and the master panel.
    const row = (await master.sentList()).groups[0]?.rows[0];
    expect(row?.chip).toBe("Came back with 4 changes");
    expect(row?.button).toBe("Review");
    expect(row?.returned).toBe(recorded.returned);
    const panel = expectKind(await master.refresh(), "master").panel;
    expect(panel.returned.map((r) => [r.text, r.detail])).toEqual([
      ["Mentor sent back your copy from 15 Oct", "4 changes"],
    ]);
    expect(recorded.node).toBe(panel.nodeId);

    // Only the mentor's changes, judged against the master as it is now.
    const review = await master.returnedReview(recorded.returned);
    expect(review.headline).toBe("4 changes from Mentor");
    expect(review.notes[0]).toMatch(/^Your file has changed since this copy left \(it's now v2\)/);
    const ids = review.view.changes.map((c) => c.id);
    const [first, ...rest] = ids;
    expect(first).toBeDefined();
    const before = host.replaced;
    const result = await master.decideReturned(recorded.returned, [
      { card: first ?? "", decision: "bringIn" },
      ...rest.map((card) => ({ card, decision: "decline" as const })),
    ]);
    expect(result).toEqual({ wrote: true });
    expect(host.replaced).toBe(before + 1);
    const after = expectKind(await master.refresh(), "master").panel;
    expect(after.latest.label).toBe("v3");
    expect(after.latest.note).toBe("Kept 1 change from Mentor");
    expect(after.returned).toEqual([]);
    expect((await master.sentList()).groups[0]?.rows[0]?.chip).toBe("Reviewed");

    // The file that came back is kept exactly, and can be opened.
    await master.openReturned(family, recorded.returned);
    expect(Buffer.from(host.copies.at(-1) ?? []).equals(Buffer.from(back))).toBe(true);
    // Dropped in again, it is the same record.
    const again = await master.recordReturned(returning, candidate, "ids");
    expect(again.returned).toBe(recorded.returned);
    expect(await storage.families()).toEqual([family]);
  });

  it("opened in Word with its ids, is never shown as the master", async () => {
    const { storage, now, copy } = await sentToMentor();
    const e = engine();
    const back = edited(e, copy);
    const host = new FakeHost(e, back, {
      name: "Resume (Mentor).docx",
      url: "C:\\Users\\someone\\Downloads\\Resume (Mentor).docx",
    });
    const lineage = new Lineage({ engine: e, storage, host, now });
    const opened = expectKind(await lineage.open(), "cameBack");
    expect(opened.file.copyOf?.recipient).toBe("Mentor");
    expect(opened.file.role).toBe("master");
    expect(opened.returning.recognition.verdict).toBe("certain");
    expect(opened.returning.recognition.byIds).toBe(true);
    // Nothing that needs the file to be the master works here.
    await expect(lineage.saveCheckpoint("oops")).rejects.toThrow();
    const candidate = opened.returning.recognition.candidates[0];
    if (!candidate) {
      throw new Error("no candidate");
    }
    const recorded = await lineage.recordReturned(opened.returning, candidate, "ids");
    await lineage.openNode(recorded.family, recorded.node);
    expect(host.opened).toEqual([{ webUrl: MASTER_URL }]);
  });

  it("opened in Word without ids, is found by its content", async () => {
    const { storage, now, copy } = await sentToMentor(false);
    const e = engine();
    expect(e.readIds(copy)).toBeNull();
    const host = new FakeHost(e, edited(e, copy), {
      name: "Avery Quinn resume.docx",
      url: "C:\\Users\\someone\\Downloads\\Avery Quinn resume.docx",
    });
    const lineage = new Lineage({ engine: e, storage, host, now });
    expectKind(await lineage.open(), "untracked");
    const returning = await lineage.recogniseOpen();
    expect(returning.recognition.verdict).toBe("certain");
    expect(returning.recognition.byIds).toBe(false);
    expect(returning.recognition.candidates[0]?.score).toMatch(/% of the text matches$/);
    expect(returning.name).toBe("Avery Quinn resume.docx");
  });

  it("that Lineage has never seen is not recognised", async () => {
    const { master } = await sentToMentor();
    const returning = await master.recognise(fixture("table.docx"), "Budget.docx");
    expect(returning.recognition.verdict).toBe("unknown");
    expect(returning.recognition.candidates).toEqual([]);
  });
});
