// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The engine behind a message port, as the Excel pane runs it in a Web Worker
// (decision 0025): a MessageChannel stands in for the worker here.

import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";
import { EngineError, loadEngine } from "./engine.js";
import { Lineage } from "./lineage.js";
import { MemoryStorage } from "./storage.js";
import { FakeExcel } from "./testing/fake-host.js";
import { asyncEngine, connectEngine, serveEngine } from "./worker.js";

const wasm = readFileSync(new URL("../wasm-pkg/lineage_wasm_bg.wasm", import.meta.url));
const workbook = (name: string): Uint8Array =>
  new Uint8Array(
    readFileSync(new URL(`../../../fixtures/xlsx/synthetic/${name}`, import.meta.url)),
  );

const channels: MessageChannel[] = [];
afterEach(() => {
  for (const c of channels.splice(0)) {
    c.port1.close();
    c.port2.close();
  }
});

/** An engine served on one end of a channel, and the other end connected. */
async function connected() {
  const channel = new MessageChannel();
  channels.push(channel);
  const engine = loadEngine(wasm, new Uint8Array(32).fill(77));
  serveEngine(Promise.resolve(engine), channel.port1);
  return { remote: await connectEngine(channel.port2), engine };
}

describe("the engine behind a port", () => {
  it("answers calls with bytes, values and the engine's own errors", async () => {
    const { remote, engine } = await connected();
    expect(remote.version).toBe(engine.version);
    expect(remote.async).toBe(true);
    const bytes = new Uint8Array([0, 1, 254, 255]);
    expect(await remote.echoBytes(bytes)).toEqual(bytes);
    expect(await remote.readIds(workbook("model.xlsx"))).toBeNull();
    // A ledger whose snapshot isn't in the worker's cache: "fetch these".
    const tracked = engine.startTracking(workbook("model.xlsx"), "Model.xlsx", "Model.xlsx", 1);
    const ledger = new TextEncoder().encode(tracked.outcome.ledgerLines);
    const fresh = await connected();
    const node = engine.family(ledger).nodes[0]?.id ?? "";
    const error = await fresh.remote
      .whatChanged(ledger, node, tracked.file ?? new Uint8Array())
      .catch((e: unknown) => e);
    expect(error).toBeInstanceOf(EngineError);
    expect((error as EngineError).kind).toBe("missingBlobs");
    expect((error as EngineError).hashes.length).toBeGreaterThan(0);
  });

  it("is not mistaken for a promise", async () => {
    const { remote } = await connected();
    // Returning it from an async function must not look for a `then`.
    const again = await Promise.resolve(remote);
    expect(again.version).toBe(remote.version);
  });

  it("runs the pane's commands, the same as in this thread", async () => {
    const { remote } = await connected();
    const storage = new MemoryStorage();
    // FakeExcel keeps ids the way Excel does, with a synchronous engine of its own.
    const local = loadEngine(wasm, new Uint8Array(32).fill(78));
    const fake = new FakeExcel(local, workbook("model.xlsx"), {
      name: "Model.xlsx",
      url: "https://d.docs.live.net/0000000000000000/Documents/Model.xlsx",
    });
    const pane = new Lineage({ engine: remote, storage, host: fake, now: () => 1_792_314_000_000 });
    await pane.open();
    expect((await pane.startTracking()).kind).toBe("master");
    fake.edit(workbook("model-price-update.xlsx"));
    const view = await pane.whatChanged();
    expect(view.workbook?.groups[0]?.cards[0]?.summary).toBe("'$49' changed to '$55'");
  });

  it("wraps an engine in this thread to the same shape", async () => {
    const engine = loadEngine(wasm, new Uint8Array(32).fill(79));
    const wrapped = asyncEngine(engine);
    expect(asyncEngine(wrapped)).toBe(wrapped);
    await expect(
      wrapped.writeIds(new Uint8Array([1, 2, 3]), {
        family: "a".repeat(32),
        checkpoint: "b".repeat(32),
      }),
    ).rejects.toBeInstanceOf(EngineError);
  });
});
