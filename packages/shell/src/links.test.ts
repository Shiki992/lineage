// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Sending a copy as a link (decision 0032), through the real wasm engine and
// memory storage: the copy that left is shared exactly, edits made through the
// link come back to review, and the link can be stopped.

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { loadEngine } from "./engine.js";
import type { Engine } from "./engine.js";
import { Lineage } from "./lineage.js";
import { MemoryStorage, sha256Hex } from "./storage.js";
import type { Sharing } from "./storage.js";
import { FakeHost } from "./testing/fake-host.js";

const wasm = readFileSync(new URL("../wasm-pkg/lineage_wasm_bg.wasm", import.meta.url));
const fixture = (name: string): Uint8Array =>
  new Uint8Array(
    readFileSync(new URL(`../../../fixtures/docx/synthetic/${name}`, import.meta.url)),
  );
const URL_ = "https://d.docs.live.net/0000000000000000/Documents/Resume.docx";

let seed = 60;
function engine(): Engine {
  seed += 1;
  return loadEngine(wasm, new Uint8Array(32).fill(seed));
}

function clock(): () => number {
  let now = 1_792_056_600_000;
  return () => (now += 60_000);
}

async function sentToMentor(storage: MemoryStorage) {
  const e = engine();
  const host = new FakeHost(e, fixture("resume-master.docx"), { name: "Resume.docx", url: URL_ });
  const lineage = new Lineage({ engine: e, storage, host, now: clock() });
  await lineage.open();
  await lineage.startTracking();
  const node = lineage.file?.nodeId ?? "";
  const copy = await lineage.sendCopy(node, { recipient: "Mentor", purpose: null, embedId: true });
  const list = await lineage.sentList();
  const sent = list.groups[0]?.rows[0]?.id ?? "";
  return { e, host, lineage, copy, sent };
}

describe("a copy shared as a link", () => {
  it("shares the copy that left, sees their edits, takes them back, and stops", async () => {
    const storage = new MemoryStorage();
    const { e, lineage, copy, sent } = await sentToMentor(storage);
    expect(lineage.canShare).toBe(true);
    const left = storage.files.get(copy.ref) ?? new Uint8Array();

    const shared = await lineage.shareLink(sent, "edit", copy.ref);
    expect(shared.url).toBe("https://share.example/link-1");
    expect(shared.line).toBe("Shared as a link · Can edit");
    expect(shared.who).toBe("Anyone with the link can open it");
    expect(storage.links.get("link-1")).toEqual({ ref: copy.ref, access: "edit" });
    // The file shared is exactly what left, and nothing else was written.
    expect(storage.files.get(copy.ref)).toEqual(left);
    expect(await lineage.sharedEdits()).toEqual([]);
    await expect(lineage.shareLink(sent, "view")).rejects.toMatchObject({
      userMessage: "This copy is already shared. Stop sharing it first to make another link.",
    });

    // The mentor edits the shared file through the link.
    const ids = e.readIds(left);
    expect(ids).not.toBeNull();
    const theirs = ids ? e.writeIds(fixture("resume-master-new-job.docx"), ids) : left;
    storage.files.set(copy.ref, theirs);
    expect(await lineage.sharedEdits()).toEqual([sent]);

    // It comes back by its ids, as any file that came back.
    const { bytes, name } = await lineage.sharedFile(sent);
    expect(name).toBe("Resume (Mentor).docx");
    const returning = await lineage.recognise(bytes, name);
    const candidate = returning.recognition.candidates[0];
    expect(candidate?.sent).toBe(sent);
    if (!candidate) {
      throw new Error("not recognised");
    }
    await lineage.recordReturned(returning, candidate, "ids");
    // Once recorded, the same bytes are no longer news.
    expect(await lineage.sharedEdits()).toEqual([]);

    await lineage.stopSharing(sent);
    expect(storage.links.size).toBe(0);
    const row = (await lineage.sentList()).groups[0]?.rows[0];
    expect(row?.link?.live).toBe(false);
    expect(row?.link?.line).toMatch(/^Link stopped /);
    expect(row?.shared).toBeNull();
    // Stopping again does nothing.
    await lineage.stopSharing(sent);
  });

  it("finds an older copy by its bytes, or puts it back from history, before sharing", async () => {
    const storage = new MemoryStorage();
    const { lineage, copy, sent } = await sentToMentor(storage);
    const left = storage.files.get(copy.ref) ?? new Uint8Array();

    // No ref: the file in Sent copies with the same bytes.
    await lineage.shareLink(sent, "view");
    expect(storage.links.get("link-1")?.ref).toBe(copy.ref);
    await lineage.stopSharing(sent);

    // Deleted from Sent copies: written there again, exactly as it left.
    storage.files.delete(copy.ref);
    await lineage.shareLink(sent, "view");
    const ref = [...storage.links.values()].at(-1)?.ref ?? "";
    expect(ref).toBe("Sent copies/Resume (Mentor).docx");
    expect(await sha256Hex(storage.files.get(ref) ?? new Uint8Array())).toBe(await sha256Hex(left));
  });

  it("takes a link away again when the history can't record it", async () => {
    const storage = new MemoryStorage();
    const { lineage, copy, sent } = await sentToMentor(storage);
    storage.beforeAppend = () => {
      throw new Error("OneDrive is unreachable");
    };
    await expect(lineage.shareLink(sent, "edit", copy.ref)).rejects.toThrow(/unreachable/);
    expect(storage.links.size).toBe(0);
  });

  it("isn't offered by a storage that can't share", async () => {
    const plain = new MemoryStorage() as MemoryStorage & { sharing?: Sharing };
    Object.defineProperty(plain, "sharing", { value: undefined });
    const { lineage, sent } = await sentToMentor(plain);
    expect(lineage.canShare).toBe(false);
    await expect(lineage.shareLink(sent, "view")).rejects.toMatchObject({
      userMessage: "memory can't share files as links.",
    });
  });
});
