// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import { afterEach, describe, expect, it, vi } from "vitest";
import { ExcelHost } from "./excel-host";

type Changed = (event: {
  worksheetId: string;
  address: string;
  source: string;
  triggerSource: string;
}) => Promise<void>;

/** Just enough of Office.js for `watchEdits`: one sheet, and its change event. */
function fakeExcel(): { fire: Changed } {
  let handler: Changed | undefined;
  const context = {
    workbook: {
      worksheets: {
        onChanged: {
          add: (h: Changed) => {
            handler = h;
          },
        },
        getItem: () => ({ name: "Assumptions", load: () => undefined }),
      },
    },
    sync: () => Promise.resolve(),
  };
  vi.stubGlobal("Office", {
    context: { requirements: { isSetSupported: () => true } },
  });
  vi.stubGlobal("Excel", {
    run: (f: (c: typeof context) => Promise<unknown>) => f(context),
    EventSource: { local: "Local", remote: "Remote" },
    EventTriggerSource: { thisLocalAddin: "ThisLocalAddin", unknown: "Unknown" },
  });
  return {
    fire: (event) => {
      if (!handler) {
        throw new Error("not watching");
      }
      return handler(event);
    },
  };
}

describe("Excel's edit hints", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("say whose edit each was, and leave out Lineage's own writes", async () => {
    const excel = fakeExcel();
    const host = new ExcelHost();
    await host.watchEdits();
    const event = { worksheetId: "1", triggerSource: "Unknown" };
    await excel.fire({ ...event, address: "B4", source: "Local" });
    await excel.fire({ ...event, address: "B3", source: "Remote" });
    await excel.fire({ ...event, address: "B4", source: "Remote" });
    await excel.fire({ ...event, address: "C9", source: "Local", triggerSource: "ThisLocalAddin" });
    await excel.fire({ ...event, address: "A:A", source: "Local" });
    expect(host.editHints()).toEqual([
      { sheet: "Assumptions", address: "B4", by: "you" },
      { sheet: "Assumptions", address: "B3", by: "coAuthor" },
      { sheet: "Assumptions", address: "B4", by: "coAuthor" },
    ]);
  });
});
