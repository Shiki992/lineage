// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import type { DocumentHost, Engine } from "@lineage/shell";
import { Diagnostics } from "../dev/Diagnostics";
import type { StorageChoice } from "../storage/choose";

export function App({ engine, host, storage }: { engine: Engine; host: DocumentHost; storage: StorageChoice }) {
  return (
    <main className="stack">
      <h1>Lineage</h1>
      <p className="secondary small">
        History kept in {storage.kind === "ready" ? storage.storage.label : "OneDrive"}.
      </p>
      {import.meta.env.DEV && <Diagnostics engine={engine} host={host} />}
    </main>
  );
}
