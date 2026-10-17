// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The open Word document, through Office.js. Bytes only: Lineage reads the whole
// file, and replaces the whole file; it never edits XML or ranges itself.

import type { DocumentHost, FileLocation, Ids } from "@lineage/shell";
import { LineageError } from "@lineage/shell";
import { CHECKPOINT, FAMILY, base64, describeOpenFile, idsFrom, readWholeFile } from "./common";

/** Document.insertFileFromBase64 refuses anything larger. */
export const MAX_REPLACE_BYTES = 100 * 1024 * 1024;
/**
 * Testing only: VITE_LINEAGE_LIFT_SIZE_LIMIT=1 in .env.local skips the check above in
 * development builds, so Word's own limit decides. Production builds ignore it.
 */
const LIFT_SIZE_LIMIT =
  import.meta.env.DEV && import.meta.env["VITE_LINEAGE_LIFT_SIZE_LIMIT"] === "1";

export class WordHost implements DocumentHost {
  /** The whole document as a .docx, unsaved edits included. */
  readBytes(): Promise<Uint8Array> {
    return readWholeFile("Word");
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
      return idsFrom(family.value, checkpoint.value);
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

  describe(): Promise<{ name: string; url: string | null }> {
    return Promise.resolve(describeOpenFile("This document"));
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
