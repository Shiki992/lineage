// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Every screen, drawn from the engine's golden replies (packages/shell/test/
// golden), found by role and accessible name the way a screen reader would.

import { readFileSync } from "node:fs";
import path from "node:path";
import type {
  Family,
  MasterPanel,
  SentList,
  Updates,
  VersionPanel,
  WhatChanged,
} from "@lineage/shell";
import { fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { UpdatesView, WhatChangedView } from "./changes";
import { MakeVersion, SaveCheckpoint, SendCopy } from "./forms";
import { MasterHome, StartTracking, VersionHome } from "./home";
import { WhatIveSent } from "./sent";

function golden(name: string): unknown {
  // Tests run from apps/office-addin; jsdom has no file URL for this module.
  const file = path.resolve(process.cwd(), "../../packages/shell/test/golden", `${name}.json`);
  return (JSON.parse(readFileSync(file, "utf8")) as { value: unknown }).value;
}

describe("the master panel", () => {
  it("shows the call-out, the versions and the actions", () => {
    const panel = golden("master-panel-callout") as MasterPanel;
    const onOffer = vi.fn();
    const onOpen = vi.fn();
    const onAction = vi.fn();
    const onSent = vi.fn();
    render(
      <MasterHome
        panel={panel}
        busy={false}
        onOffer={onOffer}
        onOpen={onOpen}
        onAction={onAction}
        onSent={onSent}
      />,
    );
    expect(screen.getByRole("heading", { level: 1, name: "Resume.docx" })).toBeTruthy();
    expect(screen.getByText("Master")).toBeTruthy();
    expect(screen.getByText("2 tailored versions follow this file")).toBeTruthy();
    expect(screen.getByText(panel.callout?.text ?? "missing")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Offer them to both versions" }));
    expect(onOffer).toHaveBeenCalledWith(panel.callout?.versions);
    fireEvent.click(screen.getByRole("button", { name: "Open Resume – data roles.docx" }));
    expect(onOpen).toHaveBeenCalledWith("Resume – data roles.docx");
    fireEvent.click(screen.getByRole("button", { name: "Make a tailored version" }));
    fireEvent.click(screen.getByRole("button", { name: "Save checkpoint" }));
    fireEvent.click(screen.getByRole("button", { name: "Send a copy" }));
    expect(onAction.mock.calls.map((c) => c[0] as string)).toEqual([
      "makeVersion",
      "saveCheckpoint",
      "sendCopy",
    ]);
    fireEvent.click(screen.getByRole("button", { name: "What I've sent" }));
    expect(onSent).toHaveBeenCalled();
  });

  it("has no call-out or list before there are versions, and disables buttons while busy", () => {
    const panel = golden("master-panel-new") as MasterPanel;
    render(
      <MasterHome
        panel={panel}
        busy
        onOffer={vi.fn()}
        onOpen={vi.fn()}
        onAction={vi.fn()}
        onSent={vi.fn()}
      />,
    );
    expect(screen.getByText("No tailored versions yet")).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Tailored versions" })).toBeNull();
    expect(screen.queryByLabelText("Changes to offer")).toBeNull();
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Save checkpoint" }).disabled,
    ).toBe(true);
  });
});

describe("a tailored version's panel", () => {
  it("offers to review updates when there are some", () => {
    const panel = golden("version-panel-updates") as VersionPanel;
    const onReview = vi.fn();
    render(
      <VersionHome
        panel={panel}
        busy={false}
        onReview={onReview}
        onAction={vi.fn()}
        onSent={vi.fn()}
      />,
    );
    expect(screen.getByText("Tailored version")).toBeTruthy();
    expect(screen.getByText(panel.updates ?? "missing")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Review them" }));
    expect(onReview).toHaveBeenCalled();
    expect(screen.queryByRole("button", { name: "Make a tailored version" })).toBeNull();
  });

  it("is quiet when it is up to date", () => {
    render(
      <VersionHome
        panel={golden("version-panel") as VersionPanel}
        busy={false}
        onReview={vi.fn()}
        onAction={vi.fn()}
        onSent={vi.fn()}
      />,
    );
    expect(screen.queryByRole("button", { name: "Review them" })).toBeNull();
    expect(screen.getByText("Made from master v1 · has v3")).toBeTruthy();
  });
});

describe("updates from the master", () => {
  const updates = golden("updates") as Updates;

  it("marks removed and added text, shows both sides of an overlap and why some come by hand", () => {
    render(<UpdatesView updates={updates} busy={false} onApply={vi.fn()} onBack={vi.fn()} />);
    expect(screen.getByRole("heading", { level: 1, name: updates.headline })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "From master v2" })).toBeTruthy();
    expect(document.querySelectorAll(".what ins").length).toBeGreaterThan(0);
    expect(document.querySelectorAll(".what del").length).toBeGreaterThan(0);
    expect(screen.getByText("Yours")).toBeTruthy();
    expect(screen.getByText("The master's")).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Copy these in by hand" })).toBeTruthy();
    expect(
      screen.getByText("This changes page layout or sections. Copy it in by hand for now."),
    ).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Formatting only" })).toBeTruthy();
  });

  it("applies only what the person chose", () => {
    const onApply = vi.fn();
    render(<UpdatesView updates={updates} busy={false} onApply={onApply} onBack={vi.fn()} />);
    const apply = screen.getByRole<HTMLButtonElement>("button", { name: "Apply" });
    expect(apply.disabled).toBe(true);
    const job = updates.groups[0]?.view.changes[0];
    const card = screen
      .getByRole("heading", { name: job?.summary ?? "missing" })
      .closest("li") as HTMLElement;
    const bringIn = within(card).getByRole("button", { name: "Bring in" });
    fireEvent.click(bringIn);
    expect(bringIn.getAttribute("aria-pressed")).toBe("true");
    const overlap = screen.getByText("Yours").closest("li") as HTMLElement;
    fireEvent.click(within(overlap).getByRole("button", { name: "Keep mine" }));
    expect(screen.getByRole("status").textContent).toContain(
      "1 change to bring in, 1 to leave out",
    );
    fireEvent.click(apply);
    expect(onApply).toHaveBeenCalledWith([
      { card: job?.id, decision: "bringIn" },
      { card: updates.groups[1]?.view.overlaps[0]?.id, decision: "keepMine" },
    ]);
    // Pressing a choice again takes it back.
    fireEvent.click(bringIn);
    expect(bringIn.getAttribute("aria-pressed")).toBe("false");
  });
});

describe("save checkpoint", () => {
  it("shows what changed and saves with a note", () => {
    const changes = golden("what-changed") as WhatChanged;
    const onSave = vi.fn();
    render(<SaveCheckpoint changes={changes} busy={false} onSave={onSave} onBack={vi.fn()} />);
    expect(screen.getByText(changes.headline)).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Note (optional)"), {
      target: { value: "Corvia title" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save checkpoint" }));
    expect(onSave).toHaveBeenCalledWith("Corvia title");
  });

  it("can't save before it knows what changed", () => {
    render(<SaveCheckpoint changes={null} busy={false} onSave={vi.fn()} onBack={vi.fn()} />);
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Save checkpoint" }).disabled,
    ).toBe(true);
  });

  it("lists formatting apart from content", () => {
    render(<WhatChangedView view={golden("what-changed") as WhatChanged} />);
    expect(screen.getByRole("heading", { name: "Formatting only" })).toBeTruthy();
  });
});

describe("make a tailored version", () => {
  it("asks for a real name", () => {
    const onMake = vi.fn();
    render(
      <MakeVersion
        masterName="Resume.docx"
        latest="v3"
        busy={false}
        onMake={onMake}
        onBack={vi.fn()}
      />,
    );
    const name = screen.getByLabelText<HTMLInputElement>("Name");
    expect(name.value).toBe("Resume – ");
    fireEvent.click(screen.getByRole("button", { name: "Make a tailored version" }));
    expect(onMake).not.toHaveBeenCalled();
    expect(name.getAttribute("aria-invalid")).toBe("true");
    expect(screen.getByText(/Give the tailored version a name/)).toBeTruthy();
    fireEvent.change(name, { target: { value: "Resume – data roles" } });
    fireEvent.click(screen.getByRole("button", { name: "Make a tailored version" }));
    expect(onMake).toHaveBeenCalledWith("Resume – data roles");
  });
});

describe("send a copy", () => {
  const family = golden("family") as Family;
  const master = family.nodes[0]?.id ?? "";

  it("needs to know who it's for, and sends a clean Word copy with the id by default", () => {
    const onSend = vi.fn();
    render(
      <SendCopy family={family} openNode={master} busy={false} onSend={onSend} onBack={vi.fn()} />,
    );
    expect(screen.getAllByRole("radio")).toHaveLength(3);
    expect(screen.getByText("PDF is coming later.")).toBeTruthy();
    expect(screen.getByText(/Always a clean copy/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Save copy and record it" }));
    expect(onSend).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Who it's for").getAttribute("aria-invalid")).toBe("true");
    fireEvent.change(screen.getByLabelText("Who it's for"), { target: { value: " Northgate " } });
    fireEvent.click(screen.getByRole("radio", { name: /data roles/ }));
    fireEvent.click(screen.getByRole("checkbox", { name: /Lineage ID/ }));
    fireEvent.click(screen.getByRole("button", { name: "Save copy and record it" }));
    expect(onSend).toHaveBeenCalledWith(family.nodes[1]?.id, {
      recipient: "Northgate",
      purpose: null,
      embedId: false,
    });
  });
});

describe("what I've sent", () => {
  it("lists copies by month and opens exactly what was sent", () => {
    const list = golden("sent-list") as SentList;
    const onOpen = vi.fn();
    const onSearch = vi.fn();
    render(
      <WhatIveSent list={list} busy={false} onSearch={onSearch} onOpen={onOpen} onBack={vi.fn()} />,
    );
    expect(screen.getByRole("heading", { name: "October 2026" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open what Northgate Analytics received" }));
    expect(onOpen).toHaveBeenCalledWith(list.groups[0]?.rows[0]?.id);
    fireEvent.change(screen.getByLabelText("Search what you've sent"), {
      target: { value: "north" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Search" }));
    expect(onSearch).toHaveBeenCalledWith("north");
    expect(screen.getByText("Each copy is kept exactly as sent.")).toBeTruthy();
  });

  it("says when nothing has been sent", () => {
    render(
      <WhatIveSent
        list={golden("sent-list-empty") as SentList}
        busy={false}
        onSearch={vi.fn()}
        onOpen={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByText(/Nothing sent yet/)).toBeTruthy();
  });
});

describe("start tracking", () => {
  it("asks for an unsaved file to be saved first", () => {
    render(
      <StartTracking
        name="Document1"
        saved={false}
        storage="OneDrive"
        busy={false}
        onStart={vi.fn()}
      />,
    );
    const button = screen.getByRole<HTMLButtonElement>("button", { name: "Start tracking" });
    expect(button.disabled).toBe(true);
    expect(button.getAttribute("aria-describedby")).toBe("save-first");
  });
});
