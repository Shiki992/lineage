// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Development builds only: the history in a folder on this computer, served by
// the dev server (@lineage/storage-local), so My documents can be tried, and
// tested with Playwright, before a Microsoft app registration exists.

import type { Storage } from "@lineage/shell";
import { LocalFolderStorage } from "@lineage/storage-local";
import type { AccountProvider, AccountStatus } from "./account";
import { clearSession } from "./account";

const KEY = "lineage.devSignedOut";

export class DevFolderAccount implements AccountProvider {
  readonly name = "This computer";
  readonly signInLabel = "Use the development folder";
  readonly where = "the development folder on this computer";
  private readonly local = new LocalFolderStorage();

  status(): Promise<AccountStatus> {
    let out = false;
    try {
      out = sessionStorage.getItem(KEY) === "1";
    } catch {
      // No session storage: stay signed in.
    }
    return Promise.resolve(
      out
        ? { kind: "signedOut" }
        : {
            kind: "signedIn",
            person: { name: "This computer (development)", username: "dev-folder" },
          },
    );
  }

  signIn(): Promise<void> {
    try {
      sessionStorage.removeItem(KEY);
    } catch {
      // Nothing kept.
    }
    return Promise.resolve();
  }

  signOut(): Promise<void> {
    clearSession();
    try {
      sessionStorage.setItem(KEY, "1");
    } catch {
      // Nothing kept.
    }
    return Promise.resolve();
  }

  storage(): Storage {
    return this.local;
  }
}
