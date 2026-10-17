// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// MSAL v5's redirect bridge: Microsoft sign-in returns here, and this page
// hands the response back (for a redirect, by going back to the page that asked).
// It must be served from this origin, with no Cross-Origin-Opener-Policy header
// and Cache-Control: no-store, and registered as a redirect URI.
// https://github.com/AzureAD/microsoft-authentication-library-for-js/blob/dev/lib/msal-browser/docs/redirect-bridge.md

import { broadcastResponseToMainFrame } from "@azure/msal-browser/redirect-bridge";

broadcastResponseToMainFrame().catch((error: unknown) => {
  console.error(error);
  const note = document.getElementById("note");
  if (note) {
    note.textContent = "Signing in didn't finish. Close this page and try again.";
  }
});
