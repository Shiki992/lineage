// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// What the task pane (`Lineage`) and "My documents" (`Library`) both do with a
// family's history: fetch the blobs the engine asks for, store the ones it made,
// and keep the library index up to date after a ledger is written.

import type { CommandResult, Engine } from "./engine.js";
import { EngineError } from "./engine.js";
import { LineageError } from "./errors.js";
import type { LedgerVersion, Storage } from "./storage.js";
import { StorageConflict } from "./storage.js";

/** How many times a call fetches missing blobs before giving up. */
export const FETCH_ROUNDS = 6;
/** Blobs fetched or stored at once. */
const AT_ONCE = 6;

export class History {
  constructor(
    readonly engine: Engine,
    readonly storage: Storage,
  ) {}

  async fetchBlobs(family: string, hashes: readonly string[]): Promise<void> {
    const wanted = hashes.filter((h) => !this.engine.hasBlob(h));
    for (let i = 0; i < wanted.length; i += AT_ONCE) {
      const batch = wanted.slice(i, i + AT_ONCE);
      const fetched = await Promise.all(batch.map((h) => this.storage.getBlob(family, h)));
      batch.forEach((hash, j) => {
        const bytes = fetched[j];
        if (!bytes) {
          throw new LineageError(
            "Lineage's history for this file is incomplete. Nothing was changed.",
            `blob ${hash} is not in storage`,
          );
        }
        this.engine.addBlob(hash, bytes);
      });
    }
  }

  /**
   * Runs an engine call, fetching whatever blobs it reports missing: from the
   * family the engine names, else from `family`.
   */
  async withBlobs<T>(family: string, run: () => T, rounds = FETCH_ROUNDS): Promise<T> {
    for (let round = 0; ; round++) {
      try {
        return run();
      } catch (error) {
        if (error instanceof EngineError && error.kind === "missingBlobs" && round < rounds) {
          await this.fetchBlobs(error.family ?? family, error.hashes);
          continue;
        }
        throw error;
      }
    }
  }

  async storeBlobs(family: string, result: CommandResult): Promise<void> {
    const entries = [...result.blobs];
    for (let i = 0; i < entries.length; i += AT_ONCE) {
      await Promise.all(
        entries
          .slice(i, i + AT_ONCE)
          .map(([hash, bytes]) => this.storage.putBlob(family, hash, bytes)),
      );
    }
  }

  /**
   * Brings the library index up to date after `family`'s ledger was written
   * (decision 0015). It is only a copy, so a failure is left for the next
   * "My documents" to repair, and never fails the command that wrote.
   */
  async updateIndex(family: string, version: LedgerVersion, ledger: Uint8Array): Promise<boolean> {
    for (let attempt = 0; attempt < 2; attempt++) {
      try {
        const current = await this.storage.readIndex();
        const next = this.engine.updateIndex(current?.bytes, family, version, ledger);
        await this.storage.writeIndex(next, current?.version);
        return true;
      } catch (error) {
        if (!(error instanceof StorageConflict)) {
          return false;
        }
      }
    }
    return false;
  }
}

export function concat(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length);
  out.set(a, 0);
  out.set(b, a.length);
  return out;
}
