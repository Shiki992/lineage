// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// A copy opened from My documents downloads under the family's format: a deck
// as a .pptx with PowerPoint's media type, a resume as a .docx.

import { afterEach, describe, expect, it, vi } from "vitest";
import { saveFile, sentFileName } from "./documents";

afterEach(() => {
  vi.restoreAllMocks();
});

describe("files opened from My documents", () => {
  it("names a sent copy after its version, its recipient and its format", () => {
    expect(sentFileName("Resume – data roles · v2", "Northgate")).toBe(
      "Resume – data roles v2 (Northgate).docx",
    );
    expect(sentFileName("Pitch deck – Pellow · v1", "Pellow Ventures", "pptx")).toBe(
      "Pitch deck – Pellow v1 (Pellow Ventures).pptx",
    );
  });

  it("downloads a deck as a PowerPoint file", () => {
    const blobs: Blob[] = [];
    vi.spyOn(URL, "createObjectURL").mockImplementation((b) => {
      blobs.push(b as Blob);
      return "blob:deck";
    });
    vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => undefined);
    vi.spyOn(HTMLAnchorElement.prototype, "click").mockImplementation(() => undefined);
    saveFile(new Uint8Array([1, 2, 3]), "Pitch deck v1 (Pellow Ventures).pptx");
    saveFile(new Uint8Array([1, 2, 3]), "Resume v2 (Northgate).docx");
    expect(blobs.map((b) => b.type)).toEqual([
      "application/vnd.openxmlformats-officedocument.presentationml.presentation",
      "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ]);
  });
});
