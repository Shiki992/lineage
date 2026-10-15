// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The behaviour every Storage adapter must have, as one Vitest suite each adapter
// runs on itself: memory here, OneDrive against a fake Graph, the development
// folder through its server. It checks what the harness's Storage trait promises
// (crates/harness/src/storage.rs).

import { describe, expect, it } from "vitest";
import type { Storage } from "../storage.js";
import { BadBlob, FileExists, NotFound, StorageConflict, sha256Hex } from "../storage.js";

const FAMILY = "0123456789abcdef0123456789abcdef";
const OTHER = "fedcba9876543210fedcba9876543210";
const text = new TextEncoder();

export function storageConformance(name: string, make: () => Promise<Storage> | Storage): void {
  describe(`${name} behaves like the harness storage`, () => {
    it("starts with an empty ledger, appends at the version it read, and refuses a stale one", async () => {
      const storage = await make();
      const empty = await storage.readLedger(FAMILY);
      expect(empty.bytes.length).toBe(0);
      const v1 = await storage.appendLedger(FAMILY, empty.version, text.encode("one\n"));
      const read = await storage.readLedger(FAMILY);
      expect(new TextDecoder().decode(read.bytes)).toBe("one\n");
      expect(read.version).toBe(v1);
      await expect(
        storage.appendLedger(FAMILY, empty.version, text.encode("two\n")),
      ).rejects.toBeInstanceOf(StorageConflict);
      const v2 = await storage.appendLedger(FAMILY, v1, text.encode("two\n"));
      expect(v2).not.toBe(v1);
      expect(new TextDecoder().decode((await storage.readLedger(FAMILY)).bytes)).toBe("one\ntwo\n");
      expect(await storage.families()).toContain(FAMILY);
      expect((await storage.readLedger(OTHER)).bytes.length).toBe(0);
    });

    it("stores blobs by their hash, once, and refuses bytes that don't match", async () => {
      const storage = await make();
      const bytes = text.encode("a part of a document");
      const hash = await sha256Hex(bytes);
      expect(await storage.getBlob(FAMILY, hash)).toBeUndefined();
      await storage.putBlob(FAMILY, hash, bytes);
      await storage.putBlob(FAMILY, hash, bytes);
      expect(new TextDecoder().decode(await storage.getBlob(FAMILY, hash))).toBe(
        "a part of a document",
      );
      expect(await storage.getBlob(OTHER, hash)).toBeUndefined();
      await expect(
        storage.putBlob(FAMILY, hash, text.encode("something else")),
      ).rejects.toBeInstanceOf(BadBlob);
    });

    it("refuses ids and hashes that could name other folders", async () => {
      const storage = await make();
      await expect(storage.readLedger("../escape")).rejects.toThrow();
      await expect(storage.getBlob(FAMILY, "../../ledger.jsonl")).rejects.toThrow();
    });

    it("writes files by kind, refuses to overwrite unless asked, and finds them again", async () => {
      const storage = await make();
      const name = "Resume – data roles.docx";
      const saved = await storage.writeFile("version", name, text.encode("v1"));
      expect(saved.ref).toBe(storage.refFor("version", name));
      expect(saved.name).toBe(name);
      expect(new TextDecoder().decode(await storage.readFile(saved.ref))).toBe("v1");
      await expect(storage.writeFile("version", name, text.encode("v2"))).rejects.toBeInstanceOf(
        FileExists,
      );
      await storage.writeFile("version", name, text.encode("v2"), true);
      expect(new TextDecoder().decode(await storage.readFile(saved.ref))).toBe("v2");
      expect(await storage.fileNames("version")).toEqual([name]);
      expect(await storage.fileNames("sent")).toEqual([]);
      await storage.writeFile("sent", "Resume (Northgate).docx", text.encode("copy"));
      expect(await storage.fileNames("sent")).toEqual(["Resume (Northgate).docx"]);
      expect(await storage.fileLocation(saved.ref)).toBeTruthy();
      await expect(
        storage.readFile(storage.refFor("version", "missing.docx")),
      ).rejects.toBeInstanceOf(NotFound);
      await expect(storage.writeFile("version", "a/b.docx", text.encode("x"))).rejects.toThrow();
    });
  });
}
