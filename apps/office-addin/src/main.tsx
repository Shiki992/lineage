// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import "@fontsource/ibm-plex-sans/latin-400.css";
import "@fontsource/ibm-plex-sans/latin-600.css";
import "@fontsource/ibm-plex-mono/latin-400.css";
import "./styles/tokens.css";
import "./styles/app.css";

import { loadEngineAsync } from "@lineage/shell";
import wasmUrl from "@lineage/shell/wasm?url";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import { pageStart, timing } from "./metrics";
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
  if (info.host !== Office.HostType.Word) {
    view.render(<p className="panel">Lineage works in Word. Open a Word document to use it.</p>);
    return;
  }
  try {
    const { engine, loadMs } = await loadEngineAsync(wasmUrl);
    timing("engine.load", loadMs);
    view.render(
      <StrictMode>
        <App engine={engine} host={new WordHost()} storage={chooseStorage()} />
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

void Office.onReady((info) => boot(info));
