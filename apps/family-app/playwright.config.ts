// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// End-to-end runs of My documents in a real browser, against test accounts
// made by `cargo xtask accounts` (invented content only) and served by the dev
// server's local folder. One server per account. No browser is downloaded:
// the installed Microsoft Edge here, the runner's Chrome in CI.

import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "@playwright/test";

const here = path.dirname(fileURLToPath(import.meta.url));
export const ACCOUNTS = path.join(here, "e2e", ".accounts");
export const PORTS = {
  "three-families": 3101,
  damaged: 3102,
  large: 3103,
  workbooks: 3104,
} as const;

export default defineConfig({
  testDir: "e2e",
  globalSetup: "./e2e/global-setup.ts",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: process.env["CI"] ? "github" : "list",
  use: {
    channel: process.env["CI"] ? "chrome" : "msedge",
    acceptDownloads: true,
    trace: "retain-on-failure",
  },
  webServer: Object.entries(PORTS).map(([account, port]) => ({
    command: "npx vite",
    cwd: here,
    port,
    reuseExistingServer: false,
    timeout: 120_000,
    env: {
      LINEAGE_APP_PORT: String(port),
      LINEAGE_DEV_STORAGE: path.join(ACCOUNTS, account),
    },
  })),
});
