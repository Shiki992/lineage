// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Signing in with the person's Microsoft account, as a static web app: MSAL's
// standard public client, authorization code with PKCE, redirect flow. It asks
// only for the app folder permission the Word add-in uses, with the same client
// ID, so both see the same Apps/Lineage folder (decisions 0009 and 0015).
//
// - https://github.com/AzureAD/microsoft-authentication-library-for-js/blob/dev/lib/msal-browser/docs/initialization.md
// - v5 sends every response through a redirect bridge page (redirect.html):
//   https://github.com/AzureAD/microsoft-authentication-library-for-js/blob/dev/lib/msal-browser/docs/redirect-bridge.md
// - The app folder is named for the calling app id:
//   https://learn.microsoft.com/onedrive/developer/rest-api/concepts/special-folders-appfolder

import type { AccountInfo, IPublicClientApplication } from "@azure/msal-browser";
import {
  InteractionRequiredAuthError,
  createStandardPublicClientApplication,
} from "@azure/msal-browser";
import type { Storage } from "@lineage/shell";
import { LineageError } from "@lineage/shell";
import { Graph, OneDriveStorage, appFolder } from "@lineage/storage-onedrive";
import type { AccountProvider, AccountStatus } from "./account";
import { clearSession } from "./account";

const GUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const SCOPES = appFolder.scopes;

export class MicrosoftAccount implements AccountProvider {
  readonly name = "Microsoft";
  readonly signInLabel = "Sign in with Microsoft";
  readonly where = "your OneDrive";
  private readonly app: Promise<IPublicClientApplication>;
  private account: AccountInfo | undefined;
  private onedrive: Storage | undefined;

  constructor(clientId: unknown, origin = window.location.origin) {
    if (typeof clientId !== "string" || !GUID.test(clientId)) {
      this.app = Promise.reject(
        new LineageError(
          "Sign-in isn't set up in this copy of Lineage yet.",
          "VITE_LINEAGE_CLIENT_ID is missing; see docs/setup/microsoft-app-registration.md",
        ),
      );
      this.app.catch(() => undefined);
      return;
    }
    this.app = createStandardPublicClientApplication({
      auth: {
        clientId,
        authority: "https://login.microsoftonline.com/common",
        redirectUri: `${origin}/redirect.html`,
        postLogoutRedirectUri: `${origin}/`,
      },
      // Gone when the tab closes; signing out clears it at once.
      cache: { cacheLocation: "sessionStorage" },
    });
  }

  async status(): Promise<AccountStatus> {
    let app: IPublicClientApplication;
    try {
      app = await this.app;
    } catch (error) {
      return { kind: "failed", message: messageOf(error) };
    }
    try {
      const result = await app.handleRedirectPromise();
      this.account = result?.account ?? app.getAllAccounts()[0];
    } catch (error) {
      console.warn("Sign-in failed", error);
      return { kind: "failed", message: "Lineage couldn't sign you in. Try again." };
    }
    if (!this.account) {
      return { kind: "signedOut" };
    }
    return {
      kind: "signedIn",
      person: { name: this.account.name ?? this.account.username, username: this.account.username },
    };
  }

  async signIn(): Promise<void> {
    const app = await this.app;
    await app.loginRedirect({ scopes: [...SCOPES], prompt: "select_account" });
  }

  async signOut(): Promise<void> {
    const app = await this.app;
    // Lineage only: the Microsoft session stays, so Outlook and the rest stay
    // signed in (decision 0015).
    await app.clearCache(this.account ? { account: this.account } : undefined);
    this.account = undefined;
    this.onedrive = undefined;
    clearSession();
  }

  private async token(forceRefresh: boolean): Promise<string> {
    const app = await this.app;
    const account = this.account;
    if (!account) {
      throw new LineageError("Sign in to see your documents.", "no account");
    }
    try {
      return (await app.acquireTokenSilent({ scopes: [...SCOPES], account, forceRefresh }))
        .accessToken;
    } catch (error) {
      if (error instanceof InteractionRequiredAuthError) {
        await app.acquireTokenRedirect({ scopes: [...SCOPES], account });
      }
      throw new LineageError("Sign in again to see your documents.", String(error));
    }
  }

  storage(): Storage {
    this.onedrive ??= new OneDriveStorage(
      new Graph({ token: ({ forceRefresh }) => this.token(forceRefresh) }),
      appFolder,
    );
    return this.onedrive;
  }
}

function messageOf(error: unknown): string {
  return error instanceof LineageError ? error.userMessage : "Lineage couldn't sign you in.";
}
