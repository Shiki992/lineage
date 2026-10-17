// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import { EngineError } from "./engine.js";
import { StorageError } from "./storage.js";

/** A failure the person can act on, with the words the panel shows. */
export class LineageError extends Error {
  override name = "LineageError";

  constructor(
    readonly userMessage: string,
    detail: string,
  ) {
    super(detail);
  }
}

/** The words to show for any failure. */
export function userMessage(error: unknown): string {
  if (
    error instanceof LineageError ||
    error instanceof StorageError ||
    error instanceof EngineError
  ) {
    return error.userMessage;
  }
  return "Something went wrong. Nothing was changed. Try again.";
}
