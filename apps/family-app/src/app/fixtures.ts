// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Test helpers: the golden replies (written by crates/wasm/tests/golden.rs)
// as the screens' data, checked against the shell's schemas, and a fake
// `Documents` whose answers each test can change.

import { readFileSync } from "node:fs";
import path from "node:path";
import type { DocumentView, WhatChanged } from "@lineage/shell";
import { schemas } from "@lineage/shell";
import * as v from "valibot";
import { vi } from "vitest";
import type { Documents, LibraryView } from "./documents";

function golden(name: string): unknown {
  // Tests run from apps/family-app (jsdom's import.meta.url isn't a file URL).
  const text = readFileSync(
    path.resolve(process.cwd(), "../../packages/shell/test/golden", `${name}.json`),
    "utf8",
  );
  return (JSON.parse(text) as { value: unknown }).value;
}

export const documentView = (): DocumentView => v.parse(schemas.DocumentView, golden("document"));
export const damagedView = (): DocumentView =>
  v.parse(schemas.DocumentView, golden("document-damaged"));
export const compared = (): WhatChanged => v.parse(schemas.WhatChanged, golden("compare"));

/** The golden library, plus a cover letter and a deck. */
export function libraryView(): LibraryView {
  const one = v.parse(schemas.Refreshed, golden("refresh-library")).library;
  const resume = one.entries[0];
  if (!resume) {
    throw new Error("no entry");
  }
  return {
    empty: null,
    entries: [
      resume,
      {
        ...resume,
        familyId: "c".repeat(32),
        name: "Cover letter",
        masterFile: "Cover letter.docx",
        summary: "No tailored versions · 2 checkpoints · 1 sent",
        attention: null,
        lastAt: resume.lastAt + 60_000,
      },
      {
        ...resume,
        familyId: "d".repeat(32),
        name: "Pitch deck",
        format: "pptx",
        kind: "PowerPoint deck",
        masterFile: "Pitch deck.pptx",
        canOpen: false,
        attention: null,
        lastAt: resume.lastAt - 86_400_000,
      },
    ],
  };
}

export function fakeDocuments(overrides: Partial<Documents> = {}): Documents {
  const view = documentView();
  return {
    list: vi.fn((first?: (l: LibraryView) => void) => {
      const l = libraryView();
      first?.(l);
      return Promise.resolve(l);
    }),
    open: vi.fn(() => Promise.resolve({ view, ledger: new Uint8Array() })),
    compare: vi.fn(() => Promise.resolve(compared())),
    sentCopy: vi.fn(() => Promise.resolve(new Uint8Array([80, 75, 3, 4]))),
    missingFiles: vi.fn(() => Promise.resolve([])),
    unlinkedFiles: vi.fn(() => Promise.resolve([])),
    freeName: vi.fn((name: string) => Promise.resolve(`${name}.docx`)),
    restoreAsNewVersion: vi.fn((_f: string, _c: string, name: string) =>
      Promise.resolve({
        name: `${name}.docx`,
        ref: `Tailored versions/${name}.docx`,
        nodeId: "e".repeat(32),
      }),
    ),
    markMilestone: vi.fn(() => Promise.resolve()),
    relink: vi.fn(() => Promise.resolve()),
    ...overrides,
  };
}
