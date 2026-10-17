// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// A document's page: loading, normal, missing file, damaged, error, and each
// action, drawn from the golden replies.

import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { NotFound } from "@lineage/shell";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DocumentScreen } from "./document";
import { damagedView, documentView, fakeDocuments } from "./fixtures";

const FAMILY = documentView().page.familyId;
const master = () => screen.getByRole("region", { name: "Resume.docx" });

beforeEach(() => {
  window.location.hash = `#/d/${FAMILY}/versions`;
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("a document's page", () => {
  it("says it is reading while the history loads", () => {
    const documents = fakeDocuments({ open: vi.fn(() => new Promise<never>(() => undefined)) });
    render(<DocumentScreen documents={documents} family={FAMILY} tab="versions" />);
    expect(screen.getByRole("status").textContent).toBe("Reading this document's history…");
  });

  it("shows each file's checkpoints, newest first", async () => {
    render(<DocumentScreen documents={fakeDocuments()} family={FAMILY} tab="versions" />);
    expect(await screen.findByRole("heading", { name: "Resume", level: 1 })).toBeTruthy();
    const labels = within(master())
      .getAllByRole("listitem")
      .map((li) => li.querySelector("strong")?.textContent);
    expect(labels).toEqual(["v3", "v2", "v1"]);
    const data = screen.getByRole("region", { name: "Resume – data roles.docx" });
    expect(within(data).getAllByText("Made from master v1")).toHaveLength(2);
    expect(within(data).getByText("Sent to Northgate Analytics")).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Versions" }).getAttribute("aria-selected")).toBe(
      "true",
    );
  });

  it("compares any two checkpoints", async () => {
    const documents = fakeDocuments();
    render(<DocumentScreen documents={documents} family={FAMILY} tab="versions" />);
    await screen.findByRole("heading", { name: "Resume", level: 1 });
    expect(screen.getByText("Tick two checkpoints to see what changed between them.")).toBeTruthy();
    fireEvent.click(within(master()).getByRole("checkbox", { name: "Compare Resume v1" }));
    fireEvent.click(within(master()).getByRole("checkbox", { name: "Compare Resume v3" }));
    fireEvent.click(screen.getByRole("button", { name: "Compare Resume v1 with Resume v3" }));
    expect(await screen.findByText("6 changes from Resume v1 to Resume v3")).toBeTruthy();
    const page = documentView().page;
    expect(documents.compare).toHaveBeenCalledWith(
      FAMILY,
      page.files[0]?.entries[2]?.id,
      page.files[0]?.entries[0]?.id,
    );
    expect(document.querySelectorAll(".what ins").length).toBeGreaterThan(0);
    expect(screen.getByText("1 formatting change")).toBeTruthy();
  });

  it("restores a checkpoint as a new version, and says nothing else changed", async () => {
    const documents = fakeDocuments();
    render(<DocumentScreen documents={documents} family={FAMILY} tab="versions" />);
    await screen.findByRole("heading", { name: "Resume", level: 1 });
    fireEvent.click(
      within(master()).getByRole("button", { name: "Restore as a new version from Resume v1" }),
    );
    const name = await screen.findByLabelText("Name of the new version");
    await waitFor(() => {
      expect((name as HTMLInputElement).value).toBe("Resume (v1)");
    });
    fireEvent.change(name, { target: { value: "First draft" } });
    fireEvent.click(screen.getByRole("button", { name: "Save the new version" }));
    expect(
      (await screen.findByText(/Saved “First draft.docx” as a new tailored version/)).textContent,
    ).toContain("Nothing else was changed.");
    expect(documents.restoreAsNewVersion).toHaveBeenCalledWith(
      FAMILY,
      documentView().page.files[0]?.entries[2]?.id,
      "First draft",
    );
    await waitFor(() => {
      expect(documents.open).toHaveBeenCalledTimes(2);
    });
  });

  it("marks, changes and removes a milestone", async () => {
    const view = documentView();
    const first = view.page.files[0]?.entries[1];
    if (first) {
      view.page.files[0]?.entries.splice(1, 1, { ...first, milestone: "Sent to ten companies" });
    }
    const documents = fakeDocuments({
      open: vi.fn(() => Promise.resolve({ view, ledger: new Uint8Array() })),
    });
    render(<DocumentScreen documents={documents} family={FAMILY} tab="versions" />);
    await screen.findByRole("heading", { name: "Resume", level: 1 });
    expect(within(master()).getByText("Sent to ten companies")).toBeTruthy();
    fireEvent.click(
      within(master()).getByRole("button", { name: "Change milestone on Resume v2" }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "Remove milestone" }));
    await waitFor(() => {
      expect(documents.markMilestone).toHaveBeenCalledWith(FAMILY, first?.id, null);
    });
    fireEvent.click(
      within(master()).getByRole("button", { name: "Mark as milestone on Resume v1" }),
    );
    fireEvent.change(await screen.findByLabelText("Milestone"), {
      target: { value: "First sent" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save milestone" }));
    await waitFor(() => {
      expect(documents.markMilestone).toHaveBeenLastCalledWith(
        FAMILY,
        view.page.files[0]?.entries[2]?.id,
        "First sent",
      );
    });
  });

  it("says when a file isn't found, and relinks it to the file the person picks", async () => {
    const page = documentView().page;
    const platform = page.files[2];
    const documents = fakeDocuments({
      missingFiles: vi.fn(() =>
        Promise.resolve([
          {
            node: platform?.node ?? "",
            name: platform?.name ?? "",
            storageRef: platform?.storageRef ?? "",
          },
        ]),
      ),
      unlinkedFiles: vi.fn(() => Promise.resolve(["Platform (renamed).docx"])),
    });
    render(<DocumentScreen documents={documents} family={FAMILY} tab="versions" />);
    const missing = await screen.findByRole("group", {
      name: "Resume – platform roles.docx is not found",
    });
    expect(within(missing).getByText(/Its history is safe/)).toBeTruthy();
    fireEvent.click(within(missing).getByRole("button", { name: "Choose the file" }));
    fireEvent.click(await within(missing).findByRole("radio", { name: "Platform (renamed).docx" }));
    fireEvent.click(within(missing).getByRole("button", { name: "Link this file" }));
    expect(await screen.findByText(/is linked to “Platform \(renamed\).docx” now/)).toBeTruthy();
    expect(documents.relink).toHaveBeenCalledWith(
      FAMILY,
      platform?.node,
      "Platform (renamed).docx",
    );
  });

  it("shows a damaged history from what can be read, and offers nothing to change", async () => {
    const view = damagedView();
    const documents = fakeDocuments({
      open: vi.fn(() => Promise.resolve({ view, ledger: new Uint8Array() })),
    });
    render(<DocumentScreen documents={documents} family={FAMILY} tab="versions" />);
    expect(
      await screen.findByText(
        "Lineage could read this history up to 15 Oct. 1 later entry is damaged, so nothing will be added to it.",
      ),
    ).toBeTruthy();
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.queryByRole("button", { name: /Restore/ })).toBeNull();
    expect(documents.missingFiles).not.toHaveBeenCalled();
  });

  it("says plainly when the document can't be opened", async () => {
    const documents = fakeDocuments({
      open: vi.fn(() => Promise.reject(new NotFound("family"))),
    });
    render(<DocumentScreen documents={documents} family={FAMILY} tab="versions" />);
    expect((await screen.findByRole("alert")).textContent).toContain(
      "Lineage couldn't find that file. It may have been moved or deleted.",
    );
  });

  it("opens a sent copy exactly as sent", async () => {
    const created = vi.fn(() => "blob:copy");
    URL.createObjectURL = created;
    URL.revokeObjectURL = vi.fn();
    const clicked = vi
      .spyOn(HTMLAnchorElement.prototype, "click")
      .mockImplementation(() => undefined);
    const documents = fakeDocuments();
    render(<DocumentScreen documents={documents} family={FAMILY} tab="sent" />);
    expect(await screen.findByText("Each copy is kept exactly as sent.")).toBeTruthy();
    const row = screen.getByText("Northgate Analytics").closest("li");
    expect(row && within(row).getByText("Came back with 1 change")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open as sent to Northgate Analytics" }));
    await waitFor(() => {
      expect(clicked).toHaveBeenCalled();
    });
    const sent = documentView()
      .page.sent.groups.flatMap((g) => g.rows)
      .find((r) => r.recipient === "Northgate Analytics");
    expect(documents.sentCopy).toHaveBeenCalledWith(FAMILY, sent?.id);
    const anchor = clicked.mock.contexts[0] as HTMLAnchorElement;
    expect(anchor.download).toBe("Resume – data roles v3 (Northgate Analytics).docx");
  });

  it("moves between tabs with the arrow keys", async () => {
    render(<DocumentScreen documents={fakeDocuments()} family={FAMILY} tab="versions" />);
    const versions = await screen.findByRole("tab", { name: "Versions" });
    fireEvent.keyDown(versions, { key: "ArrowRight" });
    expect(window.location.hash).toBe(`#/d/${FAMILY}/sent`);
    fireEvent.keyDown(versions, { key: "End" });
    expect(window.location.hash).toBe(`#/d/${FAMILY}/tree`);
  });

  it("walks the family tree by keyboard and shows each point", async () => {
    render(<DocumentScreen documents={fakeDocuments()} family={FAMILY} tab="tree" />);
    const tree = await screen.findByRole("group", { name: "Family tree" });
    const buttons = within(tree).getAllByRole("button");
    expect(buttons.filter((b) => b.tabIndex === 0)).toHaveLength(1);
    const first = buttons[0];
    if (!first) {
      throw new Error("no points");
    }
    fireEvent.click(first);
    expect(screen.getByRole("region", { name: "Details" }).textContent).toContain(
      "Started tracking",
    );
    fireEvent.keyDown(first, { key: "ArrowDown" });
    const focused = document.activeElement as HTMLElement;
    expect(focused.getAttribute("aria-label")).toMatch(/^Resume – data roles.docx v1/);
    expect(screen.getByRole("region", { name: "Details" }).textContent).toContain(
      "Made from master v1",
    );
    fireEvent.keyDown(focused, { key: "ArrowRight" });
    expect((document.activeElement as HTMLElement).getAttribute("aria-label")).toMatch(
      /^Resume – data roles.docx v2/,
    );
  });
});
