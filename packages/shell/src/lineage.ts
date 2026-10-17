// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// One method per panel button: the TypeScript twin of the harness's `Panel`
// (crates/harness/src/lib.rs). Each command reads the ledger, runs the engine,
// stores the blobs it made, changes the open document if it must, and appends to
// the ledger last. If the append finds the ledger moved on (another window or
// device), the document change is undone and the command runs once more on the
// new ledger. Files that can be made again from history (tailored versions, sent
// copies) are written after the append.

import type { CommandResult, Engine, FamilyLedger, UpdateResult } from "./engine.js";
import type { DocumentHost } from "./host.js";
import type * as s from "./schemas.js";
import type { Storage } from "./storage.js";
import type { FileLocation } from "./storage.js";
import { LineageError } from "./errors.js";
import { History, concat } from "./history.js";
import { numbered, withExtension } from "./names.js";
import { FileExists, StorageConflict, checkFileName } from "./storage.js";

export { LineageError, userMessage } from "./errors.js";

/** A file that came back, held while the person confirms which copy it is. */
export interface Returning {
  readonly bytes: Uint8Array;
  /** Its file name. */
  readonly name: string;
  readonly recognition: s.Recognition;
}

/** What the panel shows for the open document. */
export type Opened =
  | { readonly kind: "untracked"; readonly name: string; readonly saved: boolean }
  | { readonly kind: "elsewhere"; readonly ids: s.Ids }
  | { readonly kind: "master"; readonly file: s.OpenFile; readonly panel: s.MasterPanel }
  | { readonly kind: "version"; readonly file: s.OpenFile; readonly panel: s.VersionPanel }
  /**
   * The open document is a copy that came back (its ids name a sent copy), or
   * an older copy of a file (its checkpoint isn't the file's latest). It is
   * never shown as the file itself, so it can't be saved as one of its
   * checkpoints by mistake (decision 0012).
   */
  | { readonly kind: "cameBack"; readonly file: s.OpenFile; readonly returning: Returning };

export interface LineageOptions {
  readonly engine: Engine;
  readonly storage: Storage;
  readonly host: DocumentHost;
  /** Milliseconds since 1970. */
  readonly now?: () => number;
  /** Timings for the start-up and command budgets (decision 0010). */
  readonly onTiming?: (name: string, ms: number) => void;
}

const utf8 = new TextEncoder();

/** "d.docs.live.net" for a web address, "path" for a file on this computer. */
function addressKind(ref: string): string {
  if (!/^https?:\/\//i.test(ref)) {
    return "path";
  }
  try {
    return new URL(ref).host.toLowerCase();
  } catch {
    return "path";
  }
}

export class Lineage {
  private readonly engine: Engine;
  private readonly storage: Storage;
  private readonly host: DocumentHost;
  private readonly now: () => number;
  private readonly onTiming: (name: string, ms: number) => void;
  private readonly history: History;
  private openFile: s.OpenFile | undefined;
  private queue: Promise<unknown> = Promise.resolve();
  /** Library index updates, one after another, after the commands that wrote. */
  private indexing: Promise<unknown> = Promise.resolve();

  constructor(options: LineageOptions) {
    this.engine = options.engine;
    this.storage = options.storage;
    this.host = options.host;
    this.now = options.now ?? Date.now;
    this.onTiming = options.onTiming ?? (() => undefined);
    this.history = new History(this.engine, this.storage);
  }

  /** Resolves once every library index update started so far has finished. */
  indexed(): Promise<void> {
    return this.indexing.then(() => undefined);
  }

  /** Appends lines, then (without holding up the panel) updates the library index. */
  private async append(
    family: string,
    version: string,
    before: Uint8Array,
    lines: string,
  ): Promise<void> {
    const encoded = utf8.encode(lines);
    const written = await this.storage.appendLedger(family, version, encoded);
    this.indexing = this.indexing.then(() =>
      this.history.updateIndex(family, written, concat(before, encoded)),
    );
  }

  /** The open file of the family, once `open` or `startTracking` found it. */
  get file(): s.OpenFile | undefined {
    return this.openFile;
  }

  /** Runs one action at a time, in order, so two clicks never interleave. */
  private serial<T>(name: string, work: () => Promise<T>): Promise<T> {
    const run = async (): Promise<T> => {
      const started = performance.now();
      try {
        return await work();
      } finally {
        this.onTiming(name, performance.now() - started);
      }
    };
    const next = this.queue.then(run, run);
    this.queue = next.catch(() => undefined);
    return next;
  }

  private requireFile(): s.OpenFile {
    if (!this.openFile) {
      throw new LineageError("Open a tracked file first.", "no family is open");
    }
    return this.openFile;
  }

  private fetchBlobs(family: string, hashes: readonly string[]): Promise<void> {
    return this.history.fetchBlobs(family, hashes);
  }

  /**
   * Runs an engine call, fetching whatever blobs it reports missing: from the
   * family the engine names, else from `family`.
   */
  private withBlobs<T>(family: string, run: () => T, rounds?: number): Promise<T> {
    return this.history.withBlobs(family, run, rounds);
  }

  private async ledger(family: string): Promise<Uint8Array> {
    return (await this.storage.readLedger(family)).bytes;
  }

  private storeBlobs(family: string, result: CommandResult): Promise<void> {
    return this.history.storeBlobs(family, result);
  }

  /**
   * Runs a command: read, run, store blobs, change the document (returning how
   * to undo it), append. On a conflicting append, undo and run once more.
   */
  private async press(
    family: string,
    run: (ledger: Uint8Array) => CommandResult,
    change: (result: CommandResult) => Promise<(() => Promise<void>) | undefined> = () =>
      Promise.resolve(undefined),
  ): Promise<CommandResult> {
    for (let attempt = 0; attempt < 2; attempt++) {
      const { bytes: ledger, version } = await this.storage.readLedger(family);
      const result = await this.withBlobs(family, () => run(ledger));
      if (result.outcome.records === 0) {
        return result;
      }
      await this.storeBlobs(family, result);
      const undo = await change(result);
      try {
        await this.append(family, version, ledger, result.outcome.ledgerLines);
        return result;
      } catch (error) {
        if (undo) {
          await undo();
        }
        if (error instanceof StorageConflict && attempt === 0) {
          continue;
        }
        throw error;
      }
    }
    throw new StorageConflict("the ledger kept changing");
  }

  /** Finds out what the open document is, and draws its panel. */
  open(): Promise<Opened> {
    return this.serial("open", async () => {
      const ids = await this.host.readIds();
      if (!ids) {
        this.openFile = undefined;
        const { name, url } = await this.host.describe();
        return { kind: "untracked", name, saved: url !== null };
      }
      let ledger = await this.ledger(ids.family);
      if (ledger.length === 0) {
        this.openFile = undefined;
        return { kind: "elsewhere", ids };
      }
      await this.fetchBlobs(ids.family, this.engine.referencedBlobs(ledger));
      let file = this.engine.openFile(ledger, ids.checkpoint);
      if (file.copyOf === null && file.isLatest && (await this.followMove(file))) {
        ledger = await this.ledger(ids.family);
        file = this.engine.openFile(ledger, ids.checkpoint);
      }
      if (file.copyOf !== null || !file.isLatest) {
        this.openFile = undefined;
        const { name } = await this.host.describe();
        const returning = await this.recogniseBytes(await this.host.readBytes(), name);
        return { kind: "cameBack", file, returning };
      }
      this.openFile = file;
      return this.panelFor(file, ledger);
    });
  }

  /**
   * The master, opened from somewhere new (moved or renamed in OneDrive or on
   * this computer): records where it is now, so My documents shows the right
   * place (decision 0015). Word reports one file differently on the web and on
   * the desktop, so only a new address of the same kind counts as a move. Only
   * a convenience: if it can't be recorded, the panel opens anyway.
   */
  private async followMove(file: s.OpenFile): Promise<boolean> {
    if (file.role !== "master") {
      return false;
    }
    const { url } = await this.host.describe();
    if (!url || url === file.storageRef || addressKind(url) !== addressKind(file.storageRef)) {
      return false;
    }
    try {
      const bytes = await this.host.readBytes();
      const result = await this.press(file.familyId, (ledger) =>
        this.engine.relink(ledger, file.nodeId, url, bytes, this.now()),
      );
      return result.outcome.records > 0;
    } catch {
      return false;
    }
  }

  private async panelFor(file: s.OpenFile, ledger: Uint8Array): Promise<Opened> {
    if (file.role === "master") {
      const panel = await this.withBlobs(file.familyId, () => this.engine.masterPanel(ledger));
      return { kind: "master", file, panel };
    }
    const panel = await this.withBlobs(file.familyId, () =>
      this.engine.versionPanel(ledger, file.nodeId),
    );
    return { kind: "version", file, panel };
  }

  /** The panel for the open file, read fresh. */
  refresh(): Promise<Opened> {
    return this.serial("refresh", async () => {
      const file = this.requireFile();
      const ledger = await this.ledger(file.familyId);
      // The open file may have a new checkpoint now; find it again by its ids.
      const ids = await this.host.readIds();
      const current = ids ? this.engine.openFile(ledger, ids.checkpoint) : file;
      this.openFile = current;
      return this.panelFor(current, ledger);
    });
  }

  /** "Start tracking": the open document becomes the master of a new family. */
  startTracking(): Promise<Opened> {
    return this.serial("startTracking", async () => {
      const { name, url } = await this.host.describe();
      if (url === null) {
        throw new LineageError(
          "Save this file first, then start tracking it.",
          "the document has no URL",
        );
      }
      const bytes = await this.host.readBytes();
      const result = this.engine.startTracking(bytes, name, url, this.now());
      const { familyId, ids } = result.outcome;
      if (!familyId || !ids) {
        throw new LineageError(
          "Lineage couldn't start tracking this file.",
          "no family or ids came back",
        );
      }
      const { bytes: existing, version } = await this.storage.readLedger(familyId);
      if (existing.length > 0) {
        throw new LineageError(
          "Lineage couldn't start tracking this file.",
          "the new family already has a ledger",
        );
      }
      await this.storeBlobs(familyId, result);
      await this.host.writeIds(ids);
      try {
        await this.host.save();
        await this.append(familyId, version, existing, result.outcome.ledgerLines);
      } catch (error) {
        await this.host.writeIds(null);
        throw error;
      }
      const ledger = await this.ledger(familyId);
      const file = this.engine.openFile(ledger, ids.checkpoint);
      this.openFile = file;
      return this.panelFor(file, ledger);
    });
  }

  /** "What changed" in the open file since its latest checkpoint. */
  whatChanged(): Promise<s.WhatChanged> {
    return this.serial("whatChanged", async () => {
      const file = this.requireFile();
      const bytes = await this.host.readBytes();
      const ledger = await this.ledger(file.familyId);
      return this.withBlobs(file.familyId, () =>
        this.engine.whatChanged(ledger, file.nodeId, bytes),
      );
    });
  }

  /** "Save checkpoint". Returns false when nothing had changed. */
  saveCheckpoint(note: string): Promise<boolean> {
    return this.serial("saveCheckpoint", async () => {
      const file = this.requireFile();
      const bytes = await this.host.readBytes();
      const previous = await this.host.readIds();
      const result = await this.press(
        file.familyId,
        (ledger) => this.engine.saveCheckpoint(ledger, file.nodeId, bytes, note, this.now()),
        async ({ outcome }) => {
          if (!outcome.ids) {
            return undefined;
          }
          await this.host.writeIds(outcome.ids);
          await this.host.save();
          return async () => {
            await this.host.writeIds(previous);
          };
        },
      );
      return !result.outcome.nothingChanged;
    });
  }

  /** "Make a tailored version" of the master's latest checkpoint. */
  makeTailoredVersion(name: string): Promise<{ name: string; ref: string; nodeId: string }> {
    return this.serial("makeTailoredVersion", async () => {
      const file = this.requireFile();
      const clean = checkFileName(withExtension(name, file.format));
      if (
        (await this.storage.fileNames("version")).some(
          (n) => n.toLowerCase() === clean.toLowerCase(),
        )
      ) {
        throw new FileExists(clean);
      }
      const ref = this.storage.refFor("version", clean);
      const result = await this.press(file.familyId, (ledger) =>
        this.engine.makeTailoredVersion(ledger, clean, ref, this.now()),
      );
      const { newFile } = result;
      const nodeId = result.outcome.created;
      if (!newFile || !nodeId) {
        throw new LineageError("Lineage couldn't make the tailored version.", "no file came back");
      }
      await this.storage.writeFile("version", clean, newFile);
      return { name: clean, ref, nodeId };
    });
  }

  /** "Offer it to …": the versions named in the master panel's call-out. */
  offerToVersions(versions: readonly string[]): Promise<void> {
    return this.serial("offerToVersions", async () => {
      const file = this.requireFile();
      await this.press(file.familyId, (ledger) =>
        this.engine.offerToVersions(ledger, versions, this.now()),
      );
    });
  }

  /** The updates offered to the open tailored version. */
  updates(): Promise<s.Updates> {
    return this.serial("updates", async () => {
      const file = this.requireFile();
      const bytes = await this.host.readBytes();
      const ledger = await this.ledger(file.familyId);
      return this.withBlobs(file.familyId, () => this.engine.updates(ledger, file.nodeId, bytes));
    });
  }

  /**
   * "Bring in" and "Decline". Brought-in changes are written into the open
   * document as tracked changes by Lineage, read back and checked; if Word
   * didn't take them exactly, the original goes back and nothing is recorded.
   */
  decide(cards: readonly s.CardDecision[]): Promise<{ wrote: boolean }> {
    return this.serial("decide", async () => {
      const file = this.requireFile();
      const original = await this.host.readBytes();
      const result = await this.press(
        file.familyId,
        (ledger) => this.engine.decide(ledger, file.nodeId, original, cards, this.now()),
        (made) => this.writeAndSave(made, original),
      );
      return { wrote: result.file !== undefined };
    });
  }

  /**
   * The document change for a command that writes tracked changes: write,
   * check, set the ids, save; and how to undo it if the append fails.
   */
  private async writeAndSave(
    { file: written, outcome }: CommandResult,
    original: Uint8Array,
  ): Promise<(() => Promise<void>) | undefined> {
    if (!written) {
      return undefined;
    }
    await this.write(written, original);
    if (outcome.ids) {
      await this.host.writeIds(outcome.ids);
    }
    await this.host.save();
    return async () => {
      await this.write(original, written);
      await this.host.save();
    };
  }

  /** Every family's ledger in storage, for recognising a file. */
  private async ledgers(): Promise<FamilyLedger[]> {
    const out: FamilyLedger[] = [];
    for (const family of await this.storage.families()) {
      const { bytes } = await this.storage.readLedger(family);
      if (bytes.length > 0) {
        out.push({ family, ledger: bytes });
      }
    }
    return out;
  }

  private async recogniseBytes(bytes: Uint8Array, name: string): Promise<Returning> {
    const ledgers = await this.ledgers();
    // Each family with candidates may need its manifests, then their parts.
    const recognition = await this.withBlobs(
      "",
      () => this.engine.recognise(bytes, ledgers),
      2 * ledgers.length + 2,
    );
    return { bytes, name, recognition };
  }

  /** "A file came back": which copy `bytes` is, looking in every family. */
  recognise(bytes: Uint8Array, name: string): Promise<Returning> {
    return this.serial("recognise", () => this.recogniseBytes(bytes, name));
  }

  /** Is the open (untracked) document a copy that came back? */
  recogniseOpen(): Promise<Returning> {
    return this.serial("recogniseOpen", async () => {
      const { name } = await this.host.describe();
      return this.recogniseBytes(await this.host.readBytes(), name);
    });
  }

  /**
   * "Review the changes": keeps the returned file exactly as it came back, as
   * the copy the panel recognised or the person chose. Returns the record and
   * the file it came from.
   */
  recordReturned(
    returning: Returning,
    candidate: s.Candidate,
    matchedBy: s.MatchedBy,
  ): Promise<{ returned: string; family: string; node: string; nodeName: string }> {
    return this.serial("recordReturned", async () => {
      const result = await this.press(candidate.family, (ledger) =>
        this.engine.recordReturned(
          ledger,
          returning.bytes,
          {
            name: returning.name,
            checkpoint: candidate.checkpoint,
            sent: candidate.sent,
            matchedBy,
          },
          this.now(),
        ),
      );
      const returned = result.outcome.created;
      if (!returned) {
        throw new LineageError("Lineage couldn't keep the file.", "no record came back");
      }
      return {
        returned,
        family: candidate.family,
        node: candidate.node,
        nodeName: candidate.nodeName,
      };
    });
  }

  /** The changes in a returned file still to decide, against the open file. */
  returnedReview(returned: string): Promise<s.ReturnReview> {
    return this.serial("returnedReview", async () => {
      const file = this.requireFile();
      const bytes = await this.host.readBytes();
      const ledger = await this.ledger(file.familyId);
      return this.withBlobs(file.familyId, () =>
        this.engine.returnedReview(ledger, file.nodeId, bytes, returned),
      );
    });
  }

  /**
   * "Apply" on a review: kept changes are written into the open document as
   * tracked changes by Lineage, read back and checked, then a checkpoint is
   * saved; skipped ones are recorded. As with `decide`, if Word didn't take the
   * changes exactly, the original goes back and nothing is recorded.
   * `wordsOnly` brings only their words, in this file's formatting: the
   * review's `formatting.wordsOnly` says when Lineage suggests it.
   */
  decideReturned(
    returned: string,
    cards: readonly s.CardDecision[],
    { wordsOnly }: { wordsOnly: boolean },
  ): Promise<{ wrote: boolean }> {
    return this.serial("decideReturned", async () => {
      const file = this.requireFile();
      const original = await this.host.readBytes();
      const result = await this.press(
        file.familyId,
        (ledger) =>
          this.engine.decideReturned(
            ledger,
            file.nodeId,
            original,
            returned,
            cards,
            wordsOnly,
            this.now(),
          ),
        (made) => this.writeAndSave(made, original),
      );
      return { wrote: result.file !== undefined };
    });
  }

  /** "Open what came back": the exact bytes, as a new document. */
  openReturned(family: string, returned: string): Promise<void> {
    return this.serial("openReturned", async () => {
      const ledger = await this.ledger(family);
      const bytes = await this.withBlobs(family, () => this.engine.openReturned(ledger, returned));
      await this.host.openCopy(bytes);
    });
  }

  /** Opens a file of a family: a tailored version from storage, or the master where Word has it. */
  openNode(family: string, node: string): Promise<void> {
    return this.serial("openNode", async () => {
      const found = this.engine.family(await this.ledger(family)).nodes.find((n) => n.id === node);
      if (!found) {
        throw new LineageError("Lineage couldn't find that file.", `node ${node} is unknown`);
      }
      const location: FileLocation =
        found.role === "variant"
          ? await this.storage.fileLocation(found.storageRef)
          : /^https?:\/\//i.test(found.storageRef)
            ? { webUrl: found.storageRef }
            : { path: found.storageRef };
      await this.host.openFile(location);
    });
  }

  /** Puts `bytes` into the open document and checks it landed; else puts `previous` back. */
  private async write(bytes: Uint8Array, previous: Uint8Array): Promise<void> {
    await this.host.replaceContents(bytes);
    const landed = this.engine.checkWritten(bytes, await this.host.readBytes());
    if (landed.ok) {
      return;
    }
    const detail = `written: ${JSON.stringify(landed)}`;
    await this.host.replaceContents(previous);
    const back = this.engine.checkWritten(previous, await this.host.readBytes());
    if (!back.ok) {
      throw new LineageError(
        "Word didn't take the changes exactly as Lineage wrote them, and putting your file back didn't work either. Use Undo in Word, then check the file.",
        `${detail}; restored: ${JSON.stringify(back)}`,
      );
    }
    throw new LineageError(
      "Word didn't take the changes exactly as Lineage wrote them, so your file was put back as it was. Nothing was changed.",
      detail,
    );
  }

  /** Who is in the family, for "Send a copy". */
  family(): Promise<s.Family> {
    return this.serial("family", async () => {
      const file = this.requireFile();
      return this.engine.family(await this.ledger(file.familyId));
    });
  }

  /**
   * "Save copy and record it" for `node`: the open file, or a tailored version
   * read from storage. Saves a checkpoint first if the file has unsaved changes.
   */
  sendCopy(node: string, args: s.SendArgs): Promise<{ name: string; ref: string }> {
    return this.serial("sendCopy", async () => {
      const file = this.requireFile();
      const isOpen = node === file.nodeId;
      let bytes: Uint8Array;
      let stored: s.Node | undefined;
      if (isOpen) {
        bytes = await this.host.readBytes();
      } else {
        stored = this.engine
          .family(await this.ledger(file.familyId))
          .nodes.find((n) => n.id === node);
        if (stored?.role !== "variant") {
          throw new LineageError(
            "Open that file to send a copy of it.",
            `node ${node} is not a stored version`,
          );
        }
        bytes = await this.storage.readFile(stored.storageRef);
      }
      const previous = isOpen ? await this.host.readIds() : null;
      const result = await this.press(
        file.familyId,
        (ledger) => this.engine.sendCopy(ledger, node, bytes, args, this.now()),
        async ({ outcome, file: updated }) => {
          if (isOpen && outcome.ids) {
            await this.host.writeIds(outcome.ids);
            await this.host.save();
            return async () => {
              await this.host.writeIds(previous);
            };
          }
          if (stored && updated) {
            await this.storage.writeFile("version", stored.displayName, updated, true);
            const before = bytes;
            const name = stored.displayName;
            return async () => {
              await this.storage.writeFile("version", name, before, true);
            };
          }
          return undefined;
        },
      );
      const { newFile } = result;
      const suggested = result.outcome.newFileName;
      if (!newFile || !suggested) {
        throw new LineageError("Lineage couldn't make the copy.", "no copy came back");
      }
      const taken = new Set((await this.storage.fileNames("sent")).map((n) => n.toLowerCase()));
      let name = checkFileName(suggested.replace(/["*:<>?/\\|]/g, " "));
      for (let n = 2; taken.has(name.toLowerCase()); n++) {
        name = numbered(checkFileName(suggested.replace(/["*:<>?/\\|]/g, " ")), n);
      }
      const saved = await this.storage.writeFile("sent", name, newFile);
      return { name: saved.name, ref: saved.ref };
    });
  }

  /** "What I've sent". */
  sentList(search?: string): Promise<s.SentList> {
    return this.serial("sentList", async () => {
      const file = this.requireFile();
      return this.engine.sentList(await this.ledger(file.familyId), search);
    });
  }

  /** "Open" on a sent copy: the exact bytes they received, as a new document. */
  openSentCopy(sent: string): Promise<void> {
    return this.serial("openSentCopy", async () => {
      const file = this.requireFile();
      const ledger = await this.ledger(file.familyId);
      const bytes = await this.withBlobs(file.familyId, () =>
        this.engine.openSentCopy(ledger, sent),
      );
      await this.host.openCopy(bytes);
    });
  }

  /**
   * The pitch deck panel of the open master deck, with what changed in it
   * since its last checkpoint (decision 0018).
   */
  deckPanel(): Promise<s.DeckPanel> {
    return this.serial("deckPanel", async () => {
      const file = this.requireFile();
      const bytes = await this.host.readBytes();
      const ledger = await this.ledger(file.familyId);
      return this.withBlobs(file.familyId, () => this.engine.deckPanel(ledger, bytes));
    });
  }

  /**
   * "Update N versions" from the master deck's panel (decision 0018). Saves a
   * checkpoint of the open master first when it has changes. Then each chosen
   * version's file is read as stored, the engine updates it, and each new file
   * replaces the stored one only if nobody changed it meanwhile. The history is
   * appended once, after every file; if any write or the append fails, every
   * file already replaced is put back and nothing is recorded.
   */
  updateVersions(
    versions: readonly string[],
    choices: readonly s.VersionChoice[] = [],
    note = "Before updating the versions",
  ): Promise<s.UpdateRun> {
    return this.serial("updateVersions", async () => {
      const file = this.requireFile();
      if (file.role !== "master") {
        throw new LineageError(
          "Versions are updated from the master deck.",
          "updateVersions on a tailored version",
        );
      }
      // The master's changes go into a checkpoint first, as "Save checkpoint" does.
      const master = await this.host.readBytes();
      const previousIds = await this.host.readIds();
      await this.press(
        file.familyId,
        (ledger) => this.engine.saveCheckpoint(ledger, file.nodeId, master, note, this.now()),
        async ({ outcome }) => {
          if (!outcome.ids) {
            return undefined;
          }
          await this.host.writeIds(outcome.ids);
          await this.host.save();
          return async () => {
            await this.host.writeIds(previousIds);
          };
        },
      );
      for (let attempt = 0; attempt < 2; attempt++) {
        const { bytes: ledger, version } = await this.storage.readLedger(file.familyId);
        const nodes = this.engine.family(ledger).nodes;
        const inputs: { node: string; file: Uint8Array; ref: string; version: string }[] = [];
        for (const node of versions) {
          const stored = nodes.find((n) => n.id === node && n.role === "variant");
          if (!stored) {
            throw new LineageError(
              "That version isn't in this deck's family.",
              `node ${node} is not a tailored version`,
            );
          }
          const read = await this.storage.readFileVersioned(stored.storageRef);
          inputs.push({ node, file: read.bytes, ref: stored.storageRef, version: read.version });
        }
        const result: UpdateResult = await this.withBlobs(file.familyId, () =>
          this.engine.updateVersions(ledger, inputs, choices, this.now()),
        );
        const replaced: { ref: string; bytes: Uint8Array }[] = [];
        const putBack = async () => {
          for (const done of replaced.reverse()) {
            const now = await this.storage.readFileVersioned(done.ref);
            await this.storage.replaceFile(done.ref, done.bytes, now.version);
          }
        };
        try {
          for (const input of inputs) {
            const bytes = result.files.get(input.node);
            if (bytes) {
              await this.storage.replaceFile(input.ref, bytes, input.version);
              replaced.push({ ref: input.ref, bytes: input.file });
            }
          }
          if (result.run.outcome.records > 0) {
            for (const [hash, blob] of result.blobs) {
              await this.storage.putBlob(file.familyId, hash, blob);
            }
            await this.append(file.familyId, version, ledger, result.run.outcome.ledgerLines);
          }
          return result.run;
        } catch (error) {
          await putBack();
          if (error instanceof StorageConflict && attempt === 0) {
            continue;
          }
          throw error;
        }
      }
      throw new StorageConflict("the ledger kept changing");
    });
  }

  /** "Open" on a tailored version. */
  openStored(ref: string): Promise<void> {
    return this.serial("openStored", async () => {
      await this.host.openFile(await this.storage.fileLocation(ref));
    });
  }
}
