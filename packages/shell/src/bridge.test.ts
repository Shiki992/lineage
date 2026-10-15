// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Proves the Rust → wasm → TypeScript bridge: a string comes out, and bytes go in
// and come back unchanged.

import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { loadEngine } from "./index.js";

const wasmBytes = readFileSync(new URL("../wasm-pkg/lineage_wasm_bg.wasm", import.meta.url));
const engine = loadEngine(wasmBytes);

function workspaceVersion(): string {
  const manifest = readFileSync(new URL("../../../Cargo.toml", import.meta.url), "utf8");
  const match = /\[workspace\.package\][^[]*?\nversion = "([^"]+)"/.exec(manifest);
  if (!match?.[1]) {
    throw new Error("version not found in [workspace.package]");
  }
  return match[1];
}

function pattern(length: number): Uint8Array {
  const bytes = new Uint8Array(length);
  for (let i = 0; i < length; i++) {
    bytes[i] = (i * 31 + 7) % 256;
  }
  return bytes;
}

describe("wasm bridge", () => {
  it("reports the Rust workspace version", () => {
    expect(engine.version).toBe(workspaceVersion());
  });

  it.each([
    ["empty", 0],
    ["one byte", 1],
    ["every byte value", 256],
    ["one megabyte", 1024 * 1024],
  ])("round-trips %s unchanged", (_label, length) => {
    const input = pattern(length);
    const output = engine.echoBytes(input);
    expect(output).toBeInstanceOf(Uint8Array);
    // Plain comparisons rather than not.toBe / toEqual: Vitest runs a deep equality
    // check over the whole array for those, which takes seconds at a megabyte.
    expect(output === input).toBe(false);
    expect(output.length).toBe(input.length);
    expect(Buffer.from(output).equals(Buffer.from(input))).toBe(true);
  });

  it("can be loaded twice", () => {
    expect(loadEngine(wasmBytes).version).toBe(engine.version);
  });
});
