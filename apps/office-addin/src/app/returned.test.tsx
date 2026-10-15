// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The screens for files that come back, drawn from the engine's golden replies
// and found by role and accessible name.

import { readFileSync } from "node:fs";
import path from "node:path";
import type { Recognition, ReturnReview, SentList, VersionPanel } from "@lineage/shell";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { StartTracking, VersionHome } from "./home";
import { CameBack, ChooseReturned, ReviewReturned } from "./returned";
import { WhatIveSent } from "./sent";

function golden(name: string): unknown {
  const file = path.resolve(process.cwd(), "../../packages/shell/test/golden", `${name}.json`);
  return (JSON.parse(readFileSync(file, "utf8")) as { value: unknown }).value;
}

describe("a file came back", () => {
  it("takes a .docx from the file picker, and refuses anything else", async () => {
    const onFile = vi.fn();
    render(<ChooseReturned busy={false} onFile={onFile} onBack={vi.fn()} />);
    expect(screen.getByRole("heading", { name: "A file came back" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Choose file" })).toBeTruthy();
    const input = screen.getByTestId("returned-file");
    fireEvent.change(input, { target: { files: [new File(["x"], "notes.pdf")] } });
    expect(screen.getByRole("alert").textContent).toMatch(/Choose a Word file/);
    fireEvent.change(input, {
      target: { files: [new File([new Uint8Array([1, 2, 3])], "Resume - Sam.docx")] },
    });
    await waitFor(() => {
      expect(onFile).toHaveBeenCalled();
    });
    const [bytes, name] = onFile.mock.calls[0] as [Uint8Array, string];
    expect(name).toBe("Resume - Sam.docx");
    expect([...bytes]).toEqual([1, 2, 3]);
  });

  it("takes a file dropped on the pane", async () => {
    const onFile = vi.fn();
    render(<ChooseReturned busy={false} onFile={onFile} onBack={vi.fn()} />);
    const zone = screen.getByText("Drop the file here, or").parentElement ?? document.body;
    fireEvent.drop(zone, {
      dataTransfer: { files: [new File([new Uint8Array([7])], "back.docx")] },
    });
    await waitFor(() => {
      expect(onFile).toHaveBeenCalledWith(expect.any(Uint8Array), "back.docx");
    });
  });

  it("confirms a sure match", () => {
    const recognition = golden("recognise-certain") as Recognition;
    const onConfirm = vi.fn();
    render(
      <CameBack
        name="Resume - comments.docx"
        recognition={recognition}
        busy={false}
        onConfirm={onConfirm}
        onBack={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("heading", { name: "This came back from Northgate Analytics" }),
    ).toBeTruthy();
    expect(screen.getByText("Recognised by its Lineage ID")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Review their changes" }));
    expect(onConfirm).toHaveBeenCalledWith(recognition.candidates[0], "ids");
  });

  it("asks which copy it is when Lineage isn't sure, never guessing", () => {
    const recognition = golden("recognise-choose") as Recognition;
    const onConfirm = vi.fn();
    const onBack = vi.fn();
    render(
      <CameBack
        name="back.docx"
        recognition={recognition}
        busy={false}
        onConfirm={onConfirm}
        onBack={onBack}
      />,
    );
    expect(screen.getByRole("heading", { name: "Which copy is this?" })).toBeTruthy();
    const next = screen.getByRole<HTMLButtonElement>("button", { name: "Continue" });
    expect(next.disabled).toBe(true);
    const radios = screen.getAllByRole("radio");
    expect(radios).toHaveLength(recognition.candidates.length + 1);
    fireEvent.click(screen.getByRole("radio", { name: /Harbour Bank/ }));
    fireEvent.click(screen.getByRole("button", { name: "Continue" }));
    const picked = recognition.candidates.find((c) => c.title === "Harbour Bank");
    expect(onConfirm).toHaveBeenCalledWith(picked, "person");
    fireEvent.click(screen.getByRole("radio", { name: "None of these" }));
    fireEvent.click(screen.getByRole("button", { name: "Choose another file" }));
    expect(onBack).toHaveBeenCalled();
  });

  it("says when it doesn't recognise the file", () => {
    render(
      <CameBack
        name="Budget.docx"
        recognition={golden("recognise-unknown") as Recognition}
        busy={false}
        onConfirm={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("heading", { name: "Lineage doesn't recognise this file" }),
    ).toBeTruthy();
    expect(screen.queryByRole("radio")).toBeNull();
  });
});

describe("reviewing a file that came back", () => {
  it("goes one change at a time, then applies the decisions", () => {
    const review = golden("returned-review") as ReturnReview;
    const onApply = vi.fn();
    render(
      <ReviewReturned
        review={review}
        isSource
        busy={false}
        onApply={onApply}
        onOpenSource={vi.fn()}
        onOpenReturned={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByRole("heading", { level: 1, name: review.headline })).toBeTruthy();
    expect(screen.getByText("Change 1 of 1")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /instead$/ })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect(screen.getByText("1 change to keep, 0 to skip.")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(onApply).toHaveBeenCalledWith([
      { card: review.view.changes[0]?.id, decision: "bringIn" },
    ]);
  });

  it("can leave a change for later, and offers the file it came from", () => {
    const review = golden("returned-review") as ReturnReview;
    const onOpenSource = vi.fn();
    render(
      <ReviewReturned
        review={review}
        isSource={false}
        busy={false}
        onApply={vi.fn()}
        onOpenSource={onOpenSource}
        onOpenReturned={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: `Open “${review.fromName}” instead` }));
    expect(onOpenSource).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Decide later →" }));
    const apply = screen.getByRole<HTMLButtonElement>("button", { name: "Apply" });
    expect(apply.disabled).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: "Go through the changes again" }));
    expect(screen.getByRole("button", { name: "Skip" })).toBeTruthy();
  });

  it("puts formatting changes in one step you can skip at once", () => {
    const base = golden("returned-review") as ReturnReview;
    const card = base.view.changes[0];
    if (!card) {
      throw new Error("no card");
    }
    const formatting = [
      { ...card, id: "cf1", summary: "'Summary': font changed", text: [] },
      { ...card, id: "cf2", summary: "'Skills': font changed", text: [] },
    ];
    const review: ReturnReview = {
      ...base,
      view: { ...base.view, changes: [], formatting },
    };
    const onApply = vi.fn();
    render(
      <ReviewReturned
        review={review}
        isSource
        busy={false}
        onApply={onApply}
        onOpenSource={vi.fn()}
        onOpenReturned={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("heading", { name: "They also changed formatting in 2 places" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Skip all formatting" }));
    fireEvent.click(screen.getByRole("button", { name: "Apply" }));
    expect(onApply).toHaveBeenCalledWith([
      { card: "cf1", decision: "decline" },
      { card: "cf2", decision: "decline" },
    ]);
  });
});

describe("where a returned file shows", () => {
  it("on the panel of the file it was made from", () => {
    const panel = golden("version-panel-returned") as VersionPanel;
    const onReview = vi.fn();
    render(
      <VersionHome
        panel={panel}
        busy={false}
        onReview={vi.fn()}
        onAction={vi.fn()}
        onSent={vi.fn()}
        onCameBack={vi.fn()}
        onReviewReturned={onReview}
      />,
    );
    const row = panel.returned[0];
    fireEvent.click(screen.getByRole("button", { name: `Review: ${row?.text ?? ""}` }));
    expect(onReview).toHaveBeenCalledWith(row?.id);
    expect(screen.getByRole("button", { name: "A file came back" })).toBeTruthy();
  });

  it("in What I've sent, with its chip and a Review button", () => {
    const list = golden("sent-list-returned") as SentList;
    const onReview = vi.fn();
    render(
      <WhatIveSent
        list={list}
        busy={false}
        onSearch={vi.fn()}
        onOpen={vi.fn()}
        onReview={onReview}
        onCameBack={vi.fn()}
        onBack={vi.fn()}
      />,
    );
    expect(screen.getByText("Came back with 1 change")).toBeTruthy();
    fireEvent.click(
      screen.getByRole("button", { name: "Review what Northgate Analytics sent back" }),
    );
    expect(onReview).toHaveBeenCalledWith(list.groups[0]?.rows[0]?.returned);
  });

  it("on the start screen, when an untracked file looks like a copy that came back", () => {
    const onCameBack = vi.fn();
    render(
      <StartTracking
        name="Avery Quinn resume.docx"
        saved
        storage="OneDrive"
        busy={false}
        onStart={vi.fn()}
        cameBack={golden("recognise-certain") as Recognition}
        onCameBack={onCameBack}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Review it as a file that came back" }));
    expect(onCameBack).toHaveBeenCalled();
    expect(screen.getByText(/from Northgate Analytics/)).toBeTruthy();
  });
});
