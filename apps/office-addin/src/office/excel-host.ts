// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The open Excel workbook, through Office.js. Bytes only, as in Word and
// PowerPoint: Lineage reads the whole file, and the engine works out every
// change. Excel can't replace the open workbook, so changes come in as the
// engine's cell and row steps, carried out here (decision 0023).
//
// How each kind of value is written was settled by the stop 2 spike in Excel
// for Windows (16.0.20430, ExcelApi 1.21):
// - `formulas` and `values` read text as if it were typed: "00123" becomes
//   123, "TRUE" a boolean, "1E-7" or "50%" a number that also changes the
//   cell's number format, and only 15 digits are kept. So numbers are written
//   as JavaScript numbers through `values`, which keeps every digit of the
//   double and the cell's format;
// - text is written through `valuesAsJson` as a string, which Excel stores
//   exactly (ExcelApi 1.16). A leading apostrophe is Excel's quote prefix even
//   there, so text that starts with one is written with one more, and Excel
//   gives the cell its quote-prefix style;
// - formulas (en-US, with "=") and errors go through `formulas`; booleans
//   through `values`.
// https://learn.microsoft.com/javascript/api/excel/excel.range#excel-excel-range-valuesasjson-member
// https://learn.microsoft.com/javascript/api/excel/excel.range#excel-excel-range-values-member

import type {
  CellHost,
  CellNavigator,
  CellStep,
  CellWrite,
  DocumentHost,
  FileLocation,
  Hint,
  Ids,
} from "@lineage/shell";
import { LineageError, PlanStopped } from "@lineage/shell";
import { CHECKPOINT, FAMILY, base64, describeOpenFile, idsFrom, readWholeFile } from "./common";

/** ExcelApi 1.7 has the custom properties Lineage's ids live in. */
export const NEEDED = "1.7";

/**
 * ExcelApi 1.16 writes text exactly (`valuesAsJson`); 1.14 tells Lineage's own
 * writes apart from the person's in change events (`triggerSource`).
 */
export const CELL_STEPS = "1.16";

/** Cells written per sync: well inside Excel on the web's 5 MB request limit. */
const CELLS_PER_SYNC = 1000;

/** Most edit hints kept for a session; past this "What changed" goes without. */
const MAX_HINTS = 10_000;

export class ExcelHost implements DocumentHost, CellHost, CellNavigator {
  private readonly hints = new Map<string, Hint>();
  private watching = false;

  /** True when this Excel can carry out every kind of cell step. */
  canApplyPlan(): boolean {
    return Office.context.requirements.isSetSupported("ExcelApi", CELL_STEPS);
  }

  /**
   * Starts keeping the ranges the person edits, from Excel's change events,
   * leaving out Lineage's own writes. Hints only: "What changed" works from
   * the bytes without them.
   * https://learn.microsoft.com/javascript/api/excel/excel.worksheetcollection#excel-excel-worksheetcollection-onchanged-member
   * https://learn.microsoft.com/javascript/api/excel/excel.worksheetchangedeventargs#excel-excel-worksheetchangedeventargs-triggersource-member
   */
  async watchEdits(): Promise<void> {
    if (this.watching || !Office.context.requirements.isSetSupported("ExcelApi", "1.14")) {
      return;
    }
    this.watching = true;
    await Excel.run(async (context) => {
      context.workbook.worksheets.onChanged.add(async (event) => {
        if (event.triggerSource === Excel.EventTriggerSource.thisLocalAddin) {
          return;
        }
        await Excel.run(async (inner) => {
          const sheet = inner.workbook.worksheets.getItem(event.worksheetId);
          sheet.load("name");
          await inner.sync();
          this.addHint(sheet.name, event.address);
        });
      });
      await context.sync();
    });
  }

  private addHint(sheet: string, address: string): void {
    // A whole row or column, or several areas, is too coarse to say anything.
    if (!/^[A-Z]{1,3}[1-9][0-9]*(:[A-Z]{1,3}[1-9][0-9]*)?$/.test(address)) {
      return;
    }
    if (this.hints.size < MAX_HINTS) {
      this.hints.set(`${sheet}!${address}`, { sheet, address });
    }
  }

  editHints(): readonly Hint[] {
    return [...this.hints.values()];
  }

  /**
   * Carries out the engine's steps in order. Row inserts and deletes sync one
   * at a time, so a failure among them says exactly how many ran; cells are
   * written in batches, and a batch that fails counts as done, since writing a
   * cell back to what it held is harmless (the way back does that).
   */
  async applyCellPlan(steps: readonly CellStep[]): Promise<void> {
    let done = 0;
    try {
      await Excel.run(async (context) => {
        const sheets = context.workbook.worksheets;
        let pending = 0;
        for (const [i, step] of steps.entries()) {
          const sheet = sheets.getItem(step.sheet);
          switch (step.kind) {
            case "insertRows":
              if (pending > 0) {
                await context.sync();
                done = i;
                pending = 0;
              }
              sheet.getRange(step.rows).insert(Excel.InsertShiftDirection.down);
              await context.sync();
              done = i + 1;
              continue;
            case "deleteRows":
              if (pending > 0) {
                await context.sync();
                done = i;
                pending = 0;
              }
              sheet.getRange(step.rows).delete(Excel.DeleteShiftDirection.up);
              await context.sync();
              done = i + 1;
              continue;
            case "set":
              write(sheet.getRange(step.address), step.value);
              pending += 1;
              if (pending >= CELLS_PER_SYNC) {
                done = i + 1;
                await context.sync();
                pending = 0;
              }
              continue;
          }
        }
        done = steps.length;
        await context.sync();
      });
    } catch (error) {
      throw new PlanStopped(done, error instanceof Error ? error.message : String(error));
    }
  }

  /**
   * "Go to cell": shows the sheet and selects the cell or range.
   * https://learn.microsoft.com/javascript/api/excel/excel.range#excel-excel-range-select-member(1)
   */
  async goToCell(sheet: string, address: string): Promise<void> {
    await Excel.run(async (context) => {
      const target = context.workbook.worksheets.getItem(sheet);
      target.activate();
      target.getRange(address).select();
      await context.sync();
    });
  }

  readBytes(): Promise<Uint8Array> {
    return readWholeFile("Excel");
  }

  /**
   * The two ids, from the workbook's custom properties (ExcelApi 1.7). The
   * engine reads the same properties from the bytes (docProps/custom.xml).
   * https://learn.microsoft.com/javascript/api/excel/excel.custompropertycollection
   */
  async readIds(): Promise<Ids | null> {
    return Excel.run(async (context) => {
      const props = context.workbook.properties.custom;
      const family = props.getItemOrNullObject(FAMILY);
      const checkpoint = props.getItemOrNullObject(CHECKPOINT);
      family.load("value");
      checkpoint.load("value");
      await context.sync();
      if (family.isNullObject || checkpoint.isNullObject) {
        return null;
      }
      return idsFrom(family.value, checkpoint.value);
    });
  }

  async writeIds(ids: Ids | null): Promise<void> {
    await Excel.run(async (context) => {
      const props = context.workbook.properties.custom;
      if (ids) {
        props.add(FAMILY, ids.family);
        props.add(CHECKPOINT, ids.checkpoint);
      } else {
        props.getItemOrNullObject(FAMILY).delete();
        props.getItemOrNullObject(CHECKPOINT).delete();
      }
      await context.sync();
    });
  }

  /** Never called for workbooks: there's no way to replace the open workbook. */
  replaceContents(): Promise<void> {
    return Promise.reject(
      new LineageError(
        "Bring the changes in from this workbook's panel.",
        "Excel can't replace the open workbook",
      ),
    );
  }

  /**
   * Saves the workbook where it is (ExcelApi 1.11), so a file on this
   * computer matches the checkpoint just recorded, as Word's does. Files in
   * OneDrive autosave anyway.
   * https://learn.microsoft.com/javascript/api/excel/excel.workbook#excel-excel-workbook-save-member(1)
   */
  async save(): Promise<void> {
    if (!Office.context.requirements.isSetSupported("ExcelApi", "1.11")) {
      return;
    }
    await Excel.run(async (context) => {
      context.workbook.save(Excel.SaveBehavior.save);
      await context.sync();
    });
  }

  describe(): Promise<{ name: string; url: string | null }> {
    return Promise.resolve(describeOpenFile("This workbook"));
  }

  /**
   * Opens a stored workbook: in Excel for the web for a file in OneDrive.
   * Add-ins can't open a file on this computer.
   * https://learn.microsoft.com/javascript/api/office/office.ui#office-office-ui-openbrowserwindow-member(1)
   */
  openFile(location: FileLocation): Promise<void> {
    if (
      "webUrl" in location &&
      Office.context.requirements.isSetSupported("OpenBrowserWindowApi", "1.1")
    ) {
      Office.context.ui.openBrowserWindow(location.webUrl);
      return Promise.resolve();
    }
    return Promise.reject(
      new LineageError(
        "Excel can't open that file from here. It's in Lineage's folder; open it from there.",
        "no way to open a local file",
      ),
    );
  }

  /**
   * Opens bytes as a new workbook in a new window: "open what they saw".
   * https://learn.microsoft.com/javascript/api/excel#excel-excel-createworkbook-function(1)
   */
  async openCopy(bytes: Uint8Array): Promise<void> {
    await Excel.createWorkbook(base64(bytes));
  }
}

/** Queues one cell write, as the spike showed Excel stores each kind exactly. */
function write(range: Excel.Range, value: CellWrite): void {
  switch (value.kind) {
    case "clear":
      range.clear(Excel.ClearApplyTo.contents);
      return;
    case "number": {
      const n = Number(value.text);
      if (!Number.isFinite(n)) {
        throw new LineageError(
          "Lineage couldn't write a number in this workbook.",
          "a number lexeme that isn't finite",
        );
      }
      range.values = [[n]];
      return;
    }
    case "text":
      range.valuesAsJson = [
        [
          {
            type: Excel.CellValueType.string,
            basicValue: value.text.startsWith("'") ? `'${value.text}` : value.text,
          },
        ],
      ];
      return;
    case "bool":
      range.values = [[value.value]];
      return;
    case "error":
    case "formula":
      range.formulas = [[value.text]];
      return;
  }
}
