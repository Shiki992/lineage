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
  checkWritten(expected: Uint8Array, actual: Uint8Array): s.Written;
}

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
      throw new EngineError(e.kind, e.userMessage, e.detail, e.hashes ?? []);
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
    checkWritten: (expected, actual) =>
      view("checkWritten", s.Written, (e) => e.checkWritten(expected, actual)),
  };
}

const CRASHED =
  "Something went wrong inside Lineage. Nothing was changed. Close and reopen the panel.";
