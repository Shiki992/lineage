// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import type { AccountProvider } from "./account";
import { DevFolderAccount } from "./dev-folder";
import { MicrosoftAccount } from "./microsoft";

/** Microsoft sign-in, or the development folder in development builds. */
export function chooseAccount(): AccountProvider {
  if (import.meta.env.DEV && import.meta.env["VITE_LINEAGE_STORAGE"] !== "onedrive") {
    return new DevFolderAccount();
  }
  return new MicrosoftAccount(import.meta.env["VITE_LINEAGE_CLIENT_ID"]);
}
