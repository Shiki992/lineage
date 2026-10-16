// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// My documents: every state the list can be in.

import { fireEvent, render, screen, within } from "@testing-library/react";
import { StorageError } from "@lineage/shell";
import { describe, expect, it, vi } from "vitest";
import { fakeDocuments, libraryView } from "./fixtures";
import { LibraryScreen, arrange } from "./library";

const names = () =>
  within(screen.getByRole("list", { name: "Documents" }))
    .getAllByRole("link")
    .map((a) => a.textContent);

describe("My documents", () => {
  it("says it is reading while the history loads", () => {
    const documents = fakeDocuments({ list: vi.fn(() => new Promise<never>(() => undefined)) });
    render(<LibraryScreen documents={documents} where="your OneDrive" />);
    expect(screen.getByRole("status").textContent).toBe("Reading your history from your OneDrive…");
  });

  it("explains how to start when nothing is tracked", async () => {
    const documents = fakeDocuments({
      list: vi.fn(() =>
        Promise.resolve({ entries: [], empty: "You don't track any documents yet." }),
      ),
    });
    render(<LibraryScreen documents={documents} where="your OneDrive" />);
    expect(await screen.findByRole("heading", { name: "Nothing tracked yet" })).toBeTruthy();
    expect(screen.getByText("Choose Start tracking. It appears here.")).toBeTruthy();
  });

  it("lists every family, newest first, with what each holds", async () => {
    render(<LibraryScreen documents={fakeDocuments()} where="your OneDrive" />);
    await screen.findByText("3 documents");
    const rows = names();
    expect(rows[0]).toContain("Cover letter");
    expect(rows[1]).toContain("Resume");
    expect(rows[1]).toContain("2 tailored versions · 8 checkpoints · 3 sent · 2 came back");
    expect(rows[1]).toContain("1 file came back with changes to review");
    expect(rows[2]).toContain("PowerPoint deck");
    const link = screen.getByRole("link", { name: /Resume/ });
    expect(link.getAttribute("href")).toBe("#/d/119736279a3062acd1d1873c28a849ee/versions");
    expect(screen.getAllByRole("img", { name: "Word" })).toHaveLength(2);
    expect(screen.getByRole("img", { name: "PowerPoint" })).toBeTruthy();
  });

  it("searches, filters by format and sorts", async () => {
    render(<LibraryScreen documents={fakeDocuments()} where="your OneDrive" />);
    await screen.findByText("3 documents");
    fireEvent.change(screen.getByLabelText("Search"), { target: { value: "cover" } });
    expect(names()).toHaveLength(1);
    expect(screen.getByText("1 of 3 documents")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Search"), { target: { value: "nothing like it" } });
    expect(screen.getByText("No documents match “nothing like it”.")).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Search"), { target: { value: "" } });
    fireEvent.change(screen.getByLabelText("Format"), { target: { value: "pptx" } });
    expect(names()).toHaveLength(1);
    expect(names()[0]).toContain("Pitch deck");
    fireEvent.change(screen.getByLabelText("Format"), { target: { value: "all" } });
    fireEvent.change(screen.getByLabelText("Sort by"), { target: { value: "name" } });
    expect(names().map((n) => /^[WPX](Cover letter|Pitch deck|Resume)/.exec(n)?.[1])).toEqual([
      "Cover letter",
      "Pitch deck",
      "Resume",
    ]);
  });

  it("draws from the index first, then from the checked ledgers", async () => {
    let finish: (l: ReturnType<typeof libraryView>) => void = () => undefined;
    const documents = fakeDocuments({
      list: vi.fn((first?: (l: ReturnType<typeof libraryView>) => void) => {
        first?.({ ...libraryView(), entries: libraryView().entries.slice(0, 1) });
        return new Promise<ReturnType<typeof libraryView>>((resolve) => {
          finish = resolve;
        });
      }),
    });
    render(<LibraryScreen documents={documents} where="your OneDrive" />);
    expect(await screen.findByText("Checking for changes…")).toBeTruthy();
    expect(names()).toHaveLength(1);
    finish(libraryView());
    expect(await screen.findByText("3 documents")).toBeTruthy();
  });

  it("says what went wrong, and tries again", async () => {
    const list = vi
      .fn()
      .mockRejectedValueOnce(
        new StorageError("OneDrive is busy right now. Try again in a minute.", "429"),
      )
      .mockResolvedValueOnce(libraryView());
    render(<LibraryScreen documents={fakeDocuments({ list })} where="your OneDrive" />);
    expect((await screen.findByRole("alert")).textContent).toContain(
      "OneDrive is busy right now. Try again in a minute.",
    );
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("3 documents")).toBeTruthy();
  });

  it("flags a damaged history in its row", async () => {
    const damaged = libraryView();
    const first = damaged.entries[0];
    if (first) {
      damaged.entries[0] = {
        ...first,
        damage: "Lineage could read this history up to 14 Oct.",
        canOpen: false,
      };
    }
    render(
      <LibraryScreen
        documents={fakeDocuments({ list: vi.fn(() => Promise.resolve(damaged)) })}
        where="your OneDrive"
      />,
    );
    expect(await screen.findByText("Lineage could read this history up to 14 Oct.")).toBeTruthy();
  });

  it("arranges without changing the list it was given", () => {
    const entries = libraryView().entries;
    const before = entries.map((e) => e.name);
    arrange(entries, "", "all", "name");
    expect(entries.map((e) => e.name)).toEqual(before);
  });
});
