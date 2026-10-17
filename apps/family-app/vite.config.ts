// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import path from "node:path";
import { fileURLToPath } from "node:url";
import { localFolderStorage } from "@lineage/storage-local/plugin";
import { defineConfig } from "vite";
import type { Plugin } from "vite";

const here = path.dirname(fileURLToPath(import.meta.url));
/** 3001; the Playwright runs start one server per test account on other ports. */
const PORT = Number(process.env["LINEAGE_APP_PORT"] ?? 3001);

/**
 * The page's Content Security Policy: scripts only from here; wasm compiled
 * without eval; network only to Microsoft sign-in, Graph and OneDrive's
 * download hosts (personal accounts may use *.1drv.com or
 * *.microsoftpersonalcontent.com). Nothing else is ever contacted.
 */
function contentSecurityPolicy(dev: boolean): Plugin {
  const policy = [
    "default-src 'self'",
    "script-src 'self' 'wasm-unsafe-eval'",
    [
      "connect-src 'self'",
      "https://graph.microsoft.com",
      "https://login.microsoftonline.com",
      "https://*.1drv.com",
      "https://*.microsoftpersonalcontent.com",
      "https://*.sharepoint.com",
      ...(dev ? [`ws://localhost:${String(PORT)}`] : []),
    ].join(" "),
    "style-src 'self' 'unsafe-inline'",
    "font-src 'self'",
    "img-src 'self' data: blob:",
    "frame-src https://login.microsoftonline.com",
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
  ].join("; ");
  return {
    name: "lineage-csp",
    transformIndexHtml: () => [
      {
        tag: "meta",
        attrs: { "http-equiv": "Content-Security-Policy", content: policy },
        injectTo: "head-prepend",
      },
    ],
  };
}

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
      },
    },
  },
  plugins: [
    contentSecurityPolicy(command === "serve"),
    noStoreBridge(),
    // Development only: the same local folder the Word pane uses.
    localFolderStorage(
      process.env["LINEAGE_DEV_STORAGE"] ?? path.join(here, "../office-addin/.dev-storage"),
      [`http://localhost:${String(PORT)}`],
    ),
  ],
}));
