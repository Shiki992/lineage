// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The task pane end to end in jsdom: the real wasm engine, memory storage and a
// document host that behaves like Word, clicked through the way a person would.

import { readFileSync } from "node:fs";
import path from "node:path";
import { Lineage, MemoryStorage, loadEngine } from "@lineage/shell";
import { FakeHost } from "@lineage/shell/testing";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { App } from "./App";

// Tests run from apps/office-addin; jsdom has no file URL for this module.
const repo = (...parts: string[]) => path.resolve(process.cwd(), "../..", ...parts);
const wasm = readFileSync(repo("packages/shell/wasm-pkg/lineage_wasm_bg.wasm"));
const fixture = (name: string) =>
  new Uint8Array(readFileSync(repo("fixtures/docx/synthetic", name)));

describe("the task pane", () => {
  it("says when it reads the file as last saved, as in Excel on the web", async () => {
    const engine = loadEngine(wasm, new Uint8Array(32).fill(27));
    const storage = new MemoryStorage();
    const host = Object.assign(
      new FakeHost(engine, fixture("resume-master.docx"), {
        name: "Resume.docx",
        url: "https://d.docs.live.net/0/Documents/Resume.docx",
      }),
      { readsLastSaved: true },
    );
    render(<App engine={engine} host={host} storage={{ kind: "ready", storage }} />);
    expect(await screen.findByText(/reads this workbook as last saved in OneDrive/)).toBeTruthy();
  });

  it("shares a sent copy as a link, sees their edits, and stops sharing", async () => {
    const engine = loadEngine(wasm, new Uint8Array(32).fill(25));
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

    fireEvent.click(screen.getByRole("button", { name: "Share a link" }));
    expect(await screen.findByRole("heading", { name: "Share a link" })).toBeTruthy();
    fireEvent.click(screen.getByRole("radio", { name: /^Edit/ }));
    fireEvent.click(screen.getByRole("button", { name: "Make the link" }));
    expect(await screen.findByRole("heading", { name: "Link for Mentor" })).toBeTruthy();
    expect(screen.getByLabelText<HTMLInputElement>("Link").value).toBe(
      "https://share.example/link-1",
    );
    expect(screen.getByText("Anyone with the link can open it.")).toBeTruthy();

    // Mentor edits the shared copy through the link.
    const ref = "Sent copies/Resume (Mentor).docx";
    const ids = engine.readIds(storage.files.get(ref) ?? new Uint8Array());
    if (!ids) {
      throw new Error("the copy has ids");
    }
    storage.files.set(ref, engine.writeIds(fixture("resume-master-new-job.docx"), ids));
    fireEvent.click(screen.getByRole("button", { name: /Back/ }));
    expect(await screen.findByText("Mentor changed the shared copy.")).toBeTruthy();
    expect(screen.getByText("Shared as a link · Can edit")).toBeTruthy();

    // Stop sharing asks first.
    fireEvent.click(screen.getByRole("button", { name: "Stop sharing with Mentor" }));
    expect(screen.getByText(/The link stops working for everyone who has it/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Keep sharing" }));
    expect(storage.links.size).toBe(1);

    fireEvent.click(
      screen.getByRole("button", { name: "Review Mentor's changes to the shared copy" }),
    );
    expect(await screen.findByRole("heading", { name: "This came back from Mentor" })).toBeTruthy();

    // Back goes back to What I've sent.
    fireEvent.click(screen.getByRole("button", { name: /Back/ }));
    fireEvent.click(await screen.findByRole("button", { name: "Stop sharing with Mentor" }));
    fireEvent.click(screen.getByRole("button", { name: "Stop sharing" }));
    expect(await screen.findByText("Stopped. The link doesn't work any more.")).toBeTruthy();
    expect(storage.links.size).toBe(0);
    expect(await screen.findByText(/^Link stopped /)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Share a link with Mentor" })).toBeTruthy();
  });

  it("says when someone editing with them changed the file, until the next checkpoint", async () => {
    const engine = loadEngine(wasm, new Uint8Array(32).fill(21));
    const storage = new MemoryStorage();
    let coAuthorEdit: (() => void) | undefined;
    const host = Object.assign(
      new FakeHost(engine, fixture("resume-master.docx"), {
        name: "Resume.docx",
        url: "https://d.docs.live.net/0/Documents/Resume.docx",
      }),
      {
        watchCoAuthors: (onEdit: () => void) => {
          coAuthorEdit = onEdit;
          return Promise.resolve();
        },
      },
    );
    render(<App engine={engine} host={host} storage={{ kind: "ready", storage }} />);
    fireEvent.click(await screen.findByRole("button", { name: "Start tracking" }));
    await screen.findByText("Lineage is tracking this file. This is v1.");
    const note = /Someone you share this file with edited it while it was open here/;
    expect(screen.queryByText(note)).toBeNull();

    host.edit(fixture("resume-master-new-job.docx"));
    act(() => {
      coAuthorEdit?.();
    });
    expect(await screen.findByText(note)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Save checkpoint" }));
    expect(await screen.findByText("2 changes since v1")).toBeTruthy();
    expect(screen.getByText(note)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Save checkpoint" }));
    expect(await screen.findByText("Checkpoint saved.")).toBeTruthy();
    expect(screen.queryByText(note)).toBeNull();
  });

  it("opens a file someone shared without offering to track it", async () => {
    const engine = loadEngine(wasm, new Uint8Array(32).fill(23));
    const url = "https://d.docs.live.net/0/Documents/Resume.docx";
    const owners = new FakeHost(engine, fixture("resume-master.docx"), {
      name: "Resume.docx",
      url,
    });
    await new Lineage({ engine, storage: new MemoryStorage(), host: owners }).startTracking();

    const theirs = Object.assign(new MemoryStorage(), {
      holds: () => Promise.resolve(false),
    });
    const host = new FakeHost(engine, await owners.readBytes(), { name: "Resume.docx", url });
    render(<App engine={engine} host={host} storage={{ kind: "ready", storage: theirs }} />);
    expect(await screen.findByRole("heading", { name: "Shared with you" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Start tracking" })).toBeNull();
    expect(theirs.ledgers.size).toBe(0);
  });

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

  it("offers to review an untracked file that is a copy sent without its ids", async () => {
    const engine = loadEngine(wasm, new Uint8Array(32).fill(12));
    const storage = new MemoryStorage();
    const masterHost = new FakeHost(engine, fixture("resume-master.docx"), {
      name: "Resume.docx",
      url: "C:/Users/someone/Resume.docx",
    });
    const master = new Lineage({ engine, storage, host: masterHost });
    await master.open();
    const started = await master.startTracking();
    if (started.kind !== "master") {
      throw new Error("not tracking");
    }
    const sent = await master.sendCopy(started.file.nodeId, {
      recipient: "Northgate Analytics",
      purpose: null,
      embedId: false,
    });
    const copy = await storage.readFile(sent.ref);

    // The copy comes back without ids and is opened in Word.
    const host = new FakeHost(engine, copy, {
      name: "Avery Quinn - Northgate notes.docx",
      url: "C:/Users/someone/Downloads/Avery Quinn - Northgate notes.docx",
    });
    render(<App engine={engine} host={host} storage={{ kind: "ready", storage }} />);
    fireEvent.click(
      await screen.findByRole("button", { name: "Review it as a file that came back" }),
    );
    expect(
      await screen.findByRole("heading", { name: "This came back from Northgate Analytics" }),
    ).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Review their changes" }));
    expect(await screen.findByRole("heading", { name: "Lineage kept the file" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Open “Resume”" }));
    await waitFor(() => {
      expect(host.opened).toEqual([{ path: "C:/Users/someone/Resume.docx" }]);
    });
  });

  it("links to My documents from the footer when it knows where it is", async () => {
    const engine = loadEngine(wasm, new Uint8Array(32).fill(11));
    const host = new FakeHost(engine, fixture("resume-master.docx"), {
      name: "Resume.docx",
      url: "https://d.docs.live.net/0/Documents/Resume.docx",
    });
    let opened = 0;
    const storage = new MemoryStorage();
    const { unmount } = render(
      <App
        engine={engine}
        host={host}
        storage={{ kind: "ready", storage }}
        openMyDocuments={() => {
          opened += 1;
        }}
      />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Start tracking" }));
    fireEvent.click(await screen.findByRole("button", { name: "My documents" }));
    expect(opened).toBe(1);
    unmount();
    render(<App engine={engine} host={host} storage={{ kind: "ready", storage }} />);
    await screen.findByRole("button", { name: "What I've sent" });
    expect(screen.queryByRole("button", { name: "My documents" })).toBeNull();
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
      "This file isn't a document Lineage can open.",
    );
  });
});
