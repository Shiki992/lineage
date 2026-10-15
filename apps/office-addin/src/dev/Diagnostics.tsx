// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Development only: checks how Word takes what the engine writes (decision
// 0008). "Run the write check" puts each file from the Word smoke test
// (`.dev-storage/spike`, written by the engine's tests with LINEAGE_SMOKE_DIR)
// into the open document with insertFileFromBase64, reads it back with
// getFileAsync and asks the engine whether every revision survived as Lineage's.
// Use a scratch document: each file replaces its contents.

import type { DocumentHost, Engine, Written } from "@lineage/shell";
import { useState } from "react";
import { devLog } from "../storage/dev-folder";

interface Row {
  name: string;
  written?: Written;
  error?: string;
  ms: number;
}

export function Diagnostics({ engine, host }: { engine: Engine; host: DocumentHost }) {
  const [rows, setRows] = useState<Row[]>([]);
  const [status, setStatus] = useState("");
  const [busy, setBusy] = useState(false);

  async function idsCheck(): Promise<void> {
    setBusy(true);
    try {
      const before = await host.readIds();
      const ids = { family: "a".repeat(32), checkpoint: "b".repeat(32) };
      await host.writeIds(ids);
      const read = engine.readIds(await host.readBytes());
      await host.writeIds(before);
      const ok = read?.family === ids.family && read.checkpoint === ids.checkpoint;
      setStatus(
        ok ? "Ids written by Word are read by the engine." : `Ids differ: ${JSON.stringify(read)}`,
      );
      devLog({ name: "spike.ids", kind: ok ? "ok" : "differ" });
    } catch (error) {
      setStatus(String(error));
    } finally {
      setBusy(false);
    }
  }

  async function writeCheck(): Promise<void> {
    setBusy(true);
    setRows([]);
    try {
      const names = (await (await fetch("/__lineage-dev/spike")).json()) as string[];
      const done: Row[] = [];
      for (const name of names) {
        const started = performance.now();
        try {
          const bytes = new Uint8Array(
            await (await fetch(`/__lineage-dev/spike/${encodeURIComponent(name)}`)).arrayBuffer(),
          );
          await host.replaceContents(bytes);
          const written = engine.checkWritten(bytes, await host.readBytes());
          done.push({ name, written, ms: performance.now() - started });
          devLog({
            name: `spike.write.${written.ok ? "ok" : "failed"}`,
            ms: performance.now() - started,
            kind: name.slice(0, 40),
          });
        } catch (error) {
          done.push({ name, error: String(error), ms: performance.now() - started });
          devLog({ name: "spike.write.error", kind: name.slice(0, 40) });
        }
        setRows([...done]);
      }
      const ok = done.filter((r) => r.written?.ok).length;
      setStatus(`${String(ok)} of ${String(done.length)} files landed exactly.`);
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="panel stack" aria-labelledby="diagnostics">
      <h2 id="diagnostics">Diagnostics (development)</h2>
      <div className="row">
        <button type="button" className="button" disabled={busy} onClick={() => void idsCheck()}>
          Check ids
        </button>
        <button type="button" className="button" disabled={busy} onClick={() => void writeCheck()}>
          Run the write check
        </button>
      </div>
      <p role="status">{status}</p>
      <ul className="list small">
        {rows.map((r) => (
          <li key={r.name}>
            <span className="mono">{r.name}</span>:{" "}
            {r.error ??
              (r.written?.ok
                ? "ok"
                : `accepted ${String(r.written?.sameAccepted)}, rejected ${String(r.written?.sameRejected)}, Lineage ${String(r.written?.lineageFound)}/${String(r.written?.lineageExpected)}, others ${String(r.written?.otherAdded)}`)}{" "}
            ({Math.round(r.ms)} ms)
          </li>
        ))}
      </ul>
    </section>
  );
}
