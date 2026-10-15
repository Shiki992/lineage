// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Builds crates/wasm for wasm32 and generates the JavaScript glue in
// packages/shell/wasm-pkg. The wasm-bindgen CLI must match the wasm-bindgen crate
// version in Cargo.lock exactly, so this script checks that first.
// Guide: https://wasm-bindgen.github.io/wasm-bindgen/reference/cli.html

import { execFileSync } from "node:child_process";
import { readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const root = fileURLToPath(new URL("..", import.meta.url));
const outDir = join(root, "packages", "shell", "wasm-pkg");
// The `wasm` profile in Cargo.toml: release with the smallest code (decision 0010).
const wasmFile = join(root, "target", "wasm32-unknown-unknown", "wasm", "lineage_wasm.wasm");

function lockedWasmBindgenVersion() {
  const lock = readFileSync(join(root, "Cargo.lock"), "utf8");
  const match = /\[\[package\]\]\r?\nname = "wasm-bindgen"\r?\nversion = "([^"]+)"/.exec(lock);
  if (!match) {
    throw new Error("wasm-bindgen not found in Cargo.lock");
  }
  return match[1];
}

function installedCliVersion() {
  try {
    const output = execFileSync("wasm-bindgen", ["--version"], { encoding: "utf8" });
    return output.trim().split(/\s+/)[1];
  } catch {
    return undefined;
  }
}

function run(command, args) {
  execFileSync(command, args, { cwd: root, stdio: "inherit" });
}

const wanted = lockedWasmBindgenVersion();
const installed = installedCliVersion();
if (installed !== wanted) {
  console.error(
    installed === undefined
      ? "wasm-bindgen CLI not found."
      : `wasm-bindgen CLI is ${installed}, but Cargo.lock needs ${wanted}.`,
  );
  console.error(`Install it with: cargo install --locked wasm-bindgen-cli --version ${wanted}`);
  process.exit(1);
}

run("cargo", [
  "build",
  "--package",
  "lineage-wasm",
  "--target",
  "wasm32-unknown-unknown",
  "--profile",
  "wasm",
]);
// Function names are only for debugging and are a third of the file.
run("wasm-bindgen", [
  "--target",
  "web",
  "--remove-name-section",
  "--remove-producers-section",
  "--out-dir",
  outDir,
  "--out-name",
  "lineage_wasm",
  wasmFile,
]);

const size = statSync(join(outDir, "lineage_wasm_bg.wasm")).size;
console.log(`wasm-pkg ready: lineage_wasm_bg.wasm is ${size} bytes`);
