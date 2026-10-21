// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import "@fontsource/ibm-plex-sans/latin-400.css";
import "@fontsource/ibm-plex-sans/latin-600.css";
import "@fontsource/ibm-plex-mono/latin-400.css";
import "@lineage/design/tokens.css";
import "./styles/app.css";

import type { AsyncEngine, Engine, EnginePort } from "@lineage/shell";
import { connectEngine, loadEngineAsync } from "@lineage/shell";
import wasmUrl from "@lineage/shell/wasm?url";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { Stopped } from "./app/stopped";
import { signIn } from "./auth/sign-in";
import { pageStart, timing } from "./metrics";
import { myDocumentsOpener } from "./office/my-documents";
import { ExcelHost, NEEDED as EXCEL_NEEDED } from "./office/excel-host";
import { NEEDED, PowerPointHost } from "./office/powerpoint-host";
import { WordHost } from "./office/word-host";
import { chooseStorage } from "./storage/choose";

function root() {
  const element = document.getElementById("root");
  if (!element) {
    throw new Error("no #root");
  }
  return createRoot(element);
}

async function boot(info: { host: Office.HostType | null }): Promise<void> {
  timing("office.ready", performance.now() - pageStart);
  const view = root();
  const inWord = info.host === Office.HostType.Word;
  const inPowerPoint = info.host === Office.HostType.PowerPoint;
  const inExcel = info.host === Office.HostType.Excel;
  if (!inWord && !inPowerPoint && !inExcel) {
    view.render(
      <p className="panel">
        Lineage works in Word, PowerPoint and Excel. Open a document, a deck or a workbook to use
        it.
      </p>,
    );
    return;
  }
  if (inWord && !Office.context.requirements.isSetSupported("WordApi", "1.5")) {
    view.render(
      <p className="alert" role="alert">
        Lineage needs a newer Word. Update Word, then open the panel again.
      </p>,
    );
    return;
  }
  if (inPowerPoint && !Office.context.requirements.isSetSupported("PowerPointApi", NEEDED)) {
    view.render(
      <p className="alert" role="alert">
        Lineage needs a newer PowerPoint. Update PowerPoint, then open the panel again.
      </p>,
    );
    return;
  }
  // Excel on the web gives an add-in only a PDF of the open workbook, and
  // reading the saved file instead needs a wider OneDrive permission than
  // Lineage asks for: not built yet (decision 0020).
  if (inExcel && Office.context.platform === Office.PlatformType.OfficeOnline) {
    view.render(
      <p className="panel">
        Lineage can't read workbooks in Excel on the web yet. Open this workbook in Excel for
        Windows or Mac to use Lineage with it.
      </p>,
    );
    return;
  }
  if (inExcel && !Office.context.requirements.isSetSupported("ExcelApi", EXCEL_NEEDED)) {
    view.render(
      <p className="alert" role="alert">
        Lineage needs a newer Excel. Update Excel, then open the panel again.
      </p>,
    );
    return;
  }
  try {
    const { engine, loadMs } = inExcel ? await workerEngine() : await loadEngineAsync(wasmUrl);
    timing("engine.load", loadMs);
    let storage = chooseStorage();
    if (storage.kind === "signIn") {
      // Signed in to Word already? Then no button is needed.
      try {
        const signedIn = await signIn({ interactive: false });
        void engine.setAuthor(signedIn.name);
        storage = { kind: "ready", storage: signedIn.storage };
      } catch {
        // The sign-in screen offers the button.
      }
    }
    const interactive = async () => {
      const signedIn = await signIn({ interactive: true });
      void engine.setAuthor(signedIn.name);
      return signedIn.storage;
    };
    const host = inExcel ? await excelHost() : inPowerPoint ? new PowerPointHost() : new WordHost();
    view.render(
      <StrictMode>
        <Stopped>
          <App
            engine={engine}
            host={host}
            storage={storage}
            signIn={interactive}
            openMyDocuments={myDocumentsOpener()}
          />
        </Stopped>
      </StrictMode>,
    );
  } catch (error) {
    console.error(error);
    view.render(
      <p className="alert" role="alert">
        Lineage couldn't start. Close and reopen the panel.
      </p>,
    );
  }
}

/**
 * The engine in a Web Worker, so comparing a large workbook never blocks
 * Excel (decision 0025); in this thread if the worker can't start.
 */
async function workerEngine(): Promise<{ engine: Engine | AsyncEngine; loadMs: number }> {
  const started = performance.now();
  let worker: Worker | undefined;
  try {
    const started_ = new Worker(new URL("./engine-worker.ts", import.meta.url), {
      type: "module",
    });
    worker = started_;
    const failed = new Promise<never>((_, reject) => {
      started_.addEventListener("error", (event) => {
        reject(new Error(`the engine worker failed: ${event.message}`));
      });
      setTimeout(() => {
        reject(new Error("the engine worker didn't start"));
      }, 20_000);
    });
    const engine = await Promise.race([connectEngine(started_ as unknown as EnginePort), failed]);
    return { engine, loadMs: performance.now() - started };
  } catch (error) {
    console.error(error);
    worker?.terminate();
    return loadEngineAsync(wasmUrl);
  }
}

/** The Excel host, keeping the person's edits as hints from the start (decision 0022). */
async function excelHost(): Promise<ExcelHost> {
  const host = new ExcelHost();
  try {
    await host.watchEdits();
  } catch (error) {
    // Hints only: "What changed" works without them.
    console.error(error);
  }
  return host;
}

void Office.onReady((info) => boot(info));
