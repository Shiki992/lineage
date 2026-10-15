// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Sign-in with the person's Microsoft account through nested app
// authentication (NAA): Word signs them in, MSAL gets tokens through Word, and
// Lineage asks only for the Graph permission its OneDrive layout needs
// (decisions 0008 and 0009). Personal Microsoft accounts are supported.
// https://learn.microsoft.com/office/dev/add-ins/develop/enable-nested-app-authentication-in-your-add-in
//
// The app registration's client ID comes from apps/office-addin/.env.local,
// which is git-ignored: client IDs tied to a person are never committed. See
// docs/setup/microsoft-app-registration.md.

import type { AccountInfo, IPublicClientApplication } from "@azure/msal-browser";
import {
  InteractionRequiredAuthError,
  createNestablePublicClientApplication,
} from "@azure/msal-browser";
import type { Storage } from "@lineage/shell";
import { LineageError } from "@lineage/shell";
import type { Layout, TokenProvider } from "@lineage/storage-onedrive";
import { Graph, OneDriveStorage, appFolder, nextToMaster } from "@lineage/storage-onedrive";

const CLIENT_ID: unknown = import.meta.env["VITE_LINEAGE_CLIENT_ID"];
const GUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

let msal: Promise<IPublicClientApplication> | undefined;

function app(): Promise<IPublicClientApplication> {
  if (typeof CLIENT_ID !== "string" || !GUID.test(CLIENT_ID)) {
    throw new LineageError(
      "Sign-in isn't set up in this copy of Lineage yet.",
      "VITE_LINEAGE_CLIENT_ID is missing; see docs/setup/microsoft-app-registration.md",
    );
  }
  if (!Office.context.requirements.isSetSupported("NestedAppAuth", "1.1")) {
    throw new LineageError(
      "This version of Word can't sign Lineage in. Update Office, then try again.",
      "NestedAppAuth 1.1 is not supported",
    );
  }
  return (msal ??= createNestablePublicClientApplication({
    auth: { clientId: CLIENT_ID, authority: "https://login.microsoftonline.com/common" },
    cache: { cacheLocation: "localStorage" },
  }));
}

/**
 * Word on the web needs a login hint to pick the right account.
 * https://learn.microsoft.com/javascript/api/requirement-sets/common/nested-app-auth-requirement-sets
 */
async function loginHint(): Promise<string | undefined> {
  try {
    return (await Office.auth.getAuthContext()).userPrincipalName || undefined;
  } catch {
    return undefined;
  }
}

async function token(
  scopes: readonly string[],
  interactive: boolean,
  forceRefresh: boolean,
): Promise<{ accessToken: string; account: AccountInfo | null }> {
  const pca = await app();
  const account = pca.getAllAccounts()[0];
  if (account) {
    try {
      const r = await pca.acquireTokenSilent({ scopes: [...scopes], account, forceRefresh });
      return { accessToken: r.accessToken, account: r.account };
    } catch (error) {
      if (!(error instanceof InteractionRequiredAuthError)) {
        console.warn("Silent token failed", error);
      }
    }
  }
  const hint = await loginHint();
  const request = { scopes: [...scopes], ...(hint ? { loginHint: hint } : {}) };
  try {
    const r = await pca.ssoSilent(request);
    return { accessToken: r.accessToken, account: r.account };
  } catch (error) {
    if (!interactive || !(error instanceof InteractionRequiredAuthError)) {
      throw new LineageError("Sign in to use your OneDrive with Lineage.", String(error));
    }
  }
  try {
    const r = await pca.acquireTokenPopup(request);
    return { accessToken: r.accessToken, account: r.account };
  } catch (error) {
    throw new LineageError("Lineage couldn't sign you in. Try again.", String(error));
  }
}

/** Which layout: the app folder, unless a development build asks for the other. */
export function chooseLayout(): Layout {
  const wanted: unknown = import.meta.env["VITE_LINEAGE_LAYOUT"];
  if (import.meta.env.DEV && wanted === "nextToMaster") {
    const url = Office.context.document.url;
    if (url) {
      return nextToMaster(url);
    }
  }
  return appFolder;
}

/**
 * Signs in and returns OneDrive storage, and the person's name for checkpoint
 * records. `interactive: false` tries without showing anything, for start-up.
 */
export async function signIn(options: {
  interactive: boolean;
}): Promise<{ storage: Storage; name: string | null }> {
  const layout = chooseLayout();
  const first = await token(layout.scopes, options.interactive, false);
  const provider: TokenProvider = async ({ forceRefresh }) =>
    (await token(layout.scopes, true, forceRefresh)).accessToken;
  const storage = new OneDriveStorage(new Graph({ token: provider }), layout);
  return { storage, name: first.account?.name ?? null };
}
