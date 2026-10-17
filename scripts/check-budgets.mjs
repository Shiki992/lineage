// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Fails when a built app is over its budgets (decisions 0010 and 0015): the
// engine's wasm at most 4 MB raw and gzipped, and each app's
// JavaScript at most 250 KB gzipped. Run after building the task pane and My
// documents (`pnpm budgets` does both).

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

// The wasm limits were 1.5 MB raw and 420 KB gzipped until the deck engine
// (Phase 4) took the engine to 467 KB gzipped; the maintainer chose to raise
// them to 4 MB rather than split the engine (decision 0010, 2026-10-17).
const BUDGETS = { wasmRaw: 4_000_000, wasmGzip: 4_000_000, jsGzip: 250_000 };
const APPS = { "task pane": "office-addin", "My documents": "family-app" };

const gzip = (bytes) => gzipSync(bytes, { level: 9 }).length;

let over = false;
for (const [label, app] of Object.entries(APPS)) {
  const assets = fileURLToPath(new URL(`../apps/${app}/dist/assets/`, import.meta.url));
  let files;
  try {
    files = readdirSync(assets);
  } catch {
    console.error(`No build of ${label}. Run \`pnpm --filter @lineage/${app} build\` first.`);
    process.exit(1);
  }
  const wasm = files.filter((f) => f.endsWith(".wasm")).map((f) => readFileSync(join(assets, f)));
  const js = files.filter((f) => f.endsWith(".js")).map((f) => readFileSync(join(assets, f)));
  if (wasm.length !== 1) {
    console.error(`Expected one .wasm file in the ${label} build, found ${wasm.length}.`);
    process.exit(1);
  }
  const measured = {
    wasmRaw: wasm[0].length,
    wasmGzip: gzip(wasm[0]),
    jsGzip: js.reduce((n, b) => n + gzip(b), 0),
  };
  console.log(label);
  for (const [name, limit] of Object.entries(BUDGETS)) {
    const value = measured[name];
    const ok = value <= limit;
    over ||= !ok;
    console.log(
      `  ${ok ? "ok  " : "OVER"} ${name.padEnd(9)} ${String(value).padStart(9)} bytes (budget ${limit})`,
    );
  }
}
process.exit(over ? 1 : 0);
