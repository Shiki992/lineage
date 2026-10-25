// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The open document, as the platform exposes it: Office.js in Word, PowerPoint
// and Excel (apps/office-addin/src/office/), a fake in tests, and later the
// Google APIs. Bytes only: the host never looks inside them.

import type { CellStep, Hint, Ids, SlideOp } from "./schemas.js";
import type { FileLocation } from "./storage.js";

/**
 * A host that can change the slides of the open deck (PowerPoint), which
 * can't replace the open file (decision 0019).
 */
export interface SlideHost {
  /** True when this PowerPoint can carry out every kind of slide step. */
  canChangeSlides(): boolean;
  /**
   * Carries out the steps in order, inserting slides from `source`. Stops at
   * the first step that fails, with an error; the steps before it stay done.
   */
  applySlideOps(ops: readonly SlideOp[], source: Uint8Array): Promise<void>;
}

/** True when `host` can change the open deck's slides. */
export function changesSlides(host: DocumentHost): host is DocumentHost & SlideHost {
  const slides = host as Partial<SlideHost>;
  return (
    typeof slides.applySlideOps === "function" &&
    typeof slides.canChangeSlides === "function" &&
    slides.canChangeSlides()
  );
}

/**
 * Thrown by `applyCellPlan` when Excel stopped partway: the first `done` steps
 * were carried out, the rest weren't.
 */
export class PlanStopped extends Error {
  constructor(
    readonly done: number,
    readonly reason: string,
  ) {
    super(`stopped after ${String(done)} steps: ${reason}`);
    this.name = "PlanStopped";
  }
}

/**
 * A host that can change the cells and rows of the open workbook (Excel),
 * which can't replace the open file (decision 0023).
 */
export interface CellHost {
  /** True when this Excel can carry out every kind of cell step. */
  canApplyPlan(): boolean;
  /**
   * Carries out the steps in order. Stops at the first step that fails with
   * a `PlanStopped` saying how many were done; those stay done.
   */
  applyCellPlan(steps: readonly CellStep[]): Promise<void>;
  /**
   * The ranges the person edited in this session, from Excel's change events
   * (never Lineage's own writes). Hints only: "What changed" works without them.
   */
  editHints(): readonly Hint[];
}

/** True when `host` can change the open workbook's cells. */
export function changesCells(host: DocumentHost): host is DocumentHost & CellHost {
  const cells = host as Partial<CellHost>;
  return (
    typeof cells.applyCellPlan === "function" &&
    typeof cells.canApplyPlan === "function" &&
    typeof cells.editHints === "function" &&
    cells.canApplyPlan()
  );
}

/** A host that can show a cell of the open workbook ("Go to cell"). */
export interface CellNavigator {
  /** Activates `sheet` and selects `address` ("B4" or "B4:C9"). */
  goToCell(sheet: string, address: string): Promise<void>;
}

/** True when `host` can show a cell. */
export function goesToCells(host: DocumentHost): host is DocumentHost & CellNavigator {
  return typeof (host as Partial<CellNavigator>).goToCell === "function";
}

/** True when `host` reports edits in the open workbook, whether or not it can apply plans. */
export function reportsEdits(
  host: DocumentHost,
): host is DocumentHost & Pick<CellHost, "editHints"> {
  return typeof (host as Partial<CellHost>).editHints === "function";
}

export interface DocumentHost {
  /** The open document as a .docx, unsaved edits included. */
  readBytes(): Promise<Uint8Array>;
  /** Lineage's ids from the document's custom properties, or null. */
  readIds(): Promise<Ids | null>;
  /** Sets the ids, or removes them when `ids` is null. Other properties stay. */
  writeIds(ids: Ids | null): Promise<void>;
  /**
   * Makes the open document hold exactly `bytes`. Returns once the host holds
   * them; the caller reads them back to check (decision 0008).
   */
  replaceContents(bytes: Uint8Array): Promise<void>;
  save(): Promise<void>;
  /** The open document's file name, and where it is saved (null if never saved). */
  describe(): Promise<{ name: string; url: string | null }>;
  /** Opens a stored file in a new window or tab. */
  openFile(location: FileLocation): Promise<void>;
  /** Opens bytes as a new, unsaved document, such as "what they received". */
  openCopy(bytes: Uint8Array): Promise<void>;
  /**
   * Calls `onEdit` whenever the app reports an edit by someone editing the
   * same file with this person (co-authoring, a share link; decision 0032).
   * A hint only: nothing is recorded. Optional: PowerPoint can't tell.
   */
  watchCoAuthors?(onEdit: () => void): Promise<void>;
}
