// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The open document, as the platform exposes it: Office.js in Word
// (apps/office-addin/src/office/word-host.ts), a fake in tests, and later the
// Google Docs API. Bytes only: the host never looks inside them.

import type { Ids } from "./schemas.js";
import type { FileLocation } from "./storage.js";

export interface DocumentHost {
  /** The open document as a .docx, unsaved edits included. */
  readBytes(): Promise<Uint8Array>;
  /** Lineage's ids from the document's custom properties, or null. */
  readIds(): Promise<Ids | null>;
  /** Sets the ids, or removes them when `ids` is null. Other properties stay. */
  writeIds(ids: Ids | null): Promise<void>;
  /**
   * Makes the open document hold exactly `bytes`. Returns once the host holds
   * them; the caller reads them back to check (decision 0008).
   */
  replaceContents(bytes: Uint8Array): Promise<void>;
  save(): Promise<void>;
  /** The open document's file name, and where it is saved (null if never saved). */
  describe(): Promise<{ name: string; url: string | null }>;
  /** Opens a stored file in a new window or tab. */
  openFile(location: FileLocation): Promise<void>;
  /** Opens bytes as a new, unsaved document, such as "what they received". */
  openCopy(bytes: Uint8Array): Promise<void>;
}
