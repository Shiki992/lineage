// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// What the task pane (`Lineage`) and "My documents" (`Library`) both do with a
// family's history: fetch the blobs the engine asks for, store the ones it made,
// and keep the library index up to date after a ledger is written.

import type { Engine } from "./engine.js";
import { EngineError } from "./engine.js";
import type { AsyncEngine } from "./worker.js";
import { asyncEngine } from "./worker.js";
import { LineageError } from "./errors.js";
import type { LedgerVersion, Storage } from "./storage.js";
import { StorageConflict } from "./storage.js";

/** How many times a call fetches missing blobs before giving up. */
export const FETCH_ROUNDS = 6;
/** Blobs fetched or stored at once. */
const AT_ONCE = 6;

export class History {
  /**
   * Blobs this session has seen in storage, by family: read from it, or stored
   * in it without an error. Blobs are never deleted, so these need not be
   * stored again. A checkpoint of a deck names about 40 parts, nearly all of
   * them unchanged since the last one (found in the PowerPoint run, Phase 4).
   */
  private readonly inStorage = new Map<string, Set<string>>();

  readonly engine: AsyncEngine;

  constructor(
    engine: Engine | AsyncEngine,
    readonly storage: Storage,
  ) {
    this.engine = asyncEngine(engine);
  }

  private seen(family: string): Set<string> {
    let set = this.inStorage.get(family);
    if (!set) {
      set = new Set();
      this.inStorage.set(family, set);
    }
    return set;
  }

  async fetchBlobs(family: string, hashes: readonly string[]): Promise<void> {
    const have = await Promise.all(hashes.map((h) => this.engine.hasBlob(h)));
    const wanted = hashes.filter((_, i) => !have[i]);
    const seen = this.seen(family);
    for (let i = 0; i < wanted.length; i += AT_ONCE) {
      const batch = wanted.slice(i, i + AT_ONCE);
      const fetched = await Promise.all(batch.map((h) => this.storage.getBlob(family, h)));
      for (const [j, hash] of batch.entries()) {
        const bytes = fetched[j];
        if (!bytes) {
          throw new LineageError(
            "Lineage's history for this file is incomplete. Nothing was changed.",
            `blob ${hash} is not in storage`,
          );
        }
        await this.engine.addBlob(hash, bytes);
        seen.add(hash);
      }
    }
  }

  /**
   * Runs an engine call, fetching whatever blobs it reports missing: from the
   * family the engine names, else from `family`.
   */
  async withBlobs<T>(family: string, run: () => T | Promise<T>, rounds = FETCH_ROUNDS): Promise<T> {
    for (let round = 0; ; round++) {
      try {
        return await run();
      } catch (error) {
        if (error instanceof EngineError && error.kind === "missingBlobs" && round < rounds) {
          await this.fetchBlobs(error.family ?? family, error.hashes);
          continue;
        }
        throw error;
      }
    }
  }

  /** Stores the blobs a command made, except those already in storage. */
  async storeBlobs(family: string, blobs: ReadonlyMap<string, Uint8Array>): Promise<void> {
    const seen = this.seen(family);
    const entries = [...blobs].filter(([hash]) => !seen.has(hash));
    for (let i = 0; i < entries.length; i += AT_ONCE) {
      await Promise.all(
        entries.slice(i, i + AT_ONCE).map(async ([hash, bytes]) => {
          await this.storage.putBlob(family, hash, bytes);
          seen.add(hash);
        }),
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
        const next = await this.engine.updateIndex(current?.bytes, family, version, ledger);
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
