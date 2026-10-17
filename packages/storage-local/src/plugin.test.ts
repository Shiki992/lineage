// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The local folder behaves like every other storage: the conformance suite,
// against the real dev-server handler on a temporary folder.

import fs from "node:fs";
import { createServer } from "node:http";
import type { AddressInfo } from "node:net";
import os from "node:os";
import path from "node:path";
import { storageConformance } from "@lineage/shell/testing";
import { FileLocked } from "@lineage/shell";
import { afterAll, describe, expect, it, vi } from "vitest";
import { LocalFolderStorage } from "./client.js";
import { ROUTE, handle } from "./plugin.js";

const roots: string[] = [];
const servers: { close: () => void }[] = [];

async function serve(): Promise<{ storage: LocalFolderStorage; root: string }> {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "lineage-local-"));
  roots.push(root);
  const server = createServer((req, res) => {
    req.url = (req.url ?? "/").slice(ROUTE.length);
    void handle(root, req, res);
  });
  servers.push(server);
  await new Promise<void>((resolve) => {
    server.listen(0, "127.0.0.1", resolve);
  });
  const { port } = server.address() as AddressInfo;
  return { storage: new LocalFolderStorage(`http://127.0.0.1:${String(port)}${ROUTE}`), root };
}

afterAll(() => {
  for (const s of servers) {
    s.close();
  }
  for (const r of roots) {
    fs.rmSync(r, { recursive: true, force: true });
  }
});

storageConformance("LocalFolderStorage", async () => (await serve()).storage);

describe("LocalFolderStorage on disk", () => {
  it("keeps the index where the harness looks for it", async () => {
    const { storage, root } = await serve();
    await storage.writeIndex(new TextEncoder().encode("{}"), undefined);
    expect(fs.readFileSync(path.join(root, "Lineage", "index.json"), "utf8")).toBe("{}");
  });

  it("says when a file is open and can't be replaced, and leaves nothing behind", async () => {
    // PowerPoint for Windows keeps a deck it has open from being replaced.
    const { storage, root } = await serve();
    const stored = await storage.writeFile("version", "Deck.pptx", new Uint8Array([1, 2, 3]));
    const read = await storage.readFileVersioned(stored.ref);
    const rename = vi.spyOn(fs, "renameSync").mockImplementation(() => {
      throw Object.assign(new Error("busy"), { code: "EBUSY" });
    });
    try {
      await expect(
        storage.replaceFile(stored.ref, new Uint8Array([4, 5, 6]), read.version),
      ).rejects.toBeInstanceOf(FileLocked);
    } finally {
      rename.mockRestore();
    }
    const folder = path.dirname(path.join(root, stored.ref));
    expect(fs.readdirSync(folder)).toEqual(["Deck.pptx"]);
    expect((await storage.readFileVersioned(stored.ref)).bytes).toEqual(new Uint8Array([1, 2, 3]));
  });
});
