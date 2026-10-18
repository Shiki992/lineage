// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// File names for the files Lineage makes: tailored versions, restored versions
// and sent copies keep the extension of their family's format.

const OFFICE = /\.(docx|pptx|xlsx|xlsm)$/i;

/** ".docx", ".pptx" or ".xlsx", for a family format ("docx", "pptx", "xlsx"). */
export function extensionFor(format: string): string {
  switch (format) {
    case "pptx":
      return ".pptx";
    case "xlsx":
      return ".xlsx";
    default:
      return ".docx";
  }
}

/**
 * `name` with an extension, unless it already ends in one: the extension of
 * `like` (the file it is made from, so a workbook with macros stays .xlsm),
 * or else the format's.
 */
export function withExtension(name: string, format = "docx", like?: string): string {
  const trimmed = name.trim();
  if (OFFICE.test(trimmed)) {
    return trimmed;
  }
  const own = like === undefined ? null : OFFICE.exec(like.trim());
  return `${trimmed}${own ? own[0].toLowerCase() : extensionFor(format)}`;
}

/** "Name.pptx" → "Name (2).pptx", for a file name that is taken. */
export function numbered(name: string, n: number): string {
  const match = OFFICE.exec(name);
  const extension = match ? match[0] : "";
  const stem = match ? name.slice(0, -extension.length) : name;
  return `${stem} (${String(n)})${extension}`;
}
