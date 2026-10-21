// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { hosting, releaseFrom, taskPanePolicy, taskPaneSite } from "@lineage/hosting";
import { localFolderStorage } from "@lineage/storage-local/plugin";
import { defineConfig } from "vite";

const here = path.dirname(fileURLToPath(import.meta.url));

/** The certificate scripts/dev-cert.ps1 makes, from the user profile. */
function devCertificate(): { pfx: Buffer; passphrase: string } {
  const dir = path.join(os.homedir(), ".lineage-dev-certs");
  const pfx = path.join(dir, "localhost.pfx");
  const pass = path.join(dir, "localhost.pass");
  if (!fs.existsSync(pfx) || !fs.existsSync(pass)) {
    throw new Error(
      "No development certificate. Word loads the task pane only over HTTPS. Run, once:\n" +
        "  powershell -ExecutionPolicy Bypass -File scripts/dev-cert.ps1",
    );
  }
  return { pfx: fs.readFileSync(pfx), passphrase: fs.readFileSync(pass, "utf8").trim() };
}

export default defineConfig(({ command }) => ({
  root: here,
  publicDir: "public",
  server: {
    port: 3000,
    strictPort: true,
    ...(command === "serve" ? { https: devCertificate() } : {}),
  },
  preview: { port: 3000, strictPort: true },
  // The Excel pane's engine worker imports modules (main.tsx, decision 0025).
  worker: { format: "es" },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    rollupOptions: { input: { taskpane: path.join(here, "taskpane.html") } },
  },
  plugins: [
    // The page's security policy, and in a build the hosting headers, the
    // release manifest and the checks on what ships (decision 0031).
    hosting({
      site: "taskPane",
      policy: taskPanePolicy(command === "serve"),
      config: taskPaneSite(taskPanePolicy(false)),
      release: releaseFrom(process.env),
      manifestTemplate: fs.readFileSync(path.join(here, "manifest.template.xml"), "utf8"),
    }),
    // Development only: history in a local folder, shared with My documents.
    localFolderStorage(process.env["LINEAGE_DEV_STORAGE"] ?? path.join(here, ".dev-storage"), [
      "https://localhost:3000",
    ]),
  ],
}));
