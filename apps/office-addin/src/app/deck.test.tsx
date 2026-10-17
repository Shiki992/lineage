// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The pitch deck panel and the report after "Update N versions", drawn from
// the engine's golden replies and found by role and accessible name.

import { readFileSync } from "node:fs";
import path from "node:path";
import type { ChangeCard, DeckPanel, UpdateRun, Updates, VersionPanel } from "@lineage/shell";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DeckHome, DeckVersionReview, UpdateReport } from "./deck";
import { VersionHome } from "./home";

function golden(name: string): unknown {
  const file = path.resolve(process.cwd(), "../../packages/shell/test/golden", `${name}.json`);
  return (JSON.parse(readFileSync(file, "utf8")) as { value: unknown }).value;
}

describe("the pitch deck panel", () => {
  it("says what just changed, with the figures marked, and updates the ticked versions", () => {
    const panel = golden("deck-panel") as DeckPanel;
    const onUpdate = vi.fn();
    const onOpenSent = vi.fn();
    const onCheck = vi.fn();
    render(
      <DeckHome
        panel={panel}
        busy={false}
        onUpdate={onUpdate}
        onCheck={onCheck}
        onOpen={vi.fn()}
        onOpenSent={onOpenSent}
        onAction={vi.fn()}
        onSent={vi.fn()}
      />,
    );
    expect(screen.getByText("Master deck")).toBeTruthy();
    const changed = screen.getByRole("region", { name: "You just changed Slide 6 · Traction" });
    expect(within(changed).getByText("'$1.2M' changed to '$2.0M'")).toBeTruthy();
    expect(within(changed).getAllByText("1.2M")[0]?.tagName).toBe("DEL");
    expect(within(changed).getAllByText("2.0M")[0]?.tagName).toBe("INS");
    // More edits can follow: PowerPoint has no change event, so the person can
    // always ask for a fresh look (found in the PowerPoint run).
    fireEvent.click(screen.getByRole("button", { name: "Check for changes" }));
    expect(onCheck).toHaveBeenCalled();

    const boxes = screen.getAllByRole("checkbox");
    expect(boxes).toHaveLength(2);
    expect(screen.getByText("Last sent 15 Oct, before this change")).toBeTruthy();
    // Untick the second version: the button says so.
    const second = boxes[1];
    if (!second) {
      throw new Error("two versions to tick");
    }
    fireEvent.click(second);
    fireEvent.click(screen.getByRole("button", { name: "Update 1 version" }));
    expect(onUpdate).toHaveBeenCalledWith([panel.targets[0]?.id], "Changed Slide 6 · Traction");

    const sent = screen.getByRole("region", { name: "Last copy sent" });
    expect(within(sent).getByText(/^Sent to Pellow Ventures on /)).toBeTruthy();
    fireEvent.click(within(sent).getByRole("button", { name: "Open what they saw" }));
    expect(onOpenSent).toHaveBeenCalledWith(panel.lastSent?.sent);
    // Decks don't offer "A file came back" yet.
    expect(screen.queryByRole("button", { name: "A file came back" })).toBeNull();
  });

  it("reports each version, and asks about what both changed", () => {
    const run = golden("update-versions") as UpdateRun;
    const onChoose = vi.fn();
    render(<UpdateReport run={run} busy={false} onChoose={onChoose} onDone={vi.fn()} />);
    expect(screen.getByRole("status").textContent).toBe("Updated all 2 versions");
    const lines = within(screen.getByRole("list", { name: "Versions" })).getAllByRole("heading", {
      level: 2,
    });
    expect(lines.map((h) => h.textContent)).toEqual([
      "Pitch deck – Seed fund.pptx",
      "Pitch deck – Northgate.pptx",
    ]);
    expect(screen.getByText("Brought in 1 change · 1 to choose")).toBeTruthy();
    expect(screen.getByText("Both changed '$1.2M ARR'")).toBeTruthy();
    const apply = screen.getByRole("button", { name: "Apply my choices" });
    expect((apply as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Use the master's" }));
    fireEvent.click(apply);
    const northgate = run.versions[1];
    expect(onChoose).toHaveBeenCalledWith(
      [northgate?.node],
      [
        {
          node: northgate?.node,
          card: northgate?.left.overlaps[0]?.id,
          decision: "useMasters",
        },
      ],
    );
  });

  it("offers Review on a tailored deck PowerPoint can change", () => {
    const panel = {
      ...(golden("version-panel-updates") as VersionPanel),
      fileName: "Pitch deck – Seed fund.pptx",
    };
    const onReview = vi.fn();
    render(
      <VersionHome
        panel={panel}
        busy={false}
        deck
        bringsIn
        onReview={onReview}
        onAction={vi.fn()}
        onSent={vi.fn()}
      />,
    );
    expect(screen.queryByText(/Close this deck/)).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Review them" }));
    expect(onReview).toHaveBeenCalled();
  });

  it("brings changes into the open deck, with Keep mine or Use the master's", () => {
    const updates = golden("updates-deck") as Updates;
    const onBringIn = vi.fn();
    render(
      <DeckVersionReview
        updates={updates}
        node="0123456789abcdef0123456789abcdef"
        busy={false}
        onBringIn={onBringIn}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByRole("heading", { level: 1 }).textContent).toBe(updates.headline);
    expect(screen.getByText("Both changed '$1.2M ARR'")).toBeTruthy();
    // Only the overlap is left: nothing to bring in until a side is chosen.
    expect(screen.queryByRole("button", { name: /^Bring in/ })).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Keep mine" }));
    expect(screen.getByRole("status").textContent).toBe("Nothing to bring in.");
    fireEvent.click(screen.getByRole("button", { name: "Use the master's" }));
    expect(screen.getByRole("status").textContent).toBe("1 change to bring in.");
    fireEvent.click(screen.getByRole("button", { name: "Bring in 1 change" }));
    const overlap = updates.groups[0]?.view.overlaps[0];
    expect(onBringIn).toHaveBeenCalledWith([
      { node: "0123456789abcdef0123456789abcdef", card: overlap?.id, decision: "useMasters" },
    ]);
  });

  it("brings in what only the master changed, and says what waits", () => {
    const base = golden("updates-deck") as Updates;
    const group = base.groups[0];
    const overlap = group?.view.overlaps[0];
    if (!group || !overlap) {
      throw new Error("an overlap in the golden reply");
    }
    const change: ChangeCard = {
      id: "c1",
      summary: "The notes changed",
      place: "Slide 6 · Traction",
      text: [],
      choices: [],
      byHand: null,
    };
    const updates: Updates = {
      ...base,
      groups: [{ ...group, view: { ...group.view, changes: [change] } }],
    };
    const onBringIn = vi.fn();
    render(
      <DeckVersionReview
        updates={updates}
        node="0123456789abcdef0123456789abcdef"
        busy={false}
        onBringIn={onBringIn}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByRole("status").textContent).toBe(
      "1 change to bring in. 1 change you both made waits until you choose.",
    );
    fireEvent.click(screen.getByRole("button", { name: "Bring in 1 change" }));
    expect(onBringIn).toHaveBeenCalledWith([]);
  });

  it("tells a tailored deck where its updates come from", () => {
    const panel = {
      ...(golden("version-panel-updates") as VersionPanel),
      fileName: "Pitch deck – Seed fund.pptx",
    };
    render(
      <VersionHome
        panel={panel}
        busy={false}
        deck
        onReview={vi.fn()}
        onAction={vi.fn()}
        onSent={vi.fn()}
      />,
    );
    expect(screen.queryByRole("button", { name: "Review them" })).toBeNull();
    expect(
      screen.getByText(/^PowerPoint can.t change a deck while it.s open. Close this deck/),
    ).toBeTruthy();
  });
});
