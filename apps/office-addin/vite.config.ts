// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import type { Plugin } from "vite";
import { localFolderStorage } from "@lineage/storage-local/plugin";

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

/**
 * The page's Content Security Policy. Scripts only from here and the Office.js
 * CDN; wasm compiled without eval ('wasm-unsafe-eval'); network only to Graph,
 * Microsoft sign-in and OneDrive's download and upload hosts.
 * https://learn.microsoft.com/office/dev/add-ins/concepts/privacy-and-security
 */
function contentSecurityPolicy(dev: boolean): Plugin {
  const policy = [
    "default-src 'self'",
    "script-src 'self' 'wasm-unsafe-eval' https://officeapis.public.onecdn.static.microsoft https://appsforoffice.microsoft.com",
    [
      "connect-src 'self'",
      "https://graph.microsoft.com",
      "https://login.microsoftonline.com",
      "https://*.1drv.com",
      "https://*.sharepoint.com",
      "https://officeapis.public.onecdn.static.microsoft",
      ...(dev ? ["wss://localhost:3000"] : []),
    ].join(" "),
    "style-src 'self' 'unsafe-inline'",
    "font-src 'self'",
    "img-src 'self' data:",
    "frame-src https://*.microsoft.com https://*.office.com https://*.officeapps.live.com https://login.microsoftonline.com",
    "object-src 'none'",
    "base-uri 'self'",
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

export default defineConfig(({ command }) => ({
  root: here,
  publicDir: "public",
  server: {
    port: 3000,
    strictPort: true,
    ...(command === "serve" ? { https: devCertificate() } : {}),
  },
  preview: { port: 3000, strictPort: true },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    rollupOptions: { input: { taskpane: path.join(here, "taskpane.html") } },
  },
  plugins: [
    contentSecurityPolicy(command === "serve"),
    // Development only: history in a local folder, shared with My documents.
    localFolderStorage(process.env["LINEAGE_DEV_STORAGE"] ?? path.join(here, ".dev-storage"), [
      "https://localhost:3000",
    ]),
  ],
}));
