// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Typed wrapper around the wasm engine. Everything that crosses the boundary is
// checked here (schemas.ts), so the rest of the TypeScript code can trust these
// types. TypeScript never parses document XML: it hands bytes to the engine and
// gets bytes back.

import * as v from "valibot";
import init, {
  Engine as WasmEngine,
  echoBytes,
  engineVersion,
  initSync,
} from "../wasm-pkg/lineage_wasm.js";
import type { InitInput, Reply, SyncInitInput } from "../wasm-pkg/lineage_wasm.js";
import * as s from "./schemas.js";

/** Thrown when a value coming back from the engine is not the shape we expect. */
export class EngineBoundaryError extends Error {
  override name = "EngineBoundaryError";
}

/** A failure the engine reported, with the words the panel shows. */
export class EngineError extends Error {
  override name = "EngineError";

  constructor(
    readonly kind: s.ErrorKind | "crashed",
    readonly userMessage: string,
    readonly detail: string,
    /** For `missingBlobs`: what to fetch before calling again. */
    readonly hashes: readonly string[] = [],
    /** For `missingBlobs` from a call over several families: where they are. */
    readonly family?: string,
  ) {
    super(`${kind}: ${detail}`);
  }
}

/** What a command produced: the outcome, and bytes that travel beside it. */
export interface CommandResult {
  readonly outcome: s.Outcome;
  /** New bytes for the file the command acted on. */
  readonly file: Uint8Array | undefined;
  /** A new file: a tailored version or a sent copy. */
  readonly newFile: Uint8Array | undefined;
  /** Every blob in `outcome.blobs`, by hash. */
  readonly blobs: ReadonlyMap<string, Uint8Array>;
}

/** "Update N versions": the run, the blobs to store, and each version's new bytes. */
export interface UpdateResult {
  readonly run: s.UpdateRun;
  readonly blobs: ReadonlyMap<string, Uint8Array>;
  /** New bytes for each version written, by node id. */
  readonly files: ReadonlyMap<string, Uint8Array>;
}

/** The engine as the TypeScript shell sees it. Every method is synchronous. */
export interface Engine {
  /** Engine version, from the Rust workspace manifest. */
  readonly version: string;
  /** Returns a copy of `bytes`. Proves the bridge. */
  echoBytes(bytes: Uint8Array): Uint8Array;
  addBlob(hash: string, bytes: Uint8Array): void;
  hasBlob(hash: string): boolean;
  cachedBytes(): number;
  setAuthor(author: string | null): void;
  readIds(file: Uint8Array): s.Ids | null;
  writeIds(file: Uint8Array, ids: s.Ids): Uint8Array;
  family(ledger: Uint8Array): s.Family;
  openFile(ledger: Uint8Array, checkpoint: string): s.OpenFile;
  referencedBlobs(ledger: Uint8Array): string[];
  startTracking(
    file: Uint8Array,
    displayName: string,
    storageRef: string,
    nowMs: number,
  ): CommandResult;
  saveCheckpoint(
    ledger: Uint8Array,
    node: string,
    file: Uint8Array,
    note: string,
    nowMs: number,
  ): CommandResult;
  makeTailoredVersion(
    ledger: Uint8Array,
    displayName: string,
    storageRef: string,
    nowMs: number,
  ): CommandResult;
  whatChanged(ledger: Uint8Array, node: string, file: Uint8Array): s.WhatChanged;
  masterPanel(ledger: Uint8Array): s.MasterPanel;
  versionPanel(ledger: Uint8Array, node: string): s.VersionPanel;
  offerToVersions(ledger: Uint8Array, versions: readonly string[], nowMs: number): CommandResult;
  updates(ledger: Uint8Array, node: string, file: Uint8Array): s.Updates;
  /** The pitch deck panel; `open` is the open deck, to show what just changed. */
  deckPanel(ledger: Uint8Array, open: Uint8Array | undefined): s.DeckPanel;
  /**
   * "Update N versions" for `versions` (each node and its file as stored).
   * `open` are chosen versions whose files can't be changed now: each gets a
   * line saying so, and nothing is recorded for it.
   */
  updateVersions(
    ledger: Uint8Array,
    versions: readonly { readonly node: string; readonly file: Uint8Array }[],
    choices: readonly s.VersionChoice[],
    nowMs: number,
    open?: readonly string[],
  ): UpdateResult;
  decide(
    ledger: Uint8Array,
    node: string,
    file: Uint8Array,
    cards: readonly s.CardDecision[],
    nowMs: number,
  ): CommandResult;
  sendCopy(
    ledger: Uint8Array,
    node: string,
    file: Uint8Array,
    args: s.SendArgs,
    nowMs: number,
  ): CommandResult;
  sentList(ledger: Uint8Array, search?: string): s.SentList;
  openSentCopy(ledger: Uint8Array, sent: string): Uint8Array;
  /** Which copy `file` is, looking in every family given. */
  recognise(file: Uint8Array, ledgers: readonly FamilyLedger[]): s.Recognition;
  recordReturned(
    ledger: Uint8Array,
    file: Uint8Array,
    args: s.RecordArgs,
    nowMs: number,
  ): CommandResult;
  returnedReview(
    ledger: Uint8Array,
    node: string,
    file: Uint8Array,
    returned: string,
  ): s.ReturnReview;
  /** `wordsOnly` brings their words in this file's formatting (decision 0014). */
  decideReturned(
    ledger: Uint8Array,
    node: string,
    file: Uint8Array,
    returned: string,
    cards: readonly s.CardDecision[],
    wordsOnly: boolean,
    nowMs: number,
  ): CommandResult;
  openReturned(ledger: Uint8Array, returned: string): Uint8Array;
  checkWritten(expected: Uint8Array, actual: Uint8Array): s.Written;

  // My documents (decision 0015).
  /** The library straight from the index; null when the index can't be trusted. */
  libraryFromIndex(index: Uint8Array): s.Library | null;
  /** The families whose ledgers must be read to bring the index up to date. */
  staleFamilies(index: Uint8Array | undefined, observed: readonly Observed[]): string[];
  /** The library, and the index to store when it changed. */
  refreshLibrary(
    index: Uint8Array | undefined,
    observed: readonly Observed[],
    ledgers: readonly FamilyLedger[],
  ): { library: s.Library; index: Uint8Array | undefined };
  /** The index after one family's ledger was written. */
  updateIndex(
    index: Uint8Array | undefined,
    family: string,
    version: string,
    ledger: Uint8Array,
  ): Uint8Array;
  /** A document's page and family tree, from as much of its ledger as can be read. */
  document(ledger: Uint8Array): s.DocumentView;
  compare(ledger: Uint8Array, a: string, b: string): s.WhatChanged;
  restoreAsNewVersion(
    ledger: Uint8Array,
    checkpoint: string,
    displayName: string,
    storageRef: string,
    nowMs: number,
  ): CommandResult;
  markMilestone(
    ledger: Uint8Array,
    checkpoint: string,
    label: string | null,
    nowMs: number,
  ): CommandResult;
  relink(
    ledger: Uint8Array,
    node: string,
    storageRef: string,
    file: Uint8Array,
    nowMs: number,
  ): CommandResult;
}

/** A family folder and its ledger's version as storage reports it now. */
export interface Observed {
  readonly family: string;
  readonly version: string;
}

/** One family's stored ledger, for calls that look across families. */
export interface FamilyLedger {
  readonly family: string;
  readonly ledger: Uint8Array;
}

const ledgerText = new TextDecoder("utf-8", { fatal: true });
/** For ledgers that may be damaged: bad bytes become U+FFFD, and the engine reports them. */
const lenientText = new TextDecoder("utf-8");

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

let initialised = false;

/** Loads the engine from the module's bytes (Node and tests). */
export function loadEngine(wasm: SyncInitInput, seed?: Uint8Array): Engine {
  if (!initialised) {
    initSync({ module: wasm });
    initialised = true;
  }
  return wrap(seed ?? randomSeed());
}

/**
 * Loads the engine in a browser, compiling while it downloads. Returns how long
 * that took, for the start-up budget (decision 0010).
 */
export async function loadEngineAsync(
  wasm: InitInput | Promise<InitInput>,
  seed?: Uint8Array,
): Promise<{ engine: Engine; loadMs: number }> {
  const started = performance.now();
  if (!initialised) {
    await init({ module_or_path: wasm });
    initialised = true;
  }
  const engine = wrap(seed ?? randomSeed());
  return { engine, loadMs: performance.now() - started };
}

function observedList(observed: readonly Observed[]): Observed[] {
  return observed.map((o) => ({
    family: checked(s.Id, o.family, "observed"),
    version: checked(v.pipe(v.string(), v.maxLength(512)), o.version, "observed"),
  }));
}

function randomSeed(): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(32));
}

function checked<S extends v.GenericSchema>(
  schema: S,
  value: unknown,
  what: string,
): v.InferOutput<S> {
  const result = v.safeParse(schema, value);
  if (!result.success) {
    const issue = result.issues[0];
    const path = issue.path?.map((p) => String(p.key)).join(".") ?? "";
    throw new EngineBoundaryError(`${what}: ${path} ${issue.message}`.trim());
  }
  return result.output;
}

function wrap(seed: Uint8Array): Engine {
  const version: unknown = engineVersion();
  if (typeof version !== "string" || !SEMVER.test(version)) {
    throw new EngineBoundaryError(`engine returned an invalid version: ${String(version)}`);
  }
  const inner = new WasmEngine(seed);
  let crashed = false;

  // Runs one call. A trap (a Rust panic) leaves the module in an unknown state,
  // so after one every call fails until the task pane is reloaded.
  function call(
    what: string,
    run: (engine: WasmEngine) => Reply,
  ): { value: unknown; reply: Reply } {
    if (crashed) {
      throw new EngineError("crashed", CRASHED, `${what} after an earlier crash`);
    }
    let reply: Reply;
    try {
      reply = run(inner);
    } catch (error) {
      if (error instanceof WebAssembly.RuntimeError) {
        crashed = true;
        throw new EngineError("crashed", CRASHED, `${what}: ${error.message}`);
      }
      throw error;
    }
    let envelope: s.Envelope;
    try {
      envelope = checked(s.Envelope, JSON.parse(reply.json()), what);
    } catch (error) {
      reply.free();
      throw error;
    }
    if (!envelope.ok) {
      reply.free();
      const e = envelope.error;
      throw new EngineError(e.kind, e.userMessage, e.detail, e.hashes ?? [], e.family);
    }
    return { value: envelope.value, reply };
  }

  function view<S extends v.GenericSchema>(
    what: string,
    schema: S,
    run: (engine: WasmEngine) => Reply,
  ): v.InferOutput<S> {
    const { value, reply } = call(what, run);
    reply.free();
    return checked(schema, value, what);
  }

  function bytes(what: string, run: (engine: WasmEngine) => Reply): Uint8Array {
    const { reply } = call(what, run);
    try {
      const file = reply.takeFile();
      if (!(file instanceof Uint8Array)) {
        throw new EngineBoundaryError(`${what}: no bytes came back`);
      }
      return file;
    } finally {
      reply.free();
    }
  }

  function command(what: string, run: (engine: WasmEngine) => Reply): CommandResult {
    const { value, reply } = call(what, run);
    try {
      const outcome = checked(s.Outcome, value, what);
      const blobs = new Map<string, Uint8Array>();
      for (const hash of outcome.blobs) {
        const blob = reply.blob(hash);
        if (!(blob instanceof Uint8Array)) {
          throw new EngineBoundaryError(`${what}: blob ${hash} is listed but missing`);
        }
        blobs.set(hash, blob);
      }
      const file = reply.takeFile();
      const newFile = reply.takeNewFile();
      if (outcome.hasFile !== (file !== undefined)) {
        throw new EngineBoundaryError(`${what}: hasFile does not match the bytes`);
      }
      if ((outcome.newFileName !== null) !== (newFile !== undefined)) {
        throw new EngineBoundaryError(`${what}: newFileName does not match the bytes`);
      }
      return { outcome, file, newFile, blobs };
    } finally {
      reply.free();
    }
  }

  const json = (value: unknown): string => JSON.stringify(value);

  return {
    version,
    echoBytes(input: Uint8Array): Uint8Array {
      const result: unknown = echoBytes(input);
      if (!(result instanceof Uint8Array)) {
        throw new EngineBoundaryError("engine returned something other than bytes");
      }
      return result;
    },
    addBlob(hash, data) {
      checked(s.BlobHash, hash, "addBlob");
      view("addBlob", v.null(), (e) => e.addBlob(hash, data));
    },
    hasBlob: (hash) => inner.hasBlob(hash),
    cachedBytes: () => inner.cachedBytes(),
    setAuthor: (author) => {
      inner.setAuthor(author);
    },
    readIds: (file) => view("readIds", v.nullable(s.Ids), (e) => e.readIds(file)),
    writeIds: (file, ids) =>
      bytes("writeIds", (e) => e.writeIds(file, json(checked(s.Ids, ids, "writeIds")))),
    family: (ledger) => view("family", s.Family, (e) => e.family(ledger)),
    openFile: (ledger, checkpoint) =>
      view("openFile", s.OpenFile, (e) => e.openFile(ledger, checkpoint)),
    referencedBlobs: (ledger) =>
      view("referencedBlobs", v.array(s.BlobHash), (e) => e.referencedBlobs(ledger)),
    startTracking: (file, displayName, storageRef, nowMs) =>
      command("startTracking", (e) => e.startTracking(file, displayName, storageRef, nowMs)),
    saveCheckpoint: (ledger, node, file, note, nowMs) =>
      command("saveCheckpoint", (e) => e.saveCheckpoint(ledger, node, file, note, nowMs)),
    makeTailoredVersion: (ledger, displayName, storageRef, nowMs) =>
      command("makeTailoredVersion", (e) =>
        e.makeTailoredVersion(ledger, displayName, storageRef, nowMs),
      ),
    whatChanged: (ledger, node, file) =>
      view("whatChanged", s.WhatChanged, (e) => e.whatChanged(ledger, node, file)),
    masterPanel: (ledger) => view("masterPanel", s.MasterPanel, (e) => e.masterPanel(ledger)),
    versionPanel: (ledger, node) =>
      view("versionPanel", s.VersionPanel, (e) => e.versionPanel(ledger, node)),
    offerToVersions: (ledger, versions, nowMs) =>
      command("offerToVersions", (e) =>
        e.offerToVersions(ledger, json(checked(v.array(s.Id), versions, "offerToVersions")), nowMs),
      ),
    updates: (ledger, node, file) =>
      view("updates", s.Updates, (e) => e.updates(ledger, node, file)),
    deckPanel: (ledger, open) => view("deckPanel", s.DeckPanel, (e) => e.deckPanel(ledger, open)),
    updateVersions(ledger, versions, choices, nowMs, open = []) {
      for (const version of versions) {
        view("stageVersionFile", v.null(), (e) =>
          e.stageVersionFile(checked(s.Id, version.node, "updateVersions"), version.file),
        );
      }
      for (const node of open) {
        view("markVersionOpen", v.null(), (e) =>
          e.markVersionOpen(checked(s.Id, node, "updateVersions")),
        );
      }
      const { value, reply } = call("updateVersions", (e) =>
        e.updateVersions(
          ledger,
          json(checked(v.array(s.VersionChoice), choices, "updateVersions")),
          nowMs,
        ),
      );
      try {
        const run = checked(s.UpdateRun, value, "updateVersions");
        const blobs = new Map<string, Uint8Array>();
        for (const hash of run.outcome.blobs) {
          const blob = reply.blob(hash);
          if (!(blob instanceof Uint8Array)) {
            throw new EngineBoundaryError(`updateVersions: blob ${hash} is listed but missing`);
          }
          blobs.set(hash, blob);
        }
        const files = new Map<string, Uint8Array>();
        for (const line of run.versions) {
          const file = reply.takeVersionFile(line.node);
          if (line.hasFile !== (file !== undefined)) {
            throw new EngineBoundaryError("updateVersions: hasFile does not match the bytes");
          }
          if (file !== undefined) files.set(line.node, file);
        }
        return { run, blobs, files };
      } finally {
        reply.free();
      }
    },
    decide: (ledger, node, file, cards, nowMs) =>
      command("decide", (e) =>
        e.decide(
          ledger,
          node,
          file,
          json(checked(v.array(s.CardDecision), cards, "decide")),
          nowMs,
        ),
      ),
    sendCopy: (ledger, node, file, args, nowMs) =>
      command("sendCopy", (e) =>
        e.sendCopy(ledger, node, file, json(checked(s.SendArgs, args, "sendCopy")), nowMs),
      ),
    sentList: (ledger, search) =>
      view("sentList", s.SentList, (e) => e.sentList(ledger, search ?? null)),
    openSentCopy: (ledger, sent) => bytes("openSentCopy", (e) => e.openSentCopy(ledger, sent)),
    recognise: (file, ledgers) =>
      view("recognise", s.Recognition, (e) =>
        e.recognise(
          file,
          json(
            ledgers.map((l) => ({
              family: checked(s.Id, l.family, "recognise"),
              ledger: ledgerText.decode(l.ledger),
            })),
          ),
        ),
      ),
    recordReturned: (ledger, file, args, nowMs) =>
      command("recordReturned", (e) =>
        e.recordReturned(ledger, file, json(checked(s.RecordArgs, args, "recordReturned")), nowMs),
      ),
    returnedReview: (ledger, node, file, returned) =>
      view("returnedReview", s.ReturnReview, (e) => e.returnedReview(ledger, node, file, returned)),
    decideReturned: (ledger, node, file, returned, cards, wordsOnly, nowMs) =>
      command("decideReturned", (e) =>
        e.decideReturned(
          ledger,
          node,
          file,
          returned,
          json(checked(v.array(s.CardDecision), cards, "decideReturned")),
          checked(v.boolean(), wordsOnly, "decideReturned"),
          nowMs,
        ),
      ),
    openReturned: (ledger, returned) =>
      bytes("openReturned", (e) => e.openReturned(ledger, returned)),
    checkWritten: (expected, actual) =>
      view("checkWritten", s.Written, (e) => e.checkWritten(expected, actual)),
    libraryFromIndex: (index) =>
      view("libraryFromIndex", v.nullable(s.Library), (e) => e.libraryFromIndex(index)),
    staleFamilies: (index, observed) =>
      view("staleFamilies", v.array(s.Id), (e) =>
        e.staleFamilies(index, json(observedList(observed))),
      ),
    refreshLibrary(index, observed, ledgers) {
      const { value, reply } = call("refreshLibrary", (e) =>
        e.refreshLibrary(
          index,
          json(observedList(observed)),
          json(
            ledgers.map((l) => ({
              family: checked(s.Id, l.family, "refreshLibrary"),
              ledger: lenientText.decode(l.ledger),
            })),
          ),
        ),
      );
      try {
        const refreshed = checked(s.Refreshed, value, "refreshLibrary");
        const file = reply.takeFile();
        if (refreshed.indexChanged !== (file !== undefined)) {
          throw new EngineBoundaryError("refreshLibrary: indexChanged does not match the bytes");
        }
        return { library: refreshed.library, index: file };
      } finally {
        reply.free();
      }
    },
    updateIndex: (index, family, version, ledger) =>
      bytes("updateIndex", (e) =>
        e.updateIndex(index, checked(s.Id, family, "updateIndex"), version, ledger),
      ),
    document: (ledger) => view("document", s.DocumentView, (e) => e.document(ledger)),
    compare: (ledger, a, b) => view("compare", s.WhatChanged, (e) => e.compare(ledger, a, b)),
    restoreAsNewVersion: (ledger, checkpoint, displayName, storageRef, nowMs) =>
      command("restoreAsNewVersion", (e) =>
        e.restoreAsNewVersion(ledger, checkpoint, displayName, storageRef, nowMs),
      ),
    markMilestone: (ledger, checkpoint, label, nowMs) =>
      command("markMilestone", (e) =>
        e.markMilestone(
          ledger,
          checkpoint,
          checked(s.MilestoneLabel, label, "markMilestone") ?? undefined,
          nowMs,
        ),
      ),
    relink: (ledger, node, storageRef, file, nowMs) =>
      command("relink", (e) => e.relink(ledger, node, storageRef, file, nowMs)),
  };
}

const CRASHED =
  "Something went wrong inside Lineage. Nothing was changed. Close and reopen the panel.";
