// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import type { Storage } from "@lineage/shell";
import { DevFolderStorage } from "./dev-folder";

/** Where history goes: OneDrive after sign-in, or the dev folder in development. */
export type StorageChoice = { readonly kind: "ready"; readonly storage: Storage } | { readonly kind: "signIn" };

export function chooseStorage(): StorageChoice {
  if (import.meta.env.DEV && import.meta.env["VITE_LINEAGE_STORAGE"] !== "onedrive") {
    return { kind: "ready", storage: new DevFolderStorage() };
  }
  return { kind: "signIn" };
}
