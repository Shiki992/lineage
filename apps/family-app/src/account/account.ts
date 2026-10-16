// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Who is signed in, and where their history is. There are no Lineage accounts
// (decision 0015): "signing in" means the person's own Microsoft account (and,
// from Phase 5, their Google account), and everything is read from their own
// storage, in this browser. The screens know only this interface, so a Google
// provider can be added without changing them.

import type { Storage } from "@lineage/shell";

export interface Person {
  /** "Avery Quinn". */
  readonly name: string;
  /** "avery@example.com", to tell accounts apart. */
  readonly username: string;
}

export type AccountStatus =
  | { readonly kind: "signedOut" }
  | { readonly kind: "signedIn"; readonly person: Person }
  /** Signing in failed; the words say why, in plain language. */
  | { readonly kind: "failed"; readonly message: string };

export interface AccountProvider {
  /** "Microsoft", "This computer". */
  readonly name: string;
  /** What the sign-in button says. */
  readonly signInLabel: string;
  /** Where the history is, for the page: "your OneDrive". */
  readonly where: string;
  /** Who is signed in now, once any sign-in redirect has been handled. */
  status(): Promise<AccountStatus>;
  /** Starts signing in. A provider may leave the page to do it. */
  signIn(): Promise<void>;
  /** Signs out of Lineage here: clears what this browser kept for it. */
  signOut(): Promise<void>;
  /** The person's storage; only while signed in. */
  storage(): Storage;
}

/** Keys this app keeps in session storage, so signing out can clear them all. */
export const SESSION_PREFIX = "lineage.";

/** Clears what My documents kept in this browser's session storage. */
export function clearSession(): void {
  try {
    for (const key of Object.keys(sessionStorage)) {
      if (key.startsWith(SESSION_PREFIX)) {
        sessionStorage.removeItem(key);
      }
    }
  } catch {
    // Storage can be unavailable (private windows); there is nothing to clear.
  }
}
