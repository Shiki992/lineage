// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// "My documents" in the footer opens the family app in the person's browser.
// openBrowserWindow (OpenBrowserWindowApi 1.1) opens the default browser from
// Word on Windows and Mac; elsewhere a new tab does the same.
// https://learn.microsoft.com/javascript/api/office/office.ui#office-office-ui-openbrowserwindow-member(1)

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
    if (Office.context.requirements.isSetSupported("OpenBrowserWindowApi", "1.1")) {
      Office.context.ui.openBrowserWindow(url);
    } else {
      window.open(url, "_blank", "noopener");
    }
  };
}
