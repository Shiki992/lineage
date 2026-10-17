// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import "@fontsource/ibm-plex-sans/latin-400.css";
import "@fontsource/ibm-plex-sans/latin-600.css";
import "@fontsource/ibm-plex-mono/latin-400.css";
import "@lineage/design/tokens.css";
import "./styles/app.css";

import { loadEngineAsync } from "@lineage/shell";
import wasmUrl from "@lineage/shell/wasm?url";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { chooseAccount } from "./account/choose";
import { App } from "./app/App";

async function boot(): Promise<void> {
  const element = document.getElementById("root");
  if (!element) {
    throw new Error("no #root");
  }
  const view = createRoot(element);
  try {
    const { engine } = await loadEngineAsync(wasmUrl);
    view.render(
      <StrictMode>
        <App account={chooseAccount()} engine={engine} />
      </StrictMode>,
    );
  } catch (error) {
    console.error(error);
    view.render(
      <p className="alert" role="alert">
        Lineage couldn&apos;t start. Reload the page to try again.
      </p>,
    );
  }
}

void boot();
