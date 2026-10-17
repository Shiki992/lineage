// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// What Word and PowerPoint share through Office.js: reading the whole open
// file, and the file's name and address.

import { LineageError } from "@lineage/shell";

/** getFileAsync slices: at most 4 MB on Windows, Mac and the web. */
const SLICE = 4 * 1024 * 1024;

export const FAMILY = "LineageFamilyId";
export const CHECKPOINT = "LineageCheckpointId";
export const ID = /^[0-9a-f]{32}$/;

export function base64(bytes: Uint8Array): string {
  let binary = "";
  const step = 0x8000;
  for (let i = 0; i < bytes.length; i += step) {
    binary += String.fromCharCode(...bytes.subarray(i, i + step));
  }
  return btoa(binary);
}

export function asAsync<T>(
  app: string,
  start: (done: (result: Office.AsyncResult<T>) => void) => void,
): Promise<T> {
  return new Promise((resolve, reject) => {
    start((result) => {
      if (result.status === Office.AsyncResultStatus.Succeeded) {
        resolve(result.value);
      } else {
        reject(
          new LineageError(
            `${app} couldn't give Lineage this file just now. Try again.`,
            `${result.error.name}: ${result.error.message}`,
          ),
        );
      }
    });
  });
}

/**
 * The whole open file, unsaved edits included: Office makes a temporary copy
 * for getFileAsync. The file must be closed after reading, or the next call
 * fails. Works the same in Word and PowerPoint, on Windows, Mac and the web.
 * https://learn.microsoft.com/office/dev/add-ins/develop/get-the-whole-document-from-an-add-in-for-powerpoint-or-word
 */
export async function readWholeFile(app: string): Promise<Uint8Array> {
  const file = await asAsync<Office.File>(app, (done) => {
    Office.context.document.getFileAsync(Office.FileType.Compressed, { sliceSize: SLICE }, done);
  });
  try {
    const parts: Uint8Array[] = [];
    for (let i = 0; i < file.sliceCount; i++) {
      const slice = await asAsync<Office.Slice>(app, (done) => {
        file.getSliceAsync(i, done);
      });
      parts.push(new Uint8Array(slice.data as ArrayLike<number>));
    }
    const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
    let at = 0;
    for (const part of parts) {
      out.set(part, at);
      at += part.length;
    }
    return out;
  } finally {
    await new Promise<void>((resolve) => {
      file.closeAsync(() => {
        resolve();
      });
    });
  }
}

/** The file name at the end of a URL or path. */
export function fileNameOf(url: string, fallback: string): string {
  const last = url.split(/[\\/]/).pop() ?? "";
  const clean = last.split(/[?#]/)[0] ?? "";
  try {
    return decodeURIComponent(clean) || fallback;
  } catch {
    return clean || fallback;
  }
}

/**
 * The file's name and where it lives: a OneDrive or SharePoint address, a
 * local path on desktop, or nothing for a file never saved.
 * https://learn.microsoft.com/javascript/api/office/office.document#office-office-document-url-member
 */
export function describeOpenFile(fallback: string): { name: string; url: string | null } {
  const url = Office.context.document.url;
  if (!url) {
    return { name: fallback, url: null };
  }
  return { name: fileNameOf(url, fallback), url };
}

/** Checks the two id values read from custom properties. */
export function idsFrom(
  family: unknown,
  checkpoint: unknown,
): { family: string; checkpoint: string } | null {
  return typeof family === "string" &&
    typeof checkpoint === "string" &&
    ID.test(family) &&
    ID.test(checkpoint)
    ? { family, checkpoint }
    : null;
}
