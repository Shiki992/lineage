// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// File names for the files Lineage makes: tailored versions, restored versions
// and sent copies keep the extension of their family's format.

const OFFICE = /\.(docx|pptx)$/i;

/** ".docx" or ".pptx", for a family format ("docx", "pptx"). */
export function extensionFor(format: string): string {
  return format === "pptx" ? ".pptx" : ".docx";
}

/** `name` with the format's extension, unless it already ends in one. */
export function withExtension(name: string, format = "docx"): string {
  const trimmed = name.trim();
  return OFFICE.test(trimmed) ? trimmed : `${trimmed}${extensionFor(format)}`;
}

/** "Name.pptx" → "Name (2).pptx", for a file name that is taken. */
export function numbered(name: string, n: number): string {
  const match = OFFICE.exec(name);
  const extension = match ? match[0] : "";
  const stem = match ? name.slice(0, -extension.length) : name;
  return `${stem} (${String(n)})${extension}`;
}
