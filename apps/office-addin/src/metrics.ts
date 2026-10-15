// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Timings for the task-pane budgets (decision 0010): engine load, panel ready,
// and each command. In development they also go to the dev server's log, as
// names and milliseconds only.

import { devLog } from "./storage/dev-folder";

/** When the page's scripts started, for "panel ready". */
export const pageStart = performance.now();

export const timings = new Map<string, number>();

export function timing(name: string, ms: number): void {
  timings.set(name, ms);
  if (import.meta.env.DEV) {
    devLog({ name, ms });
  }
}

/** An error kind for the dev log; never the message, which may quote the document. */
export function errorKind(kind: string): void {
  if (import.meta.env.DEV) {
    devLog({ name: "error", kind });
  }
}
