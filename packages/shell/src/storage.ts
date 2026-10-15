// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Where a family's history and the person's files live. This mirrors the
// harness's `Storage` trait (crates/harness/src/storage.rs) method for method, so
// the scenarios proved there hold here (decision 0001):
//
//   <files>/<name>.docx
//   Lineage/<familyId>/ledger.jsonl
//   Lineage/<familyId>/blobs/<sha256>
//
// Appends are optimistic: the caller passes the ledger version it read, and the
// append fails with StorageConflict if the ledger has moved on. A version is
// opaque: the harness uses the ledger's length, OneDrive uses its eTag.
//
// Two additions the panel needs and the harness doesn't: files are written into a
// folder by kind (tailored versions or sent copies), and `fileLocation` says how
// to open one.

/** A ledger version, as returned by `readLedger` and `appendLedger`. */
export type LedgerVersion = string;

/** Which folder a file goes in. */
export type FileKind = "version" | "sent";

/** A file Lineage saved. */
export interface StoredFile {
  /** What the ledger records as the file's storage ref. */
  readonly ref: string;
  readonly name: string;
}

/** How to open a stored file: a web address, or a local path (development only). */
export type FileLocation = { readonly webUrl: string } | { readonly path: string };

export interface Storage {
  /** Shown in the panel: "OneDrive", or the development folder. */
  readonly label: string;
  /** The storage ref a file of this kind and name will have, before it is written. */
  refFor(kind: FileKind, name: string): string;
  readFile(ref: string): Promise<Uint8Array>;
  /** Writes a file. Fails with FileExists unless `replace` is set. */
  writeFile(
    kind: FileKind,
    name: string,
    bytes: Uint8Array,
    replace?: boolean,
  ): Promise<StoredFile>;
  fileNames(kind: FileKind): Promise<string[]>;
  fileLocation(ref: string): Promise<FileLocation>;
  /** The ledger and its version. A family with no ledger yet has empty bytes. */
  readLedger(family: string): Promise<{ bytes: Uint8Array; version: LedgerVersion }>;
  /** Appends lines if the ledger is still at `expected`; returns the new version. */
  appendLedger(family: string, expected: LedgerVersion, lines: Uint8Array): Promise<LedgerVersion>;
  /** Stores a blob. Storing the same blob twice is fine. */
  putBlob(family: string, hash: string, bytes: Uint8Array): Promise<void>;
  getBlob(family: string, hash: string): Promise<Uint8Array | undefined>;
  families(): Promise<string[]>;
}

/** A storage failure, with the words the panel shows. */
export class StorageError extends Error {
  override name = "StorageError";

  constructor(
    readonly userMessage: string,
    detail: string,
  ) {
    super(detail);
  }
}

/** The ledger changed since it was read. */
export class StorageConflict extends StorageError {
  override name = "StorageConflict";

  constructor(detail = "the ledger changed since it was read") {
    super("Your history changed in another window. Try again.", detail);
  }
}

/** A blob's bytes do not match its name. */
export class BadBlob extends StorageError {
  override name = "BadBlob";

  constructor(hash: string) {
    super(
      "Lineage's history for this file is damaged. Nothing was changed.",
      `blob ${hash} does not match its bytes`,
    );
  }
}

export class NotFound extends StorageError {
  override name = "NotFound";

  constructor(what: string) {
    super(
      "Lineage couldn't find that file. It may have been moved or deleted.",
      `${what} not found`,
    );
  }
}

export class FileExists extends StorageError {
  override name = "FileExists";

  constructor(name: string) {
    super(`There's already a file called '${name}'. Choose another name.`, `${name} exists`);
  }
}

/** Lowercase hex SHA-256, with WebCrypto (browsers and Node). */
export async function sha256Hex(bytes: Uint8Array): Promise<string> {
  const digest = new Uint8Array(
    await crypto.subtle.digest("SHA-256", bytes as Uint8Array<ArrayBuffer>),
  );
  return Array.from(digest, (b) => b.toString(16).padStart(2, "0")).join("");
}

const ID = /^[0-9a-f]{32}$/;
const HASH = /^[0-9a-f]{64}$/;

/** Checks a family id before it names a folder. */
export function checkFamily(family: string): string {
  if (!ID.test(family)) {
    throw new StorageError(
      "Lineage's history for this file is damaged.",
      `not a family id: ${family}`,
    );
  }
  return family;
}

/** Checks a blob hash before it names a file. */
export function checkHash(hash: string): string {
  if (!HASH.test(hash)) {
    throw new StorageError(
      "Lineage's history for this file is damaged.",
      `not a blob hash: ${hash}`,
    );
  }
  return hash;
}

/**
 * Checks a file name a person typed: one plain name, no folders, nothing
 * OneDrive or Windows refuses.
 */
export function checkFileName(name: string): string {
  const trimmed = name.trim();
  if (
    trimmed.length === 0 ||
    trimmed.length > 200 ||
    /["*:<>?/\\|]/.test(trimmed) ||
    trimmed.split("").some((c) => c.charCodeAt(0) < 32) ||
    trimmed.endsWith(".") ||
    trimmed === "." ||
    trimmed === ".."
  ) {
    throw new StorageError(
      "File names can't be empty or use any of these: \" * : < > ? / \\ |",
      `bad file name: ${JSON.stringify(name)}`,
    );
  }
  return trimmed;
}

const FOLDERS: Record<FileKind, string> = { version: "Tailored versions", sent: "Sent copies" };

/** The folder each kind of file goes in, the same for every adapter. */
export function folderFor(kind: FileKind): string {
  return FOLDERS[kind];
}

function concat(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length);
  out.set(a, 0);
  out.set(b, a.length);
  return out;
}

/** Everything in memory, like the harness's MemoryStorage. For tests. */
export class MemoryStorage implements Storage {
  readonly label = "memory";
  readonly files = new Map<string, Uint8Array>();
  readonly ledgers = new Map<string, Uint8Array>();
  readonly blobs = new Map<string, Uint8Array>();
  /** Called before every append, so tests can sneak another device's append in. */
  beforeAppend: ((family: string) => Promise<void> | void) | undefined;

  refFor(kind: FileKind, name: string): string {
    return `${folderFor(kind)}/${checkFileName(name)}`;
  }

  async readFile(ref: string): Promise<Uint8Array> {
    await Promise.resolve();
    const bytes = this.files.get(ref);
    if (!bytes) {
      throw new NotFound(ref);
    }
    return bytes.slice();
  }

  async writeFile(
    kind: FileKind,
    name: string,
    bytes: Uint8Array,
    replace = false,
  ): Promise<StoredFile> {
    await Promise.resolve();
    const ref = this.refFor(kind, name);
    if (!replace && this.files.has(ref)) {
      throw new FileExists(name);
    }
    this.files.set(ref, bytes.slice());
    return { ref, name: checkFileName(name) };
  }

  async fileNames(kind: FileKind): Promise<string[]> {
    await Promise.resolve();
    const prefix = `${folderFor(kind)}/`;
    return [...this.files.keys()]
      .filter((k) => k.startsWith(prefix))
      .map((k) => k.slice(prefix.length))
      .sort();
  }

  async fileLocation(ref: string): Promise<FileLocation> {
    await Promise.resolve();
    if (!this.files.has(ref)) {
      throw new NotFound(ref);
    }
    return { path: ref };
  }

  async readLedger(family: string): Promise<{ bytes: Uint8Array; version: LedgerVersion }> {
    await Promise.resolve();
    const bytes = this.ledgers.get(checkFamily(family)) ?? new Uint8Array();
    return { bytes: bytes.slice(), version: String(bytes.length) };
  }

  async appendLedger(
    family: string,
    expected: LedgerVersion,
    lines: Uint8Array,
  ): Promise<LedgerVersion> {
    await this.beforeAppend?.(family);
    const current = this.ledgers.get(checkFamily(family)) ?? new Uint8Array();
    if (String(current.length) !== expected) {
      throw new StorageConflict();
    }
    const next = concat(current, lines);
    this.ledgers.set(family, next);
    return String(next.length);
  }

  async putBlob(family: string, hash: string, bytes: Uint8Array): Promise<void> {
    if ((await sha256Hex(bytes)) !== checkHash(hash)) {
      throw new BadBlob(hash);
    }
    const key = `${checkFamily(family)}/${hash}`;
    if (!this.blobs.has(key)) {
      this.blobs.set(key, bytes.slice());
    }
  }

  async getBlob(family: string, hash: string): Promise<Uint8Array | undefined> {
    await Promise.resolve();
    return this.blobs.get(`${checkFamily(family)}/${checkHash(hash)}`)?.slice();
  }

  async families(): Promise<string[]> {
    await Promise.resolve();
    return [...this.ledgers.keys()].sort();
  }
}
