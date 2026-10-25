// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import { afterEach, describe, expect, it, vi } from "vitest";
import { AUTOSAVE_WAIT_MS, ExcelHost } from "./excel-host";
import type { LastSaved } from "./excel-host";

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
      properties: { custom: { add: () => undefined } },
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

/** A saved workbook whose tag follows a script: each tag() call takes the next. */
function savedFile(tags: string[]): LastSaved & { reads: number; asked: number } {
  let i = 0;
  const current = () => tags[Math.min(i, tags.length - 1)] ?? "";
  const file = {
    reads: 0,
    asked: 0,
    tag: () => {
      file.asked += 1;
      const tag = current();
      i += 1;
      return Promise.resolve(tag);
    },
    read: () => {
      file.reads += 1;
      const tag = tags[Math.min(Math.max(i - 1, 0), tags.length - 1)] ?? "";
      return Promise.resolve({ bytes: new TextEncoder().encode(tag), tag });
    },
  };
  return file;
}

describe("Excel on the web", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  function clocked() {
    let now = 10_000;
    return {
      clock: () => now,
      wait: (ms: number) => {
        now += ms;
        return Promise.resolve();
      },
      advance: (ms: number) => {
        now += ms;
      },
    };
  }

  it("reads the workbook as last saved, at once when nothing changed lately", async () => {
    fakeExcel();
    const saved = savedFile(["t1"]);
    const time = clocked();
    const host = new ExcelHost(saved, time.wait, time.clock);
    expect(host.readsLastSaved).toBe(true);
    expect(new TextDecoder().decode(await host.readBytes())).toBe("t1");
    expect(saved.asked).toBe(0);
    expect(new ExcelHost().readsLastSaved).toBe(false);
  });

  it("after Lineage's own writes, waits for Excel's autosave to carry them", async () => {
    fakeExcel();
    const saved = savedFile(["t1", "t1", "t1", "t2"]);
    const time = clocked();
    const host = new ExcelHost(saved, time.wait, time.clock);
    await host.readBytes();
    await host.writeIds({ family: "a".repeat(32), checkpoint: "b".repeat(32) });
    expect(new TextDecoder().decode(await host.readBytes())).toBe("t2");
    expect(saved.reads).toBe(2);
  });

  it("says so when Excel hasn't saved in time, and reads nothing stale", async () => {
    fakeExcel();
    const saved = savedFile(["t1"]);
    const time = clocked();
    const host = new ExcelHost(saved, time.wait, time.clock);
    await host.readBytes();
    await host.applyCellPlan([]);
    await expect(host.readBytes()).rejects.toMatchObject({
      userMessage: "Excel hasn't saved the workbook yet. Wait a moment, then try again.",
    });
    expect(saved.reads).toBe(1);
    expect(saved.asked).toBeGreaterThanOrEqual(AUTOSAVE_WAIT_MS / 1000);
  });

  it("after an edit a moment ago, waits for the saved file to stop changing", async () => {
    const excel = fakeExcel();
    const saved = savedFile(["t1", "t2", "t3", "t3", "t3"]);
    const time = clocked();
    const host = new ExcelHost(saved, time.wait, time.clock);
    await host.watchEdits();
    await excel.fire({
      worksheetId: "1",
      address: "B4",
      source: "Local",
      triggerSource: "Unknown",
    });
    time.advance(500);
    expect(new TextDecoder().decode(await host.readBytes())).toBe("t3");
    expect(saved.asked).toBeGreaterThanOrEqual(3);
  });
});
