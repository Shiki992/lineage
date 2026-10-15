// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The task pane end to end in jsdom: the real wasm engine, memory storage and a
// document host that behaves like Word, clicked through the way a person would.

import { readFileSync } from "node:fs";
import path from "node:path";
import { MemoryStorage, loadEngine } from "@lineage/shell";
import { FakeHost } from "@lineage/shell/testing";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "./App";

// Tests run from apps/office-addin; jsdom has no file URL for this module.
const repo = (...parts: string[]) => path.resolve(process.cwd(), "../..", ...parts);
const wasm = readFileSync(repo("packages/shell/wasm-pkg/lineage_wasm_bg.wasm"));
const fixture = (name: string) =>
  new Uint8Array(readFileSync(repo("fixtures/docx/synthetic", name)));

describe("the task pane", () => {
  it("tracks a resume, makes a version, offers a change and sends a copy", async () => {
    const engine = loadEngine(wasm, new Uint8Array(32).fill(9));
    const storage = new MemoryStorage();
    const host = new FakeHost(engine, fixture("resume-master.docx"), {
      name: "Resume.docx",
      url: "https://d.docs.live.net/0/Documents/Resume.docx",
    });
    render(<App engine={engine} host={host} storage={{ kind: "ready", storage }} />);

    fireEvent.click(await screen.findByRole("button", { name: "Start tracking" }));
    expect(await screen.findByText("Lineage is tracking this file. This is v1.")).toBeTruthy();
    expect(screen.getByText("No tailored versions yet")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Make a tailored version" }));
    fireEvent.change(screen.getByLabelText("Name"), { target: { value: "Resume – data roles" } });
    fireEvent.click(screen.getByRole("button", { name: "Make a tailored version" }));
    expect(await screen.findByText("Made “Resume – data roles.docx”.")).toBeTruthy();
    expect(screen.getByText("1 tailored version follows this file")).toBeTruthy();

    host.edit(fixture("resume-master-new-job.docx"));
    fireEvent.click(screen.getByRole("button", { name: "Save checkpoint" }));
    expect(await screen.findByText("2 changes since v1")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Save checkpoint" }));
    expect(await screen.findByText("Checkpoint saved.")).toBeTruthy();
    fireEvent.click(
      await screen.findByRole("button", { name: "Offer them to your tailored version" }),
    );
    expect(await screen.findByText(/^Offered\./)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Send a copy" }));
    fireEvent.change(await screen.findByLabelText("Who it's for"), { target: { value: "Mentor" } });
    fireEvent.click(screen.getByRole("button", { name: "Save copy and record it" }));
    expect(await screen.findByText(/Saved “Resume \(Mentor\)\.docx” in memory/)).toBeTruthy();
    expect(await storage.fileNames("sent")).toEqual(["Resume (Mentor).docx"]);

    fireEvent.click(screen.getByRole("button", { name: "What I've sent" }));
    fireEvent.click(await screen.findByRole("button", { name: "Open what Mentor received" }));
    await waitFor(() => {
      expect(host.copies).toHaveLength(1);
    });
  });

  it("brings back a mentor's copy, one change at a time", async () => {
    const engine = loadEngine(wasm, new Uint8Array(32).fill(11));
    const storage = new MemoryStorage();
    const host = new FakeHost(engine, fixture("resume-master.docx"), {
      name: "Resume.docx",
      url: "https://d.docs.live.net/0/Documents/Resume.docx",
    });
    render(<App engine={engine} host={host} storage={{ kind: "ready", storage }} />);
    fireEvent.click(await screen.findByRole("button", { name: "Start tracking" }));
    await screen.findByText("Lineage is tracking this file. This is v1.");
    fireEvent.click(screen.getByRole("button", { name: "Send a copy" }));
    fireEvent.change(await screen.findByLabelText("Who it's for"), { target: { value: "Mentor" } });
    fireEvent.click(screen.getByRole("button", { name: "Save copy and record it" }));
    await screen.findByText(/Saved “Resume \(Mentor\)\.docx”/);

    // The mentor's edits, in a file that kept the copy's ids.
    const copy = await storage.readFile(storage.refFor("sent", "Resume (Mentor).docx"));
    const ids = engine.readIds(copy);
    expect(ids).not.toBeNull();
    const back = engine.writeIds(
      fixture("resume-tailored-data.docx"),
      ids ?? { family: "", checkpoint: "" },
    );

    fireEvent.click(screen.getByRole("button", { name: "A file came back" }));
    fireEvent.change(await screen.findByTestId("returned-file"), {
      target: {
        files: [new File([back as Uint8Array<ArrayBuffer>], "Resume - comments from Sam.docx")],
      },
    });
    expect(await screen.findByRole("heading", { name: "This came back from Mentor" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Review their changes" }));
    expect(await screen.findByText("Change 1 of 4")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    for (let i = 0; i < 3; i++) {
      fireEvent.click(await screen.findByRole("button", { name: "Skip" }));
    }
    fireEvent.click(await screen.findByRole("button", { name: "Apply" }));
    expect(await screen.findByText(/^Done\. The changes you kept are in your file/)).toBeTruthy();
    expect(host.replaced).toBe(1);
    expect(screen.getByText("Kept 1 change from Mentor")).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "What I've sent" }));
    expect(await screen.findByText("Reviewed")).toBeTruthy();
  });

  it("shows the engine's words when something can't be done", async () => {
    const engine = loadEngine(wasm, new Uint8Array(32).fill(10));
    const host = new FakeHost(engine, new TextEncoder().encode("not a Word file"), {
      name: "notes.docx",
      url: "https://d.docs.live.net/0/notes.docx",
    });
    render(
      <App engine={engine} host={host} storage={{ kind: "ready", storage: new MemoryStorage() }} />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Start tracking" }));
    expect((await screen.findByRole("alert")).textContent).toContain(
      "This file isn't a Word document Lineage can open.",
    );
  });
});
