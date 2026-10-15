// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";
import { MemoryStorage, checkFileName, sha256Hex } from "./storage.js";
import { storageConformance } from "./testing/storage-conformance.js";

storageConformance("MemoryStorage", () => new MemoryStorage());

describe("storage helpers", () => {
  it("hash like the engine", async () => {
    // SHA-256 of "abc", from FIPS 180-2.
    expect(await sha256Hex(new TextEncoder().encode("abc"))).toBe(
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
  });

  it.each([
    "",
    "  ",
    "a/b.docx",
    "a\\b.docx",
    "what?.docx",
    "name.",
    "..",
    "x".repeat(201),
    "tab\tname",
  ])("refuse the file name %j", (name) => {
    expect(() => checkFileName(name)).toThrow();
  });

  it.each(["Resume – data roles.docx", "Résumé (Northgate).docx", "  Trimmed.docx  "])(
    "accept the file name %j",
    (name) => {
      expect(checkFileName(name)).toBe(name.trim());
    },
  );
});
