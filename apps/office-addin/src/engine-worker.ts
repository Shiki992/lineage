// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The engine in a Web Worker, for the Excel pane: comparing a large workbook
// takes seconds, and Office restarts a pane that stops responding for 5
// (decision 0025). The pane talks to it through `connectEngine`.

import { loadEngineAsync, serveEngine } from "@lineage/shell";
import wasmUrl from "@lineage/shell/wasm?url";

serveEngine(
  loadEngineAsync(wasmUrl).then(({ engine }) => engine),
  self,
);
