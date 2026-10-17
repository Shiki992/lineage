// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// "My documents" (decision 0015): every family in the person's storage, a
// document's history and tree, compare, open a copy exactly as sent, and the
// few changes the page can make (restore as a new version, milestones,
// relinking a moved file). Like `Lineage`, it reads the ledger, runs the engine,
// stores blobs and appends last, once more on a conflict; but it has no open
// document, and it never changes a file that exists.

import type { CommandResult, Engine, FamilyLedger, Observed } from "./engine.js";
import { LineageError } from "./errors.js";
import { History, concat } from "./history.js";
import type * as s from "./schemas.js";
import type { Storage } from "./storage.js";
import { FileExists, NotFound, StorageConflict, checkFileName } from "./storage.js";

export interface LibraryOptions {
  readonly engine: Engine;
  readonly storage: Storage;
  /** Milliseconds since 1970. */
  readonly now?: () => number;
  readonly onTiming?: (name: string, ms: number) => void;
}

/** A document's page and tree, with the ledger they came from. */
export interface OpenedDocument {
  readonly view: s.DocumentView;
  readonly ledger: Uint8Array;
}

/** A tailored version whose file isn't where the history says. */
export interface MissingFile {
  readonly node: string;
  readonly name: string;
  readonly storageRef: string;
}

const utf8 = new TextEncoder();
/** Fetch rounds for a command: one per snapshot it may read, and a bound. */
const PRESS_ROUNDS = 64;

function docx(name: string): string {
  const trimmed = name.trim();
  return /\.docx$/i.test(trimmed) ? trimmed : `${trimmed}.docx`;
}

function numbered(name: string, n: number): string {
  return `${name.replace(/\.docx$/i, "")} (${String(n)}).docx`;
}

export class Library {
  private readonly engine: Engine;
  private readonly storage: Storage;
  private readonly history: History;
  private readonly now: () => number;
  private readonly onTiming: (name: string, ms: number) => void;

  constructor(options: LibraryOptions) {
    this.engine = options.engine;
    this.storage = options.storage;
    this.history = new History(this.engine, this.storage);
    this.now = options.now ?? Date.now;
    this.onTiming = options.onTiming ?? (() => undefined);
  }

  private async timed<T>(name: string, work: () => Promise<T>): Promise<T> {
    const started = performance.now();
    try {
      return await work();
    } finally {
      this.onTiming(name, performance.now() - started);
    }
  }

  /**
   * Every family. `first` is called with the library as the index has it, as
   * soon as it is read, so the list can draw before anything is checked; the
   * promise gives the library checked against every ledger. A wrong or missing
   * index is rewritten.
   */
  list(first?: (library: s.Library) => void): Promise<s.Library> {
    return this.timed("library", async () => {
      const index = await this.storage.readIndex();
      if (index && first) {
        const quick = this.engine.libraryFromIndex(index.bytes);
        if (quick) {
          first(quick);
        }
      }
      const families = await this.storage.families();
      const versions = await this.storage.ledgerVersions(families);
      let observed: Observed[] = families.map((family) => ({
        family,
        version: versions.get(family) ?? "",
      }));
      const stale = this.engine.staleFamilies(index?.bytes, observed);
      const ledgers: FamilyLedger[] = [];
      for (const family of stale) {
        const read = await this.storage.readLedger(family);
        ledgers.push({ family, ledger: read.bytes });
        // The ledger may have moved on since its version was asked for.
        observed = observed.map((o) =>
          o.family === family ? { family, version: read.version } : o,
        );
      }
      const refreshed = this.engine.refreshLibrary(index?.bytes, observed, ledgers);
      if (refreshed.index) {
        try {
          await this.storage.writeIndex(refreshed.index, index?.version);
        } catch (error) {
          // Another window rewrote it first; the next visit checks again.
          if (!(error instanceof StorageConflict)) {
            throw error;
          }
        }
      }
      return refreshed.library;
    });
  }

  /** A document's page and family tree. */
  open(family: string): Promise<OpenedDocument> {
    return this.timed("document", async () => {
      const { bytes } = await this.storage.readLedger(family);
      if (bytes.length === 0) {
        throw new NotFound(`family ${family}`);
      }
      return { view: this.engine.document(bytes), ledger: bytes };
    });
  }

  /** "Compare": what changed from checkpoint `a` to checkpoint `b`. */
  compare(family: string, a: string, b: string): Promise<s.WhatChanged> {
    return this.timed("compare", async () => {
      const { bytes } = await this.storage.readLedger(family);
      return this.history.withBlobs(family, () => this.engine.compare(bytes, a, b));
    });
  }

  /** "Open as sent": the exact bytes of a sent copy, checked against their hash. */
  sentCopy(family: string, sent: string): Promise<Uint8Array> {
    return this.timed("sentCopy", async () => {
      const { bytes } = await this.storage.readLedger(family);
      return this.history.withBlobs(family, () => this.engine.openSentCopy(bytes, sent));
    });
  }

  /** Tailored versions whose file isn't in storage any more. */
  missingFiles(page: s.DocumentPage): Promise<MissingFile[]> {
    return this.timed("missingFiles", async () => {
      const missing: MissingFile[] = [];
      for (const file of page.files) {
        if (file.role !== "variant" || file.archived) {
          continue;
        }
        try {
          await this.storage.fileLocation(file.storageRef);
        } catch (error) {
          if (!(error instanceof NotFound)) {
            throw error;
          }
          missing.push({ node: file.node, name: file.name, storageRef: file.storageRef });
        }
      }
      return missing;
    });
  }

  /** The files in Lineage's folder of tailored versions no file of `page` points at. */
  async unlinkedFiles(page: s.DocumentPage): Promise<string[]> {
    const linked = new Set(page.files.map((f) => f.storageRef));
    return (await this.storage.fileNames("version")).filter(
      (name) => !linked.has(this.storage.refFor("version", name)),
    );
  }

  /** Runs a command on the ledger, stores blobs and appends; once more on a conflict. */
  private async press(
    family: string,
    run: (ledger: Uint8Array) => CommandResult,
  ): Promise<CommandResult> {
    for (let attempt = 0; attempt < 2; attempt++) {
      const { bytes: ledger, version } = await this.storage.readLedger(family);
      // As the task pane does on open: every manifest and change list first, so
      // a command that walks the family's history needs one round per snapshot
      // at most, not one per record.
      await this.history.fetchBlobs(family, this.engine.referencedBlobs(ledger));
      // Restoring compares the new version with every master checkpoint after
      // the one it follows, so allow a round for each of those snapshots.
      const result = await this.history.withBlobs(family, () => run(ledger), PRESS_ROUNDS);
      if (result.outcome.records === 0) {
        return result;
      }
      await this.history.storeBlobs(family, result);
      try {
        const lines = utf8.encode(result.outcome.ledgerLines);
        const written = await this.storage.appendLedger(family, version, lines);
        await this.history.updateIndex(family, written, concat(ledger, lines));
        return result;
      } catch (error) {
        if (error instanceof StorageConflict && attempt === 0) {
          continue;
        }
        throw error;
      }
    }
    throw new StorageConflict("the ledger kept changing");
  }

  /** A file name for a new tailored version: `name`, or "name (2)" if it's taken. */
  async freeName(name: string): Promise<string> {
    const wanted = checkFileName(docx(name));
    const taken = new Set((await this.storage.fileNames("version")).map((n) => n.toLowerCase()));
    if (!taken.has(wanted.toLowerCase())) {
      return wanted;
    }
    for (let n = 2; n < 100; n++) {
      const next = numbered(wanted, n);
      if (!taken.has(next.toLowerCase())) {
        return next;
      }
    }
    throw new FileExists(wanted);
  }

  /**
   * "Restore as a new version": a new tailored version holding exactly what
   * `checkpoint` held, saved as a new file. Nothing that exists is changed.
   */
  restoreAsNewVersion(
    family: string,
    checkpoint: string,
    name: string,
  ): Promise<{ name: string; ref: string; nodeId: string }> {
    return this.timed("restoreAsNewVersion", async () => {
      const clean = checkFileName(docx(name));
      if (
        (await this.storage.fileNames("version")).some(
          (n) => n.toLowerCase() === clean.toLowerCase(),
        )
      ) {
        throw new FileExists(clean);
      }
      const ref = this.storage.refFor("version", clean);
      const result = await this.press(family, (ledger) =>
        this.engine.restoreAsNewVersion(ledger, checkpoint, clean, ref, this.now()),
      );
      const nodeId = result.outcome.created;
      if (!result.newFile || !nodeId) {
        throw new LineageError("Lineage couldn't restore that version.", "no file came back");
      }
      // Written after the append: it can always be made again from the history.
      await this.storage.writeFile("version", clean, result.newFile);
      return { name: clean, ref, nodeId };
    });
  }

  /** "Mark as milestone"; null takes the mark off. */
  async markMilestone(family: string, checkpoint: string, label: string | null): Promise<void> {
    await this.timed("markMilestone", () =>
      this.press(family, (ledger) =>
        this.engine.markMilestone(ledger, checkpoint, label, this.now()),
      ),
    );
  }

  /**
   * "Choose the file": the version's file is `name` in Lineage's folder now.
   * Only a file whose own Lineage ids name that version is accepted.
   */
  async relink(family: string, node: string, name: string): Promise<void> {
    await this.timed("relink", async () => {
      const ref = this.storage.refFor("version", checkFileName(name));
      const bytes = await this.storage.readFile(ref);
      await this.press(family, (ledger) =>
        this.engine.relink(ledger, node, ref, bytes, this.now()),
      );
    });
  }
}
