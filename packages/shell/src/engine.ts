// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Typed wrapper around the wasm engine. Everything that crosses the boundary is
// checked here, so the rest of the TypeScript code can trust these types.
// TypeScript never parses document XML: it hands bytes to the engine and gets bytes back.

import { echoBytes, engineVersion, initSync } from "../wasm-pkg/lineage_wasm.js";
import type { SyncInitInput } from "../wasm-pkg/lineage_wasm.js";

/** The engine as the TypeScript shell sees it. */
export interface Engine {
  /** Engine version, from the Rust workspace manifest. */
  readonly version: string;
  /** Returns a copy of `bytes`. Exists to prove the bridge; removed once real operations land. */
  echoBytes(bytes: Uint8Array): Uint8Array;
}

/** Thrown when a value coming back from the engine is not the shape we expect. */
export class EngineBoundaryError extends Error {
  override name = "EngineBoundaryError";
}

const SEMVER = /^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$/;

let initialised = false;

/**
 * Loads the engine from the wasm module's bytes, or an already compiled module.
 * The host decides how to get the bytes: Node reads the file, the task pane fetches it.
 */
export function loadEngine(wasm: SyncInitInput): Engine {
  if (!initialised) {
    initSync({ module: wasm });
    initialised = true;
  }

  const version: unknown = engineVersion();
  if (typeof version !== "string" || !SEMVER.test(version)) {
    throw new EngineBoundaryError(`engine returned an invalid version: ${String(version)}`);
  }

  return {
    version,
    echoBytes(bytes: Uint8Array): Uint8Array {
      const result: unknown = echoBytes(bytes);
      if (!(result instanceof Uint8Array)) {
        throw new EngineBoundaryError("engine returned something other than bytes");
      }
      return result;
    },
  };
}
