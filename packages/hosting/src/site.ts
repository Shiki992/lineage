// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Azure Static Web Apps configuration for the two sites (decision 0031). The
// file goes at the root of each build, routes are matched in order and stop
// at the first match, a route's headers override the global ones, and the
// file may be at most 20 KB.
// https://learn.microsoft.com/azure/static-web-apps/configuration

import type { Policy } from "./policy.ts";
import { MY_DOCUMENTS_HEADER_ONLY, render } from "./policy.ts";

export const CONFIG_FILE = "staticwebapp.config.json";
export const CONFIG_LIMIT = 20 * 1024;

export interface Route {
  readonly route: string;
  readonly headers: Readonly<Record<string, string>>;
}

export interface SiteConfig {
  readonly routes: readonly Route[];
  readonly globalHeaders: Readonly<Record<string, string>>;
  readonly mimeTypes: Readonly<Record<string, string>>;
  readonly responseOverrides: Readonly<Record<string, { readonly rewrite: string }>>;
}

/** Pages: always asked for again, so a release reaches people at once. */
const PAGE = { "Cache-Control": "no-cache" };
/** Vite's hashed files never change under their name. */
const HASHED = { "Cache-Control": "public, max-age=31536000, immutable" };
/**
 * Images Office shows in the ribbon and the add-in list must be cacheable in
 * production: no no-cache or no-store.
 * https://learn.microsoft.com/office/dev/add-ins/concepts/requirements-for-running-office-add-ins
 */
const ICON = { "Cache-Control": "public, max-age=86400" };

/**
 * Sent with everything. No `includeSubDomains`: the sites may live under a
 * domain whose other hosts aren't ours to bind to HTTPS.
 */
function common(policy: Policy): Record<string, string> {
  return {
    "Content-Security-Policy": render(policy),
    "Strict-Transport-Security": "max-age=63072000",
    "X-Content-Type-Options": "nosniff",
    "Referrer-Policy": "no-referrer",
    "Permissions-Policy": "camera=(), microphone=(), geolocation=(), payment=(), usb=()",
  };
}

const MIME = { ".wasm": "application/wasm", ".xml": "application/xml" };

/**
 * The task pane: the page, its manifest, the ribbon icons and the build. Its
 * index.html only says where Lineage runs; Static Web Apps won't deploy a site
 * without one.
 */
export function taskPaneSite(policy: Policy): SiteConfig {
  return {
    routes: [
      { route: "/taskpane.html", headers: PAGE },
      // Matches "/" too: index.html is the folder's default file.
      { route: "/index.html", headers: PAGE },
      { route: "/manifest.xml", headers: PAGE },
      // Before /assets/*: the icons keep their names across releases.
      { route: "/assets/icon-*", headers: ICON },
      { route: "/assets/*", headers: HASHED },
    ],
    globalHeaders: common(policy),
    mimeTypes: MIME,
    responseOverrides: { "404": { rewrite: "/404.html" } },
  };
}

/**
 * My documents and its pages. MSAL's redirect bridge must not be cached (its
 * address can hold a code) and must not send Cross-Origin-Opener-Policy, which
 * Static Web Apps doesn't add.
 * https://github.com/AzureAD/microsoft-authentication-library-for-js/blob/dev/lib/msal-browser/docs/redirect-bridge.md
 */
export function myDocumentsSite(policy: Policy): SiteConfig {
  return {
    routes: [
      { route: "/redirect.html", headers: { "Cache-Control": "no-store" } },
      // Matches "/" too: index.html is the folder's default file.
      { route: "/index.html", headers: PAGE },
      { route: "/*.html", headers: PAGE },
      { route: "/assets/*", headers: HASHED },
      { route: "/icon-32.png", headers: ICON },
    ],
    globalHeaders: {
      ...common([...policy, ...MY_DOCUMENTS_HEADER_ONLY]),
      "X-Frame-Options": "DENY",
    },
    mimeTypes: MIME,
    responseOverrides: { "404": { rewrite: "/404.html" } },
  };
}

/** The file as it is written into a build. */
export function serialise(config: SiteConfig): string {
  const text = `${JSON.stringify(config, null, 2)}\n`;
  if (Buffer.byteLength(text) > CONFIG_LIMIT) {
    throw new Error(`${CONFIG_FILE} is over Static Web Apps' 20 KB limit`);
  }
  return text;
}
