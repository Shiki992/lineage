// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Fails when the built task pane is over its budgets (decision 0010):
// the engine's wasm at most 1.5 MB raw and 420 KB gzipped, and all JavaScript at
// most 250 KB gzipped. Run after `pnpm --filter @lineage/office-addin build`.

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { gzipSync } from "node:zlib";

const BUDGETS = { wasmRaw: 1_500_000, wasmGzip: 420_000, jsGzip: 250_000 };
const assets = fileURLToPath(new URL("../apps/office-addin/dist/assets/", import.meta.url));

let files;
try {
  files = readdirSync(assets);
} catch {
  console.error("No build found. Run `pnpm --filter @lineage/office-addin build` first.");
  process.exit(1);
}

const gzip = (bytes) => gzipSync(bytes, { level: 9 }).length;
const wasm = files.filter((f) => f.endsWith(".wasm")).map((f) => readFileSync(join(assets, f)));
const js = files.filter((f) => f.endsWith(".js")).map((f) => readFileSync(join(assets, f)));
if (wasm.length !== 1) {
  console.error(`Expected one .wasm file in the build, found ${wasm.length}.`);
  process.exit(1);
}

const measured = {
  wasmRaw: wasm[0].length,
  wasmGzip: gzip(wasm[0]),
  jsGzip: js.reduce((n, b) => n + gzip(b), 0),
};

let over = false;
for (const [name, limit] of Object.entries(BUDGETS)) {
  const value = measured[name];
  const ok = value <= limit;
  over ||= !ok;
  console.log(
    `${ok ? "ok  " : "OVER"} ${name.padEnd(9)} ${String(value).padStart(9)} bytes (budget ${limit})`,
  );
}
process.exit(over ? 1 : 0);
