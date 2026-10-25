// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The Phase 1 "done when" scenario again, now through the real wasm engine,
// the TypeScript storage interface and a document host that behaves like Word:
// start tracking a master resume, make two tailored versions, add a job to the
// master and offer it, bring it into one version and decline it in the other,
// send a copy, then open the sent copy.

import { readFileSync } from "node:fs";
import { beforeEach, describe, expect, it } from "vitest";
import { EngineError, loadEngine } from "./engine.js";
import type { Engine } from "./engine.js";
import { Lineage, LineageError } from "./lineage.js";
import type { Opened } from "./lineage.js";
import { MemoryStorage } from "./storage.js";
import { FakeHost } from "./testing/fake-host.js";

const wasm = readFileSync(new URL("../wasm-pkg/lineage_wasm_bg.wasm", import.meta.url));
const fixture = (name: string): Uint8Array =>
  new Uint8Array(
    readFileSync(new URL(`../../../fixtures/docx/synthetic/${name}`, import.meta.url)),
  );

const MASTER_URL = "https://d.docs.live.net/0000000000000000/Documents/Resume.docx";
const ledgerText = (storage: MemoryStorage, family: string): string =>
  new TextDecoder().decode(storage.ledgers.get(family));

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

let seed = 0;
function engine(): Engine {
  seed += 1;
  return loadEngine(wasm, new Uint8Array(32).fill(seed));
}

interface World {
  storage: MemoryStorage;
  now: () => number;
  masterHost: FakeHost;
  master: Lineage;
  family: string;
}

async function tracked(): Promise<World> {
  const storage = new MemoryStorage();
  const now = clock();
  const e = engine();
  const masterHost = new FakeHost(e, fixture("resume-master.docx"), {
    name: "Resume.docx",
    url: MASTER_URL,
  });
  const master = new Lineage({ engine: e, storage, host: masterHost, now });
  const untracked = expectKind(await master.open(), "untracked");
  expect(untracked).toEqual({ kind: "untracked", name: "Resume.docx", saved: true });
  const started = expectKind(await master.startTracking(), "master");
  expect(started.panel.chip).toBe("Master");
  expect(started.panel.follows).toBe("No tailored versions yet");
  expect(started.panel.latest.label).toBe("v1");
  expect(masterHost.saves).toBe(1);
  return { storage, now, masterHost, master, family: started.file.familyId };
}

/** A tailored version opened in Word, on another device: a fresh engine. */
async function openVersion(
  world: World,
  name: string,
): Promise<{ host: FakeHost; lineage: Lineage; opened: Opened }> {
  const e = engine();
  const bytes = await world.storage.readFile(world.storage.refFor("version", name));
  const host = new FakeHost(e, bytes, { name, url: `https://example.invalid/${name}` });
  const lineage = new Lineage({ engine: e, storage: world.storage, host, now: world.now });
  return { host, lineage, opened: await lineage.open() };
}

describe("a master that moved", () => {
  it("is relinked when Word opens it from a new place of the same kind", async () => {
    const world = await tracked();
    const bytes = await world.masterHost.readBytes();
    const movedUrl = "https://d.docs.live.net/0000000000000000/Jobs/Resume (2026).docx";
    const host = new FakeHost(engine(), bytes, { name: "Resume (2026).docx", url: movedUrl });
    const lineage = new Lineage({ engine: engine(), storage: world.storage, host, now: world.now });
    const opened = expectKind(await lineage.open(), "master");
    expect(opened.file.storageRef).toBe(movedUrl);
    expect(ledgerText(world.storage, world.family)).toContain('"type":"NodeRelinked"');

    // The same file in Word on the web has another kind of address: not a move.
    const web = new FakeHost(engine(), bytes, {
      name: "Resume (2026).docx",
      url: "https://onedrive.live.com/edit?id=ABC&resid=0000",
    });
    const before = ledgerText(world.storage, world.family);
    await new Lineage({
      engine: engine(),
      storage: world.storage,
      host: web,
      now: world.now,
    }).open();
    expect(ledgerText(world.storage, world.family)).toBe(before);
  });
});

describe("the personal scenario through wasm", () => {
  let world: World;
  beforeEach(async () => {
    world = await tracked();
  });

  it("runs end to end", async () => {
    const { storage, masterHost, master, family } = world;

    // Make two tailored versions.
    const data = await master.makeTailoredVersion("Resume – data roles");
    expect(data.name).toBe("Resume – data roles.docx");
    await master.makeTailoredVersion("Resume – platform roles.docx");
    expect(await storage.fileNames("version")).toEqual([
      "Resume – data roles.docx",
      "Resume – platform roles.docx",
    ]);
    let panel = expectKind(await master.refresh(), "master").panel;
    expect(panel.follows).toBe("2 tailored versions follow this file");
    expect(panel.versions.map((v) => [v.name, v.status])).toEqual([
      ["Resume – data roles.docx", "Has v1"],
      ["Resume – platform roles.docx", "Has v1"],
    ]);
    await master.openStored(panel.versions[0]?.storageRef ?? "");
    expect(masterHost.opened).toEqual([{ path: data.ref }]);

    // Add a job to the master in Word, see what changed, save, offer.
    masterHost.edit(fixture("resume-master-new-job.docx"));
    const changed = await master.whatChanged();
    expect(changed.headline).toBe("2 changes since v1");
    expect(await master.saveCheckpoint("New job")).toBe(true);
    expect(await master.saveCheckpoint("Again")).toBe(false);
    panel = expectKind(await master.refresh(), "master").panel;
    expect(panel.latest.label).toBe("v2");
    const callout = panel.callout;
    expect(callout?.button).toBe("Offer them to both versions");
    await master.offerToVersions(callout?.versions ?? []);
    expect(expectKind(await master.refresh(), "master").panel.callout).toBeNull();

    // Bring it into one version.
    const one = await openVersion(world, "Resume – data roles.docx");
    expect(expectKind(one.opened, "version").panel.updates).toBe("2 updates from the master");
    const updates = await one.lineage.updates();
    const cards = updates.groups
      .flatMap((g) => g.view.changes)
      .map((c) => ({ card: c.id, decision: "bringIn" as const }));
    expect(cards).toHaveLength(2);
    expect(await one.lineage.decide(cards)).toEqual({ wrote: true });
    expect(one.host.replaced).toBe(1);
    expect((await one.lineage.updates()).headline).toBe("You're up to date with the master");
    const onePanel = expectKind(await one.lineage.refresh(), "version").panel;
    expect(onePanel.status).toBe("Made from master v1 · has v2");
    expect(onePanel.latest.note).toBe("Brought in 2 changes from master v2");

    // Decline it in the other.
    const two = await openVersion(world, "Resume – platform roles.docx");
    const declined = (await two.lineage.updates()).groups
      .flatMap((g) => g.view.changes)
      .map((c) => ({ card: c.id, decision: "decline" as const }));
    expect(await two.lineage.decide(declined)).toEqual({ wrote: false });
    expect(two.host.replaced).toBe(0);
    expect(expectKind(await two.lineage.refresh(), "version").panel.updates).toBeNull();

    // Send a copy of the first version, then open what they received.
    const sent = await one.lineage.sendCopy(onePanel.nodeId, {
      recipient: "Northgate Analytics",
      purpose: "Data engineer",
      embedId: true,
    });
    expect(sent.name).toBe("Resume – data roles (Northgate Analytics).docx");
    const list = await one.lineage.sentList();
    const row = list.groups[0]?.rows[0];
    expect(row?.recipient).toBe("Northgate Analytics");
    await one.lineage.openSentCopy(row?.id ?? "");
    const opened = one.host.copies[0];
    expect(opened).toBeDefined();
    expect(Buffer.from(opened ?? []).equals(Buffer.from(await storage.readFile(sent.ref)))).toBe(
      true,
    );
    expect(engine().readIds(opened ?? new Uint8Array())?.family).toBe(family);

    // A second copy to the same person gets its own name.
    const again = await one.lineage.sendCopy(onePanel.nodeId, {
      recipient: "Northgate Analytics",
      purpose: null,
      embedId: false,
    });
    expect(again.name).toBe("Resume – data roles (Northgate Analytics) (2).docx");
    expect(engine().readIds(await storage.readFile(again.ref))).toBeNull();

    // Back on the master.
    panel = expectKind(await master.refresh(), "master").panel;
    expect(panel.versions.map((v) => v.status)).toEqual(["Has v2 · last sent 15 Oct", "Has v2"]);
    const text = ledgerText(storage, family);
    for (const event of ["FamilyStarted", "UpdateOffered", "UpdateApplied", "CopySent"]) {
      expect(text).toContain(`"type":"${event}"`);
    }
  });

  it("sends a copy of a version that isn't open, from the family list", async () => {
    const { master } = world;
    await master.makeTailoredVersion("Resume – data roles");
    const family = await master.family();
    const version = family.nodes.find((n) => n.role === "variant");
    expect(version).toBeDefined();
    const sent = await master.sendCopy(version?.id ?? "", {
      recipient: "Mentor",
      purpose: null,
      embedId: true,
    });
    expect(sent.name).toBe("Resume – data roles (Mentor).docx");
    const masterNode = family.nodes.find((n) => n.role === "master");
    await expect(
      new Lineage({
        engine: engine(),
        storage: world.storage,
        host: (await openVersion(world, "Resume – data roles.docx")).host,
        now: world.now,
      }).sendCopy(masterNode?.id ?? "", { recipient: "X", purpose: null, embedId: true }),
    ).rejects.toBeInstanceOf(LineageError);
  });

  it("runs a command again when another device appended first", async () => {
    const { storage, masterHost, master, family } = world;
    const other = await openVersion(world, "Resume.docx").catch(() => undefined);
    expect(other).toBeUndefined();
    // Another device makes a tailored version while this one saves a checkpoint.
    const elsewhere = new Lineage({
      engine: engine(),
      storage,
      host: new FakeHost(engine(), await masterHost.readBytes(), {
        name: "Resume.docx",
        url: MASTER_URL,
      }),
      now: world.now,
    });
    await elsewhere.open();
    let injected = false;
    storage.beforeAppend = async () => {
      if (injected) {
        return;
      }
      injected = true;
      storage.beforeAppend = undefined;
      await elsewhere.makeTailoredVersion("Made elsewhere");
    };
    masterHost.edit(fixture("resume-master-new-job.docx"));
    expect(await master.saveCheckpoint("New job")).toBe(true);
    expect(injected).toBe(true);
    const panel = expectKind(await master.refresh(), "master").panel;
    expect(panel.latest.label).toBe("v2");
    expect(panel.versions.map((v) => v.name)).toEqual(["Made elsewhere.docx"]);
    const ids = await masterHost.readIds();
    expect(ledgerText(storage, family)).toContain(ids?.checkpoint ?? "missing");
  });

  it("puts the file back and records nothing when Word doesn't take the changes", async () => {
    const { storage, masterHost, master, family } = world;
    await master.makeTailoredVersion("Resume – data roles");
    masterHost.edit(fixture("resume-master-new-job.docx"));
    await master.saveCheckpoint("New job");
    const panel = expectKind(await master.refresh(), "master").panel;
    await master.offerToVersions(panel.callout?.versions ?? []);

    const one = await openVersion(world, "Resume – data roles.docx");
    const original = await one.host.readBytes();
    // Word drops the tracked changes on the way in.
    one.host.mangle = () => original;
    const cards = (await one.lineage.updates()).groups
      .flatMap((g) => g.view.changes)
      .map((c) => ({ card: c.id, decision: "bringIn" as const }));
    const before = ledgerText(storage, family);
    await expect(one.lineage.decide(cards)).rejects.toThrow(LineageError);
    expect(ledgerText(storage, family)).toBe(before);
    expect(one.host.replaced).toBe(2);
    expect(Buffer.from(await one.host.readBytes()).equals(Buffer.from(original))).toBe(true);
  });

  it("refuses to track a file that was never saved, and spots a family kept elsewhere", async () => {
    const e = engine();
    const unsaved = new Lineage({
      engine: e,
      storage: new MemoryStorage(),
      host: new FakeHost(e, fixture("resume-master.docx"), { name: "Document1", url: null }),
    });
    expect(expectKind(await unsaved.open(), "untracked").saved).toBe(false);
    await expect(unsaved.startTracking()).rejects.toMatchObject({
      userMessage: "Save this file first, then start tracking it.",
    });

    const tracked = await world.masterHost.readBytes();
    const elsewhere = new Lineage({
      engine: e,
      storage: new MemoryStorage(),
      host: new FakeHost(e, tracked, { name: "Resume.docx", url: MASTER_URL }),
    });
    const found = expectKind(await elsewhere.open(), "elsewhere");
    expect(found.ids.family).toBe(world.family);
    expect(found.where).toBe("unknown");
  });

  it("says a tracked file in someone else's drive was shared with you, and changes nothing", async () => {
    const e = engine();
    const tracked = await world.masterHost.readBytes();
    const asked: string[] = [];
    const theirs = Object.assign(new MemoryStorage(), {
      holds: (url: string) => {
        asked.push(url);
        return Promise.resolve(false);
      },
    });
    const host = new FakeHost(e, tracked, { name: "Resume.docx", url: MASTER_URL });
    const shared = new Lineage({ engine: e, storage: theirs, host });
    expect(expectKind(await shared.open(), "elsewhere").where).toBe("sharedWithYou");
    expect(asked).toEqual([MASTER_URL]);
    // Nothing was written: not the file's ids, not a ledger.
    expect(await host.readIds()).toEqual(
      await new FakeHost(e, tracked, { name: "Resume.docx", url: MASTER_URL }).readIds(),
    );
    expect(theirs.ledgers.size).toBe(0);
    // The engine refuses to give a tracked file new ids.
    await expect(shared.startTracking()).rejects.toThrow(/already tracked/);
    expect(theirs.ledgers.size).toBe(0);

    // A storage that can't tell, or fails to, says "unknown".
    const failing = Object.assign(new MemoryStorage(), {
      holds: () => Promise.reject(new Error("offline")),
    });
    const unknown = new Lineage({ engine: e, storage: failing, host });
    expect(expectKind(await unknown.open(), "elsewhere").where).toBe("unknown");
  });

  it("reports the engine's own words for the wrong file", () => {
    const e = engine();
    const ledger = world.storage.ledgers.get(world.family) ?? new Uint8Array();
    const family = e.family(ledger);
    const masterNode = family.nodes[0]?.id ?? "";
    try {
      e.whatChanged(ledger, masterNode, fixture("resume-master.docx"));
      expect.unreachable();
    } catch (error) {
      expect(error).toBeInstanceOf(EngineError);
      expect((error as EngineError).kind).toBe("wrongFile");
      expect((error as EngineError).userMessage).toContain("isn't the version Lineage expected");
    }
  });
});
