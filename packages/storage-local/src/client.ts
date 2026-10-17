// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Development and tests only: the Storage interface over the dev server's local
// folder (plugin.ts). Versions are the ledger's length, as in the harness's
// LocalFolderStorage, and the index's version is the start of its hash.

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

const ROUTE = "/__lineage-dev";

function failed(what: string, response: Response): StorageError {
  return new StorageError(
    "The development storage couldn't do that.",
    `${what}: ${String(response.status)}`,
  );
}

const bytes = async (response: Response) => new Uint8Array(await response.arrayBuffer());
const enc = encodeURIComponent;

export class LocalFolderStorage implements Storage {
  readonly label = "this computer (development)";

  /** `base`: where the dev server answers; an absolute URL in Node tests. */
  constructor(private readonly base = ROUTE) {}

  private async call(path: string, init?: RequestInit): Promise<Response> {
    try {
      return await fetch(`${this.base}${path}`, init);
    } catch (error) {
      throw new StorageError(
        "Lineage can't reach the development server. Is `pnpm dev` running?",
        String(error),
      );
    }
  }

  refFor(kind: FileKind, name: string): string {
    return `${folderFor(kind)}/${checkFileName(name)}`;
  }

  async readFile(ref: string): Promise<Uint8Array> {
    const response = await this.call(`/file?ref=${enc(ref)}`);
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
    const response = await this.call(`/files/${kind}/${enc(clean)}${replace ? "?replace=1" : ""}`, {
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
    const response = await this.call(`/files/${kind}`);
    if (!response.ok) {
      throw failed("fileNames", response);
    }
    return (await response.json()) as string[];
  }

  async fileLocation(ref: string): Promise<FileLocation> {
    const response = await this.call(`/location?ref=${enc(ref)}`);
    if (response.status === 404) {
      throw new NotFound(ref);
    }
    if (!response.ok) {
      throw failed("fileLocation", response);
    }
    return (await response.json()) as FileLocation;
  }

  async readLedger(family: string): Promise<{ bytes: Uint8Array; version: LedgerVersion }> {
    const response = await this.call(`/ledger/${checkFamily(family)}`);
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
    const response = await this.call(`/ledger/${checkFamily(family)}?expected=${enc(expected)}`, {
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
    const response = await this.call(`/blob/${checkFamily(family)}/${hash}`, {
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
    const response = await this.call(`/blob/${checkFamily(family)}/${checkHash(hash)}`);
    if (response.status === 404) {
      return undefined;
    }
    if (!response.ok) {
      throw failed("getBlob", response);
    }
    return bytes(response);
  }

  async families(): Promise<string[]> {
    const response = await this.call("/families");
    if (!response.ok) {
      throw failed("families", response);
    }
    return (await response.json()) as string[];
  }

  async readIndex(): Promise<{ bytes: Uint8Array; version: LedgerVersion } | undefined> {
    const response = await this.call("/index");
    if (response.status === 404) {
      return undefined;
    }
    if (!response.ok) {
      throw failed("readIndex", response);
    }
    return { bytes: await bytes(response), version: response.headers.get("X-Index-Version") ?? "" };
  }

  async writeIndex(data: Uint8Array, expected: LedgerVersion | undefined): Promise<LedgerVersion> {
    const query = expected === undefined ? "" : `?expected=${enc(expected)}`;
    const response = await this.call(`/index${query}`, {
      method: "PUT",
      body: data as Uint8Array<ArrayBuffer>,
    });
    if (response.status === 409) {
      throw new StorageConflict("the index changed since it was read");
    }
    if (!response.ok) {
      throw failed("writeIndex", response);
    }
    return ((await response.json()) as { version: string }).version;
  }

  async ledgerVersions(families: readonly string[]): Promise<Map<string, LedgerVersion>> {
    const response = await this.call("/ledger-versions", {
      method: "POST",
      body: JSON.stringify(families.map(checkFamily)),
    });
    if (!response.ok) {
      throw failed("ledgerVersions", response);
    }
    return new Map(Object.entries((await response.json()) as Record<string, string>));
  }
}

/** Sends a timing or an error kind to the dev server's log (development only). */
export function devLog(entry: { name: string; ms?: number; kind?: string }): void {
  void fetch(`${ROUTE}/log`, { method: "POST", body: JSON.stringify(entry) }).catch(
    () => undefined,
  );
}
