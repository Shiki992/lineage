// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import path from "node:path";
import { fileURLToPath } from "node:url";
import { hosting, myDocumentsPolicy, myDocumentsSite, releaseFrom } from "@lineage/hosting";
import { localFolderStorage } from "@lineage/storage-local/plugin";
import { defineConfig } from "vite";
import type { Plugin } from "vite";

const here = path.dirname(fileURLToPath(import.meta.url));
/** 3001; the Playwright runs start one server per test account on other ports. */
const PORT = Number(process.env["LINEAGE_APP_PORT"] ?? 3001);

/** MSAL's bridge page must not be cached (its address can hold a code). */
function noStoreBridge(): Plugin {
  return {
    name: "lineage-no-store-bridge",
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (req.url?.startsWith("/redirect.html")) {
          res.setHeader("Cache-Control", "no-store");
        }
        next();
      });
    },
  };
}

export default defineConfig(({ command }) => ({
  root: here,
  publicDir: "public",
  server: { port: PORT, strictPort: true },
  preview: { port: PORT, strictPort: true },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    rollupOptions: {
      input: {
        index: path.join(here, "index.html"),
        redirect: path.join(here, "redirect.html"),
        // The pages the Marketplace listing and the manifest link to.
        privacy: path.join(here, "privacy.html"),
        terms: path.join(here, "terms.html"),
        support: path.join(here, "support.html"),
      },
    },
  },
  plugins: [
    // The pages' security policy, and in a build the hosting headers and
    // the checks on what ships (decision 0031).
    hosting({
      site: "myDocuments",
      policy: myDocumentsPolicy(command === "serve" ? PORT : undefined),
      config: myDocumentsSite(myDocumentsPolicy()),
      release: releaseFrom(process.env),
    }),
    noStoreBridge(),
    // Development only: the same local folder the Word pane uses.
    localFolderStorage(
      process.env["LINEAGE_DEV_STORAGE"] ?? path.join(here, "../office-addin/.dev-storage"),
      [`http://localhost:${String(PORT)}`],
    ),
  ],
}));
