// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The workbook screens, drawn from the engine's golden replies and found by
// role and accessible name: "What changed" in the five categories with Go to
// cell and a side by side, the review of the master's updates, a returned
// workbook's changes, and what goes with a sent copy.

import { readFileSync } from "node:fs";
import path from "node:path";
import type { Family, ReturnReview, SideBySide, Updates, WhatChanged } from "@lineage/shell";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { WhatChangedView } from "./changes";
import { SendCopy } from "./forms";
import { WorkbookReview } from "./workbook";

function golden(name: string): unknown {
  const file = path.resolve(process.cwd(), "../../packages/shell/test/golden", `${name}.json`);
  return (JSON.parse(readFileSync(file, "utf8")) as { value: unknown }).value;
}

describe("what changed in a workbook", () => {
  it("groups the changes by category, with Go to cell and a side by side", async () => {
    const view = golden("workbook-what-changed") as WhatChanged;
    const side = golden("cell-side-by-side") as SideBySide;
    const goTo = vi.fn();
    const sideBySide = vi.fn(() => Promise.resolve(side));
    render(<WhatChangedView view={view} cells={{ goTo, sideBySide }} />);
    expect(screen.getByRole("status").textContent).toBe("1 input, 83 knock-on changes since v1");
    const inputs = screen.getByRole("region", { name: "Inputs" });
    expect(within(inputs).getByText("'$49' changed to '$55'")).toBeTruthy();
    expect(within(inputs).getByText("Edited in this session")).toBeTruthy();
    const knockOn = screen.getByRole("region", { name: "Knock-on changes" });
    expect(within(knockOn).getByText(/^This changed 83 calculated cells/)).toBeTruthy();

    fireEvent.click(within(inputs).getByRole("button", { name: "Go to cell" }));
    expect(goTo).toHaveBeenCalledWith("Assumptions", "B4");
    fireEvent.click(within(inputs).getByRole("button", { name: "Side by side" }));
    expect(sideBySide).toHaveBeenCalledWith("Assumptions", "B4");
    const shown = await screen.findByLabelText("Side by side");
    expect(within(shown).getByText("$49")).toBeTruthy();
    expect(within(shown).getByText("$55")).toBeTruthy();
    expect(within(shown).getAllByText("number")).toHaveLength(2);
  });
});

describe("the master's updates in an open workbook", () => {
  it("brings in the chosen cards' items", () => {
    const updates = golden("updates-workbook") as Updates;
    const onApply = vi.fn();
    const groups = updates.groups.flatMap((g) =>
      g.view.workbook ? [{ key: g.checkpoint, label: g.label, view: g.view.workbook }] : [],
    );
    render(
      <WorkbookReview
        title={updates.headline}
        groups={groups}
        verb="Bring in"
        canBringIn
        busy={false}
        onApply={onApply}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe("1 update from the master");
    expect(screen.getByText("'$49' changed to '$55'")).toBeTruthy();
    // Nothing to press until something is chosen.
    expect(screen.queryByRole("button", { name: /^Bring in 1 change/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Bring in" }));
    fireEvent.click(screen.getByRole("button", { name: "Bring in 1 change" }));
    const card = groups[0]?.view.groups[0]?.cards[0];
    expect(onApply).toHaveBeenCalledWith(
      card?.ids.map((id) => ({ card: id, decision: "bringIn" })),
    );
  });

  it("shows what changed but asks for a newer Excel when this one can't bring it in", () => {
    const updates = golden("updates-workbook") as Updates;
    render(
      <WorkbookReview
        title={updates.headline}
        groups={updates.groups.flatMap((g) =>
          g.view.workbook ? [{ key: g.checkpoint, label: g.label, view: g.view.workbook }] : [],
        )}
        verb="Bring in"
        canBringIn={false}
        busy={false}
        onApply={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByText(/^This version of Excel can't take the changes/)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Bring in" })).toBeNull();
  });
});

describe("a workbook that came back", () => {
  it("keeps the chosen changes", () => {
    const review = golden("returned-review-workbook") as ReturnReview;
    const onApply = vi.fn();
    const view = review.view.workbook;
    if (!view) {
      throw new Error("a workbook review");
    }
    render(
      <WorkbookReview
        title={review.headline}
        groups={[{ key: review.returned, label: null, view }]}
        verb="Keep"
        canBringIn
        busy={false}
        onApply={onApply}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByText("'2.00%' changed to '3.00%'")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    fireEvent.click(screen.getByRole("button", { name: "Keep 1 change" }));
    expect(onApply).toHaveBeenCalledTimes(1);
  });
});

describe("sending a workbook", () => {
  it("says what goes with the copy before it's saved", () => {
    const family = {
      ...(golden("family") as Family),
      format: "xlsx",
    };
    const goesWith = golden("goes-with-copy") as string[];
    render(
      <SendCopy
        family={family}
        goesWith={goesWith}
        openNode={family.nodes[0]?.id ?? ""}
        busy={false}
        onSend={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByText("Excel (.xlsx, or .xlsm with macros)")).toBeTruthy();
    const warning = screen.getByRole("region", { name: "What goes with this copy" });
    expect(
      within(warning).getByText("2 hidden sheets go with this copy: formulas may read them."),
    ).toBeTruthy();
  });

  it("gives no hidden-sheet reason when only macros go with the copy", () => {
    const family = { ...(golden("family") as Family), format: "xlsx" };
    render(
      <SendCopy
        family={family}
        goesWith={["Macros go with this copy"]}
        openNode={family.nodes[0]?.id ?? ""}
        busy={false}
        onSend={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    const warning = screen.getByRole("region", { name: "What goes with this copy" });
    expect(within(warning).getByText("Macros go with this copy.")).toBeTruthy();
    expect(within(warning).queryByText(/formulas/i)).toBeNull();
    expect(within(warning).getByText("Check before you send it.")).toBeTruthy();
  });
});

describe("macros, queries and pivot tables (Phase 5B)", () => {
  it("shows the macro's lines marked, the pivot's fields, and opens the master's version", () => {
    const view = golden("workbook-what-changed-objects") as WhatChanged & {
      workbook: NonNullable<WhatChanged["workbook"]>;
    };
    const onOpenMaster = vi.fn();
    render(
      <WorkbookReview
        title={view.headline}
        groups={[{ key: "c1", label: null, view: view.workbook }]}
        verb="Bring in"
        canBringIn
        busy={false}
        onOpenMaster={onOpenMaster}
        onApply={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    const macros = screen.getByRole("region", { name: "Macros" });
    expect(within(macros).getByText("Macro RefreshForecast: 1 line changed")).toBeTruthy();
    const code = within(macros).getByRole("list", { name: "− Removed + Added" });
    expect(within(code).getByText(/^Removed line 6/)).toBeTruthy();
    expect(within(code).getByText(/^Added line 6/)).toBeTruthy();
    expect(within(code).getByText("months = 18")).toBeTruthy();
    expect(within(macros).getByRole("button", { name: "Copy code" })).toBeTruthy();

    const pivots = screen.getByRole("region", { name: "Pivot tables" });
    const table = within(pivots).getByRole("table");
    const rows = within(table).getByRole("rowheader", { name: /^Rows/ }).closest("tr");
    expect(rows?.textContent).toBe("Rows (changed)PlanMonth");

    // No warning: this is the person's own master.
    expect(screen.queryByRole("alert")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Open the master's version" }));
    expect(onOpenMaster).toHaveBeenCalledWith("c1");
  });

  it("warns about a file that came back with new macro code, and asks before copying it", async () => {
    const review = golden("returned-review-macros") as ReturnReview;
    const workbook = review.view.workbook;
    if (!workbook) {
      throw new Error("a workbook view");
    }
    const writeText = vi.fn(() => Promise.resolve());
    Object.assign(navigator, { clipboard: { writeText } });
    const onOpenMaster = vi.fn();
    render(
      <WorkbookReview
        title={review.headline}
        groups={[{ key: review.returned, label: null, view: workbook }]}
        verb="Keep"
        canBringIn
        busy={false}
        fromOutside
        onOpenMaster={onOpenMaster}
        onApply={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByRole("alert").textContent).toMatch(
      /^This file came back with new macro code\. Lineage never runs it\./,
    );
    expect(screen.queryByRole("button", { name: "Open the master's version" })).toBeNull();
    const macros = screen.getByRole("region", { name: "Macros" });
    const copies = within(macros).getAllByRole("button", { name: "Copy code" });
    expect(copies).toHaveLength(2);
    const [first] = copies;
    if (!first) {
      throw new Error("a Copy code button");
    }
    fireEvent.click(first);
    expect(writeText).not.toHaveBeenCalled();
    const ask = within(macros).getByRole("group", { name: "Copy their code?" });
    fireEvent.click(within(ask).getByRole("button", { name: "Copy their code" }));
    expect(writeText).toHaveBeenCalledTimes(1);
    expect(await within(macros).findByText(/^Copied\. Paste it in the VBA editor/)).toBeTruthy();
    // Each module is decided on its own: Skip only, never a bulk choice.
    for (const group of within(macros).getAllByRole("group", { name: "Your decision" })) {
      expect(
        within(group)
          .getAllByRole("button")
          .map((b) => b.textContent),
      ).toEqual(["Skip"]);
    }
  });
});
