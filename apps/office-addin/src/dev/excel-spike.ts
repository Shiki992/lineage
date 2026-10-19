// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Development only, never imported by the pane: the Excel path driven through
// the DevTools port (decision 0008) before the Excel screens exist. Import it
// from the pane's console in `pnpm dev`:
//   const spike = await (await import("/src/dev/excel-spike.ts")).start();
// It runs the real shell, the dev-folder storage and the Excel host on the
// open workbook.

import { Lineage, loadEngineAsync } from "@lineage/shell";
import wasmUrl from "@lineage/shell/wasm?url";
import { LocalFolderStorage } from "@lineage/storage-local";
import { ExcelHost } from "../office/excel-host";

/** For making Excel stop partway, as a failing step would. */
export { PlanStopped } from "@lineage/shell";

export async function start(
  onTiming?: (name: string, ms: number) => void,
): Promise<{ lineage: Lineage; host: ExcelHost }> {
  const { engine } = await loadEngineAsync(wasmUrl);
  const host = new ExcelHost();
  await host.watchEdits();
  const lineage = new Lineage({
    engine,
    storage: new LocalFolderStorage(),
    host,
    ...(onTiming ? { onTiming } : {}),
  });
  return { lineage, host };
}
