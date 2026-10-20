// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Macros, queries and pivot tables in My documents (Phase 5B), drawn from
// the golden replies: the code before and after side by side, a pivot
// table's fields, and "Contains macros" on a macro workbook.

import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { fakeDocuments, libraryView, macroDocumentView, objectsCompared } from "./fixtures";
import { LibraryScreen } from "./library";
import { Changes } from "./parts";

describe("macros and pivot tables compared", () => {
  it("shows the macro's code side by side and the pivot's fields", () => {
    render(<Changes view={objectsCompared()} />);
    const macros = screen.getByRole("region", { name: "Macros" });
    const code = within(macros).getByRole("table", { name: "The code, before and after" });
    const row = within(code)
      .getAllByRole("row")
      .find((r) => r.textContent.includes("months = 12"));
    if (!row) {
      throw new Error("the changed line");
    }
    expect(row.textContent).toContain("months = 18");
    expect(within(row).getByText("Removed:")).toBeTruthy();
    expect(within(row).getByText("Added:")).toBeTruthy();

    const pivots = screen.getByRole("region", { name: "Pivot tables" });
    const fields = within(pivots).getByRole("table", {
      name: "The pivot table's fields, before and after",
    });
    const rows = within(fields).getByRole("rowheader", { name: /^Rows/ }).closest("tr");
    expect(rows?.textContent).toBe("Rows (changed)PlanMonth");
  });
});

describe("Contains macros", () => {
  it("is on the document page's view of a macro workbook", () => {
    expect(macroDocumentView().page.badges).toEqual(["Contains macros"]);
  });

  it("is on a macro workbook's row in My documents", async () => {
    const library = libraryView();
    const first = library.entries[0];
    if (!first) {
      throw new Error("an entry");
    }
    library.entries = [{ ...first, badges: ["Contains macros"] }, ...library.entries.slice(1)];
    const documents = fakeDocuments({ list: vi.fn(() => Promise.resolve(library)) });
    render(<LibraryScreen documents={documents} where="your OneDrive" />);
    const row = (await screen.findAllByRole("link")).find((a) =>
      a.textContent.includes(first.name),
    );
    expect(row?.textContent).toContain("Contains macros");
  });
});
