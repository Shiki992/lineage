// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import type { Library, MissingFile, OpenedDocument, schemas, WhatChanged } from "@lineage/shell";

/** What the screens ask of the person's history: `Library` from the shell. */
export type Documents = Pick<
  Library,
  | "list"
  | "open"
  | "compare"
  | "sentCopy"
  | "missingFiles"
  | "sharedEdits"
  | "unlinkedFiles"
  | "freeName"
  | "restoreAsNewVersion"
  | "markMilestone"
  | "relink"
>;

export type { MissingFile, OpenedDocument, WhatChanged };
export type LibraryView = schemas.Library;

/** The media type of each format Lineage opens, by extension. */
const TYPES: Readonly<Record<string, string>> = {
  docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  pptx: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
};

/** Saves bytes as a file the browser downloads, exactly as they are. */
export function saveFile(bytes: Uint8Array, name: string): void {
  const extension = /\.([a-z]+)$/i.exec(name)?.[1]?.toLowerCase() ?? "";
  const url = URL.createObjectURL(
    new Blob([bytes as Uint8Array<ArrayBuffer>], {
      type: TYPES[extension] ?? "application/octet-stream",
    }),
  );
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => {
    URL.revokeObjectURL(url);
  }, 0);
}

/**
 * "Resume – data roles · v2" sent to Northgate → "Resume – data roles v2 (Northgate).docx";
 * a deck's copy ends in .pptx.
 */
export function sentFileName(version: string, recipient: string, format = "docx"): string {
  const clean = (s: string) =>
    s
      .replace(/["*:<>?/\\|]/g, " ")
      .replace(/\s+/g, " ")
      .trim();
  return `${clean(version.replace(" · ", " "))} (${clean(recipient)}).${format}`;
}
