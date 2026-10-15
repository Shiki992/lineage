// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import { StorageConflict, StorageError, sha256Hex } from "@lineage/shell";
import { storageConformance } from "@lineage/shell/testing";
import { describe, expect, it } from "vitest";
import { Graph } from "./graph.js";
import { appFolder, nextToMaster } from "./layout.js";
import { OneDriveStorage } from "./onedrive-storage.js";
import { FakeGraph } from "./testing/fake-graph.js";

const FAMILY = "0123456789abcdef0123456789abcdef";
const text = new TextEncoder();
const read = (b: Uint8Array | undefined) => new TextDecoder().decode(b);

function setup(layout = appFolder) {
  const fake = new FakeGraph();
  const tokens: boolean[] = [];
  const graph = new Graph({
    token: ({ forceRefresh }) => {
      tokens.push(forceRefresh);
      return Promise.resolve("token");
    },
    fetch: fake.fetch,
    sleep: () => Promise.resolve(),
  });
  return { fake, tokens, storage: new OneDriveStorage(graph, layout) };
}

storageConformance("OneDriveStorage (app folder)", () => setup().storage);
storageConformance("OneDriveStorage (next to the master)", () => {
  const { fake, storage } = setup(nextToMaster("https://d.docs.live.net/d1/Documents/Resume.docx"));
  fake.file(fake.drive, "Documents/Resume.docx", text.encode("master"));
  return storage;
});

describe("OneDriveStorage", () => {
  it("keeps everything in the app folder, at the layout BLUEPRINT gives", async () => {
    const { fake, storage } = setup();
    const empty = await storage.readLedger(FAMILY);
    await storage.appendLedger(FAMILY, empty.version, text.encode("{}\n"));
    const blob = text.encode("part");
    await storage.putBlob(FAMILY, await sha256Hex(blob), blob);
    await storage.writeFile("version", "Resume – data roles.docx", text.encode("v"));
    await storage.writeFile("sent", "Resume (Northgate).docx", text.encode("c"));
    expect(read(fake.read("me", `Apps/Lineage/${FAMILY}/ledger.jsonl`))).toBe("{}\n");
    expect(read(fake.read("me", `Apps/Lineage/${FAMILY}/blobs/${await sha256Hex(blob)}`))).toBe(
      "part",
    );
    expect(read(fake.read("me", "Apps/Lineage/Tailored versions/Resume – data roles.docx"))).toBe(
      "v",
    );
    expect(read(fake.read("me", "Apps/Lineage/Sent copies/Resume (Northgate).docx"))).toBe("c");
    expect(await storage.families()).toEqual([FAMILY]);
    expect(storage.layout.scopes).toEqual(["Files.ReadWrite.AppFolder"]);
  });

  it("puts tailored versions beside a master in OneDrive, with history in a Lineage folder", async () => {
    const { fake, storage } = setup(
      nextToMaster("https://d.docs.live.net/d1/Documents/Resume.docx"),
    );
    fake.file(fake.drive, "Documents/Resume.docx", text.encode("master"));
    await storage.writeFile("version", "Resume – data roles.docx", text.encode("v"));
    const empty = await storage.readLedger(FAMILY);
    await storage.appendLedger(FAMILY, empty.version, text.encode("{}\n"));
    expect(read(fake.read(fake.drive, "Documents/Resume – data roles.docx"))).toBe("v");
    expect(read(fake.read(fake.drive, `Documents/Lineage/${FAMILY}/ledger.jsonl`))).toBe("{}\n");
    expect(storage.layout.scopes).toEqual(["Files.ReadWrite"]);
    // The master itself is not a tailored version.
    expect(await storage.fileNames("version")).toEqual(["Resume – data roles.docx", "Resume.docx"]);
  });

  it("refuses a master that isn't in OneDrive for the next-to-master layout", async () => {
    const { storage } = setup(nextToMaster("C:\\Users\\someone\\Resume.docx"));
    await expect(storage.fileNames("version")).rejects.toMatchObject({
      userMessage: expect.stringContaining("saved in your OneDrive") as unknown,
    });
  });

  it("appends with If-Match, and a ledger changed elsewhere is a conflict", async () => {
    const { fake, storage } = setup();
    const empty = await storage.readLedger(FAMILY);
    const v1 = await storage.appendLedger(FAMILY, empty.version, text.encode("one\n"));
    const sessions = fake.requests.filter((r) => r.url.endsWith("/createUploadSession"));
    expect(sessions.at(-1)?.headers["if-match"]).toBeUndefined();
    await storage.appendLedger(FAMILY, v1, text.encode("two\n"));
    expect(
      fake.requests.filter((r) => r.url.endsWith("/createUploadSession")).at(-1)?.headers[
        "if-match"
      ],
    ).toBe(v1);
    // Another device writes the ledger.
    fake.file("me", `Apps/Lineage/${FAMILY}/ledger.jsonl`, text.encode("one\ntwo\nthree\n"));
    await expect(storage.appendLedger(FAMILY, v1, text.encode("x\n"))).rejects.toBeInstanceOf(
      StorageConflict,
    );
    const fresh = await storage.readLedger(FAMILY);
    expect(read(fresh.bytes)).toBe("one\ntwo\nthree\n");
    await storage.appendLedger(FAMILY, fresh.version, text.encode("four\n"));
    expect(read((await storage.readLedger(FAMILY)).bytes)).toBe("one\ntwo\nthree\nfour\n");
  });

  it("refuses to create a ledger another device has just created", async () => {
    const { fake, storage } = setup();
    const empty = await storage.readLedger(FAMILY);
    fake.file("me", `Apps/Lineage/${FAMILY}/ledger.jsonl`, text.encode("theirs\n"));
    await expect(
      storage.appendLedger(FAMILY, empty.version, text.encode("mine\n")),
    ).rejects.toBeInstanceOf(StorageConflict);
    expect(read(fake.read("me", `Apps/Lineage/${FAMILY}/ledger.jsonl`))).toBe("theirs\n");
  });

  it("never sends the bearer token to an upload or download URL", async () => {
    const { fake, storage } = setup();
    const empty = await storage.readLedger(FAMILY);
    await storage.appendLedger(FAMILY, empty.version, text.encode("one\n"));
    await storage.readLedger(FAMILY);
    const outside = fake.requests.filter((r) => !r.url.startsWith("https://graph.microsoft.com/"));
    expect(outside.length).toBeGreaterThan(0);
    expect(outside.every((r) => r.headers["authorization"] === undefined)).toBe(true);
  });

  it("waits and retries when OneDrive is busy, then says so plainly", async () => {
    const { fake, storage } = setup();
    fake.busy = 2;
    expect(await storage.fileNames("version")).toEqual([]);
    fake.busy = 10;
    await expect(storage.fileNames("version")).rejects.toMatchObject({
      userMessage: "OneDrive is busy right now. Try again in a minute.",
    });
  });

  it("asks for a fresh token once after a 401", async () => {
    const fake = new FakeGraph();
    let calls = 0;
    const graph = new Graph({
      token: ({ forceRefresh }) => Promise.resolve(forceRefresh ? "fresh" : (calls++, "")),
      fetch: fake.fetch,
      sleep: () => Promise.resolve(),
    });
    const storage = new OneDriveStorage(graph, appFolder);
    expect(await storage.families()).toEqual([]);
    expect(calls).toBe(1);
  });

  it("gets a new download URL when the old one has expired", async () => {
    const { fake, storage } = setup();
    const blob = text.encode("part");
    const hash = await sha256Hex(blob);
    await storage.putBlob(FAMILY, hash, blob);
    // A second session lists the blobs (with URLs), then the URLs expire.
    const second = new OneDriveStorage(
      new Graph({
        token: () => Promise.resolve("t"),
        fetch: fake.fetch,
        sleep: () => Promise.resolve(),
      }),
      appFolder,
    );
    expect(await second.getBlob(FAMILY, await sha256Hex(text.encode("missing")))).toBeUndefined();
    fake.expireDownloadUrls();
    expect(read(await second.getBlob(FAMILY, hash))).toBe("part");
  });

  it("uploads a blob once, even when asked twice at the same time", async () => {
    const { fake, storage } = setup();
    const blob = text.encode("shared part");
    const hash = await sha256Hex(blob);
    await Promise.all([storage.putBlob(FAMILY, hash, blob), storage.putBlob(FAMILY, hash, blob)]);
    await storage.putBlob(FAMILY, hash, blob);
    const puts = fake.requests.filter((r) => r.method === "PUT" && r.url.includes("/blobs/"));
    expect(puts.length).toBeLessThanOrEqual(2);
    const lists = fake.requests.filter((r) => r.url.includes("/blobs:/children"));
    expect(lists).toHaveLength(1);
  });

  it("encodes names with dashes, spaces and accents", async () => {
    const { fake, storage } = setup();
    await storage.writeFile("version", "Résumé – data & ops #1.docx", text.encode("v"));
    expect(await storage.fileNames("version")).toEqual(["Résumé – data & ops #1.docx"]);
    expect(
      read(fake.read("me", "Apps/Lineage/Tailored versions/Résumé – data & ops #1.docx")),
    ).toBe("v");
    const location = await storage.fileLocation(
      storage.refFor("version", "Résumé – data & ops #1.docx"),
    );
    expect("webUrl" in location && location.webUrl.startsWith("https://")).toBe(true);
  });

  it("says plainly when the network is down", async () => {
    const graph = new Graph({
      token: () => Promise.resolve("t"),
      fetch: () => Promise.reject(new TypeError("Failed to fetch")),
      sleep: () => Promise.resolve(),
    });
    const storage = new OneDriveStorage(graph, appFolder);
    const failure = storage.readLedger(FAMILY);
    await expect(failure).rejects.toBeInstanceOf(StorageError);
    await expect(failure).rejects.toMatchObject({
      userMessage: "Lineage can't reach OneDrive. Check your connection and try again.",
    });
  });
});
