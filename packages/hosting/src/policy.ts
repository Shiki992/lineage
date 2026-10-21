// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Each site's Content Security Policy, in one place: the dev server and the
// built pages carry it as a meta tag, and the hosted sites send it as a header
// too (site.ts), which is the only way `frame-ancestors` takes effect.
// https://learn.microsoft.com/office/dev/add-ins/concepts/privacy-and-security
// https://developer.mozilla.org/docs/Web/HTTP/Headers/Content-Security-Policy

/** A policy as its directives, in order. */
export type Policy = readonly string[];

export function render(policy: Policy): string {
  return policy.join("; ");
}

/**
 * Microsoft sign-in, Graph, and OneDrive's download and upload hosts. A
 * download URL points at `*.sharepoint.com` for work accounts and at
 * `*.1drv.com` or `*.microsoftpersonalcontent.com` for personal ones.
 */
const MICROSOFT = [
  "https://graph.microsoft.com",
  "https://login.microsoftonline.com",
  "https://*.1drv.com",
  "https://*.microsoftpersonalcontent.com",
  "https://*.sharepoint.com",
];

/** Office.js, which add-ins must load from Microsoft's CDN. */
const OFFICE_JS = [
  "https://officeapis.public.onecdn.static.microsoft",
  "https://appsforoffice.microsoft.com",
];

/**
 * The task pane: scripts only from here and the Office.js CDN, wasm compiled
 * without eval ('wasm-unsafe-eval'), and Office's own frames for sign-in. The
 * engine's module worker is a script from here, and its response carries the
 * same header, so the worker may compile the wasm too.
 *
 * No `frame-ancestors`: Office frames the pane from origins Microsoft doesn't
 * list, and one left out would blank the pane there (decision 0031).
 */
export function taskPanePolicy(dev: boolean): Policy {
  return [
    "default-src 'self'",
    `script-src 'self' 'wasm-unsafe-eval' ${OFFICE_JS.join(" ")}`,
    [
      "connect-src 'self'",
      ...MICROSOFT,
      "https://officeapis.public.onecdn.static.microsoft",
      ...(dev ? ["wss://localhost:3000"] : []),
    ].join(" "),
    "style-src 'self' 'unsafe-inline'",
    "font-src 'self'",
    "img-src 'self' data:",
    "frame-src https://*.microsoft.com https://*.office.com https://*.officeapps.live.com https://login.microsoftonline.com",
    "object-src 'none'",
    "base-uri 'self'",
  ];
}

/**
 * My documents: scripts only from here, wasm compiled without eval, and network
 * only to Microsoft. `devPort` adds the dev server's hot-reload socket.
 */
export function myDocumentsPolicy(devPort?: number): Policy {
  return [
    "default-src 'self'",
    "script-src 'self' 'wasm-unsafe-eval'",
    [
      "connect-src 'self'",
      ...MICROSOFT,
      ...(devPort === undefined ? [] : [`ws://localhost:${String(devPort)}`]),
    ].join(" "),
    "style-src 'self' 'unsafe-inline'",
    "font-src 'self'",
    "img-src 'self' data: blob:",
    "frame-src https://login.microsoftonline.com",
    "object-src 'none'",
    "base-uri 'self'",
    "form-action 'self'",
  ];
}

/**
 * Directives a meta tag can't carry (browsers ignore them there), added to the
 * header only. My documents is never framed.
 */
export const MY_DOCUMENTS_HEADER_ONLY: Policy = ["frame-ancestors 'none'"];
