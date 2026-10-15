// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Development only: the Storage interface over the dev server's local folder
// (dev/local-folder-plugin.ts). Versions are the ledger's length, as in the
// harness's LocalFolderStorage.

import type { FileKind, FileLocation, LedgerVersion, Storage, StoredFile } from "@lineage/shell";
import {
  BadBlob,
  FileExists,
  NotFound,
  StorageConflict,
  StorageError,
  checkFamily,
  checkFileName,
  checkHash,
  folderFor,
  sha256Hex,
} from "@lineage/shell";

const BASE = "/__lineage-dev";

async function call(path: string, init?: RequestInit): Promise<Response> {
  try {
    return await fetch(`${BASE}${path}`, init);
  } catch (error) {
    throw new StorageError(
      "Lineage can't reach the development server. Is `pnpm dev` running?",
      String(error),
    );
  }
}

function failed(what: string, response: Response): StorageError {
  return new StorageError(
    "The development storage couldn't do that.",
    `${what}: ${String(response.status)}`,
  );
}

const bytes = async (response: Response) => new Uint8Array(await response.arrayBuffer());
const enc = encodeURIComponent;

export class DevFolderStorage implements Storage {
  readonly label = "this computer (development)";

  refFor(kind: FileKind, name: string): string {
    return `${folderFor(kind)}/${checkFileName(name)}`;
  }

  async readFile(ref: string): Promise<Uint8Array> {
    const response = await call(`/file?ref=${enc(ref)}`);
    if (response.status === 404) {
      throw new NotFound(ref);
    }
    if (!response.ok) {
      throw failed("readFile", response);
    }
    return bytes(response);
  }

  async writeFile(
    kind: FileKind,
    name: string,
    data: Uint8Array,
    replace = false,
  ): Promise<StoredFile> {
    const clean = checkFileName(name);
    const response = await call(`/files/${kind}/${enc(clean)}${replace ? "?replace=1" : ""}`, {
      method: "PUT",
      body: data as Uint8Array<ArrayBuffer>,
    });
    if (response.status === 409) {
      throw new FileExists(clean);
    }
    if (!response.ok) {
      throw failed("writeFile", response);
    }
    return (await response.json()) as StoredFile;
  }

  async fileNames(kind: FileKind): Promise<string[]> {
    const response = await call(`/files/${kind}`);
    if (!response.ok) {
      throw failed("fileNames", response);
    }
    return (await response.json()) as string[];
  }

  async fileLocation(ref: string): Promise<FileLocation> {
    const response = await call(`/location?ref=${enc(ref)}`);
    if (response.status === 404) {
      throw new NotFound(ref);
    }
    if (!response.ok) {
      throw failed("fileLocation", response);
    }
    return (await response.json()) as FileLocation;
  }

  async readLedger(family: string): Promise<{ bytes: Uint8Array; version: LedgerVersion }> {
    const response = await call(`/ledger/${checkFamily(family)}`);
    if (!response.ok) {
      throw failed("readLedger", response);
    }
    return {
      bytes: await bytes(response),
      version: response.headers.get("X-Ledger-Version") ?? "0",
    };
  }

  async appendLedger(
    family: string,
    expected: LedgerVersion,
    lines: Uint8Array,
  ): Promise<LedgerVersion> {
    const response = await call(`/ledger/${checkFamily(family)}?expected=${enc(expected)}`, {
      method: "POST",
      body: lines as Uint8Array<ArrayBuffer>,
    });
    if (response.status === 409) {
      throw new StorageConflict();
    }
    if (!response.ok) {
      throw failed("appendLedger", response);
    }
    return ((await response.json()) as { version: string }).version;
  }

  async putBlob(family: string, hash: string, data: Uint8Array): Promise<void> {
    if ((await sha256Hex(data)) !== checkHash(hash)) {
      throw new BadBlob(hash);
    }
    const response = await call(`/blob/${checkFamily(family)}/${hash}`, {
      method: "PUT",
      body: data as Uint8Array<ArrayBuffer>,
    });
    if (response.status === 400) {
      throw new BadBlob(hash);
    }
    if (!response.ok) {
      throw failed("putBlob", response);
    }
  }

  async getBlob(family: string, hash: string): Promise<Uint8Array | undefined> {
    const response = await call(`/blob/${checkFamily(family)}/${checkHash(hash)}`);
    if (response.status === 404) {
      return undefined;
    }
    if (!response.ok) {
      throw failed("getBlob", response);
    }
    return bytes(response);
  }

  async families(): Promise<string[]> {
    const response = await call("/families");
    if (!response.ok) {
      throw failed("families", response);
    }
    return (await response.json()) as string[];
  }
}

/** Sends a timing or an error kind to the dev server's log (development only). */
export function devLog(entry: { name: string; ms?: number; kind?: string }): void {
  void fetch(`${BASE}/log`, { method: "POST", body: JSON.stringify(entry) }).catch(() => undefined);
}
