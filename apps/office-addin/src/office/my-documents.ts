// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// "My documents" in the footer opens the family app in the person's browser,
// and the sign-in screen's Privacy and Terms open its pages (decision 0031).
// openBrowserWindow (OpenBrowserWindowApi 1.1) opens the default browser from
// Word on Windows and Mac; elsewhere a new tab does the same.
// https://learn.microsoft.com/javascript/api/office/office.ui#office-office-ui-openbrowserwindow-member(1)

/** The pages on My documents' site that the sign-in screen links to. */
export type SitePage = "privacy" | "terms";

/** Where My documents is: VITE_LINEAGE_APP_URL, or the dev server in development. */
export function myDocumentsUrl(): string | undefined {
  const configured: unknown = import.meta.env["VITE_LINEAGE_APP_URL"];
  const url =
    typeof configured === "string" && configured !== ""
      ? configured
      : import.meta.env.DEV
        ? "http://localhost:3001/"
        : undefined;
  return url && /^https?:\/\//i.test(url) ? url : undefined;
}

/** Opens My documents, or undefined when this build doesn't know where it is. */
export function myDocumentsOpener(url = myDocumentsUrl()): (() => void) | undefined {
  if (!url) {
    return undefined;
  }
  return () => {
    openInBrowser(url);
  };
}

/** A page's address: at the root of My documents' site, as its own footer links them. */
export function sitePageUrl(page: SitePage, base: string): string {
  return new URL(`/${page}.html`, base).href;
}

/** Opens Privacy or Terms, or undefined when this build doesn't know where they are. */
export function sitePageOpener(url = myDocumentsUrl()): ((page: SitePage) => void) | undefined {
  if (!url) {
    return undefined;
  }
  return (page) => {
    openInBrowser(sitePageUrl(page, url));
  };
}

function openInBrowser(url: string) {
  if (Office.context.requirements.isSetSupported("OpenBrowserWindowApi", "1.1")) {
    Office.context.ui.openBrowserWindow(url);
  } else {
    window.open(url, "_blank", "noopener");
  }
}
