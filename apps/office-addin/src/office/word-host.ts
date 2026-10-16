// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The open Word document, through Office.js. Bytes only: Lineage reads the whole
// file, and replaces the whole file; it never edits XML or ranges itself.

import type { DocumentHost, FileLocation, Ids } from "@lineage/shell";
import { LineageError } from "@lineage/shell";

const FAMILY = "LineageFamilyId";
const CHECKPOINT = "LineageCheckpointId";
const ID = /^[0-9a-f]{32}$/;
/** getFileAsync slices: at most 4 MB on Windows, Mac and the web. */
const SLICE = 4 * 1024 * 1024;
/** Document.insertFileFromBase64 refuses anything larger. */
export const MAX_REPLACE_BYTES = 100 * 1024 * 1024;
/**
 * Testing only: VITE_LINEAGE_LIFT_SIZE_LIMIT=1 in .env.local skips the check above in
 * development builds, so Word's own limit decides. Production builds ignore it.
 */
const LIFT_SIZE_LIMIT =
  import.meta.env.DEV && import.meta.env["VITE_LINEAGE_LIFT_SIZE_LIMIT"] === "1";

function base64(bytes: Uint8Array): string {
  let binary = "";
  const step = 0x8000;
  for (let i = 0; i < bytes.length; i += step) {
    binary += String.fromCharCode(...bytes.subarray(i, i + step));
  }
  return btoa(binary);
}

function asAsync<T>(start: (done: (result: Office.AsyncResult<T>) => void) => void): Promise<T> {
  return new Promise((resolve, reject) => {
    start((result) => {
      if (result.status === Office.AsyncResultStatus.Succeeded) {
        resolve(result.value);
      } else {
        reject(
          new LineageError(
            "Word couldn't give Lineage this file just now. Try again.",
            `${result.error.name}: ${result.error.message}`,
          ),
        );
      }
    });
  });
}

/** The file name at the end of a URL or path. */
export function fileNameOf(url: string): string {
  const last = url.split(/[\\/]/).pop() ?? "";
  const clean = last.split(/[?#]/)[0] ?? "";
  try {
    return decodeURIComponent(clean) || "This document";
  } catch {
    return clean || "This document";
  }
}

export class WordHost implements DocumentHost {
  /**
   * The whole document as a .docx, unsaved edits included: Office makes a
   * temporary copy for getFileAsync. The file must be closed after reading, or
   * the next call fails.
   * https://learn.microsoft.com/office/dev/add-ins/develop/get-the-whole-document-from-an-add-in-for-powerpoint-or-word
   */
  async readBytes(): Promise<Uint8Array> {
    const file = await asAsync<Office.File>((done) => {
      Office.context.document.getFileAsync(Office.FileType.Compressed, { sliceSize: SLICE }, done);
    });
    try {
      const parts: Uint8Array[] = [];
      for (let i = 0; i < file.sliceCount; i++) {
        const slice = await asAsync<Office.Slice>((done) => {
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

  /**
   * The two ids, from custom document properties (WordApi 1.3). The engine
   * reads the same properties from the bytes (docProps/custom.xml).
   * https://learn.microsoft.com/javascript/api/word/word.custompropertycollection
   */
  async readIds(): Promise<Ids | null> {
    return Word.run(async (context) => {
      const props = context.document.properties.customProperties;
      const family = props.getItemOrNullObject(FAMILY);
      const checkpoint = props.getItemOrNullObject(CHECKPOINT);
      family.load("value");
      checkpoint.load("value");
      await context.sync();
      if (family.isNullObject || checkpoint.isNullObject) {
        return null;
      }
      const f: unknown = family.value;
      const c: unknown = checkpoint.value;
      return typeof f === "string" && typeof c === "string" && ID.test(f) && ID.test(c)
        ? { family: f, checkpoint: c }
        : null;
    });
  }

  async writeIds(ids: Ids | null): Promise<void> {
    await Word.run(async (context) => {
      const props = context.document.properties.customProperties;
      if (ids) {
        props.add(FAMILY, ids.family);
        props.add(CHECKPOINT, ids.checkpoint);
      } else {
        props.getItemOrNullObject(FAMILY).delete();
        props.getItemOrNullObject(CHECKPOINT).delete();
      }
      await context.sync();
    });
  }

  /**
   * Replaces the whole document with `bytes`, tracked changes included
   * (decision 0008). Track Changes is turned off for the insert, or Word would
   * record the whole file as the person's own insertion, then put back as it
   * was. Settings are not imported; the open document keeps its own.
   * https://learn.microsoft.com/javascript/api/word/word.document#word-word-document-insertfilefrombase64-member(1)
   * https://learn.microsoft.com/javascript/api/word/word.insertfileoptions
   */
  async replaceContents(bytes: Uint8Array): Promise<void> {
    if (bytes.length > MAX_REPLACE_BYTES && !LIFT_SIZE_LIMIT) {
      throw new LineageError(
        "This file is over 100 MB, which is too large for Lineage to change while it's open in Word.",
        `${String(bytes.length)} bytes`,
      );
    }
    const supports = (version: string) =>
      Office.context.requirements.isSetSupported("WordApi", version);
    const options: Word.InsertFileOptions = {
      importTheme: true,
      importStyles: true,
      importParagraphSpacing: true,
      importPageColor: true,
      importChangeTrackingMode: false,
      ...(supports("1.6") ? { importCustomProperties: true, importCustomXmlParts: true } : {}),
      ...(supports("1.7") ? { importDifferentOddEvenPages: true } : {}),
    };
    const encoded = base64(bytes);
    await Word.run(async (context) => {
      const document = context.document;
      document.load("changeTrackingMode");
      await context.sync();
      const mode = document.changeTrackingMode;
      if (mode !== Word.ChangeTrackingMode.off) {
        document.changeTrackingMode = Word.ChangeTrackingMode.off;
        await context.sync();
      }
      try {
        document.insertFileFromBase64(encoded, "Replace", options);
        await context.sync();
      } finally {
        if (mode !== Word.ChangeTrackingMode.off) {
          document.changeTrackingMode = mode;
          await context.sync();
        }
      }
    });
  }

  async save(): Promise<void> {
    await Word.run(async (context) => {
      context.document.save();
      await context.sync();
    });
  }

  /**
   * The file's name and where it lives. `url` is a OneDrive or SharePoint
   * address, a local path on desktop, or empty for a document never saved.
   * https://learn.microsoft.com/javascript/api/office/office.document#office-office-document-url-member
   */
  describe(): Promise<{ name: string; url: string | null }> {
    const url = Office.context.document.url;
    if (!url) {
      return Promise.resolve({ name: "This document", url: null });
    }
    return Promise.resolve({ name: fileNameOf(url), url });
  }

  /**
   * Opens a stored file in a new window (desktop) or tab (web). Word on the web
   * opens only cloud addresses (WordApi 1.6).
   * https://learn.microsoft.com/javascript/api/word/word.application#word-word-application-opendocument-member(1)
   */
  async openFile(location: FileLocation): Promise<void> {
    const target = "webUrl" in location ? location.webUrl : location.path;
    if (Office.context.requirements.isSetSupported("WordApi", "1.6")) {
      await Word.run(async (context) => {
        context.application.openDocument(target);
        await context.sync();
      });
    } else if ("webUrl" in location) {
      Office.context.ui.openBrowserWindow(location.webUrl);
    } else {
      throw new LineageError(
        "Open that file from the folder it's in.",
        "openDocument is not supported",
      );
    }
  }

  /**
   * Opens bytes as a new, unsaved document: "open exactly what they received".
   * https://learn.microsoft.com/javascript/api/word/word.application#word-word-application-createdocument-member(1)
   */
  async openCopy(bytes: Uint8Array): Promise<void> {
    const encoded = base64(bytes);
    await Word.run(async (context) => {
      context.application.createDocument(encoded).open();
      await context.sync();
    });
  }
}
