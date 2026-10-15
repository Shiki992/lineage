// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The task pane: which screen is showing, and one action at a time. Every action
// goes through `Lineage` (packages/shell), which reads and writes the document,
// the engine and storage; this file only draws and routes.

import type {
  CardDecision,
  DocumentHost,
  Engine,
  Family,
  Opened,
  SendArgs,
  SentList,
  Storage,
  Updates,
  WhatChanged,
} from "@lineage/shell";
import { EngineError, Lineage, userMessage } from "@lineage/shell";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Diagnostics } from "../dev/Diagnostics";
import { errorKind, pageStart, timing } from "../metrics";
import type { StorageChoice } from "../storage/choose";
import { UpdatesView } from "./changes";
import { MakeVersion, SaveCheckpoint, SendCopy } from "./forms";
import type { Action } from "./home";
import { Elsewhere, MasterHome, SignIn, StartTracking, VersionHome } from "./home";
import { Button, ErrorBanner, LinkButton } from "./parts";
import { WhatIveSent } from "./sent";

type Screen =
  | { kind: "home" }
  | { kind: "saveCheckpoint"; changes: WhatChanged | null }
  | { kind: "makeVersion" }
  | { kind: "updates"; updates: Updates }
  | { kind: "sendCopy"; family: Family }
  | { kind: "sent"; list: SentList }
  | { kind: "diagnostics" };

export interface AppProps {
  engine: Engine;
  host: DocumentHost;
  storage: StorageChoice;
  /** Signs in and returns the person's storage (OneDrive). */
  signIn?: () => Promise<Storage>;
}

export function App({ engine, host, storage: choice, signIn }: AppProps) {
  const [storage, setStorage] = useState<Storage | null>(
    choice.kind === "ready" ? choice.storage : null,
  );
  const [opened, setOpened] = useState<Opened | null>(null);
  const [screen, setScreen] = useState<Screen>({ kind: "home" });
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<{ text: string; open?: () => void } | null>(null);
  const [crashed, setCrashed] = useState(false);
  const lineage = useMemo(
    () => (storage ? new Lineage({ engine, storage, host, onTiming: timing }) : null),
    [engine, host, storage],
  );

  /** Runs one action: busy while it runs, its error in plain words. */
  const run = useCallback(
    async (label: string, work: (l: Lineage) => Promise<void>) => {
      const current = lineage;
      if (!current) {
        return;
      }
      setBusy(label);
      setError(null);
      try {
        await work(current);
      } catch (e) {
        console.error(e);
        if (e instanceof EngineError && e.kind === "crashed") {
          setCrashed(true);
        }
        errorKind(e instanceof Error ? e.name : "unknown");
        setError(userMessage(e));
      } finally {
        setBusy(null);
      }
    },
    [lineage],
  );

  // Once storage is known: find out what the open document is.
  useEffect(() => {
    if (!lineage) {
      return;
    }
    let live = true;
    lineage.open().then(
      (found) => {
        if (live) {
          setOpened(found);
          timing("panel.ready", performance.now() - pageStart);
        }
      },
      (e: unknown) => {
        console.error(e);
        if (live) {
          setError(userMessage(e));
        }
      },
    );
    return () => {
      live = false;
    };
  }, [lineage]);

  const home = useCallback(() => {
    setScreen({ kind: "home" });
  }, []);

  const refresh = async (l: Lineage) => {
    setOpened(await l.refresh());
    setScreen({ kind: "home" });
  };

  const onAction = (action: Action) => {
    setNotice(null);
    if (action === "makeVersion") {
      setScreen({ kind: "makeVersion" });
    } else if (action === "saveCheckpoint") {
      setScreen({ kind: "saveCheckpoint", changes: null });
      void run("Looking at what changed", async (l) => {
        const changes = await l.whatChanged();
        setScreen((s) => (s.kind === "saveCheckpoint" ? { kind: "saveCheckpoint", changes } : s));
      });
    } else {
      void run("Getting your versions", async (l) => {
        setScreen({ kind: "sendCopy", family: await l.family() });
      });
    }
  };

  if (crashed) {
    return (
      <main className="stack">
        <p className="alert" role="alert">
          Something went wrong inside Lineage. Nothing was changed. Close and reopen the panel.
        </p>
      </main>
    );
  }

  const storageLabel = storage?.label ?? "OneDrive";
  let body: React.ReactNode;
  if (!storage) {
    body = (
      <SignIn
        busy={busy !== null}
        onSignIn={() => {
          if (!signIn) {
            return;
          }
          setBusy("Signing in");
          setError(null);
          signIn()
            .then(setStorage)
            .catch((e: unknown) => {
              console.error(e);
              setError(userMessage(e));
            })
            .finally(() => {
              setBusy(null);
            });
        }}
      />
    );
  } else if (screen.kind === "diagnostics") {
    body = (
      <div className="stack">
        <LinkButton onClick={home}>← Back</LinkButton>
        <Diagnostics engine={engine} host={host} storage={storage} />
      </div>
    );
  } else if (!opened) {
    body = <p role="status">Opening…</p>;
  } else if (opened.kind === "untracked") {
    body = (
      <StartTracking
        name={opened.name}
        saved={opened.saved}
        storage={storageLabel}
        busy={busy !== null}
        onStart={() =>
          void run("Starting to track", async (l) => {
            setOpened(await l.startTracking());
            setNotice({ text: "Lineage is tracking this file. This is v1." });
          })
        }
      />
    );
  } else if (opened.kind === "elsewhere") {
    body = <Elsewhere storage={storageLabel} />;
  } else if (screen.kind === "saveCheckpoint") {
    body = (
      <SaveCheckpoint
        changes={screen.changes}
        busy={busy !== null}
        onBack={home}
        onSave={(note) =>
          void run("Saving a checkpoint", async (l) => {
            const saved = await l.saveCheckpoint(note);
            await refresh(l);
            setNotice({
              text: saved ? "Checkpoint saved." : "Nothing has changed since the last checkpoint.",
            });
          })
        }
      />
    );
  } else if (screen.kind === "makeVersion" && opened.kind === "master") {
    body = (
      <MakeVersion
        masterName={opened.panel.fileName}
        latest={opened.panel.latest.label}
        busy={busy !== null}
        onBack={home}
        onMake={(name) =>
          void run("Making the tailored version", async (l) => {
            const made = await l.makeTailoredVersion(name);
            await refresh(l);
            setNotice({
              text: `Made “${made.name}”.`,
              open: () => void run("Opening", (l2) => l2.openStored(made.ref)),
            });
          })
        }
      />
    );
  } else if (screen.kind === "updates") {
    body = (
      <UpdatesView
        updates={screen.updates}
        busy={busy !== null}
        onBack={home}
        onApply={(cards: CardDecision[]) =>
          void run("Bringing in your choices", async (l) => {
            const { wrote } = await l.decide(cards);
            await refresh(l);
            setNotice({
              text: wrote
                ? "Done. The changes are in your file as tracked changes by Lineage, and a checkpoint was saved."
                : "Done. Nothing was added to your file.",
            });
          })
        }
      />
    );
  } else if (screen.kind === "sendCopy") {
    body = (
      <SendCopy
        family={screen.family}
        openNode={opened.file.nodeId}
        busy={busy !== null}
        onBack={home}
        onSend={(node: string, args: SendArgs) =>
          void run("Making a clean copy", async (l) => {
            const sent = await l.sendCopy(node, args);
            await refresh(l);
            setNotice({
              text: `Saved “${sent.name}” in ${storageLabel}, in Sent copies.`,
              open: () => void run("Opening", (l2) => l2.openStored(sent.ref)),
            });
          })
        }
      />
    );
  } else if (screen.kind === "sent") {
    body = (
      <WhatIveSent
        list={screen.list}
        busy={busy !== null}
        onBack={home}
        onSearch={(search) =>
          void run("Searching", async (l) => {
            setScreen({ kind: "sent", list: await l.sentList(search) });
          })
        }
        onOpen={(sent) => void run("Opening the copy", (l) => l.openSentCopy(sent))}
      />
    );
  } else if (opened.kind === "master") {
    body = (
      <MasterHome
        panel={opened.panel}
        busy={busy !== null}
        onAction={onAction}
        onOffer={(versions) =>
          void run("Offering the changes", async (l) => {
            await l.offerToVersions(versions);
            await refresh(l);
            setNotice({
              text: "Offered. Each tailored version shows the update when you open it.",
            });
          })
        }
        onOpen={(ref) => void run("Opening", (l) => l.openStored(ref))}
        onSent={() =>
          void run("Getting what you've sent", async (l) => {
            setScreen({ kind: "sent", list: await l.sentList() });
          })
        }
      />
    );
  } else {
    body = (
      <VersionHome
        panel={opened.panel}
        busy={busy !== null}
        onAction={onAction}
        onReview={() =>
          void run("Getting the updates", async (l) => {
            setScreen({ kind: "updates", updates: await l.updates() });
          })
        }
        onSent={() =>
          void run("Getting what you've sent", async (l) => {
            setScreen({ kind: "sent", list: await l.sentList() });
          })
        }
      />
    );
  }

  return (
    <main className="stack" aria-busy={busy !== null}>
      {error && (
        <ErrorBanner
          message={error}
          onDismiss={() => {
            setError(null);
          }}
        />
      )}
      {notice && (
        <div className="callout stack" role="status" style={{ gap: 8 }}>
          <p>{notice.text}</p>
          {notice.open && (
            <div className="row">
              <Button onClick={notice.open}>Open it</Button>
            </div>
          )}
        </div>
      )}
      {busy && (
        <p className="secondary small" role="status">
          {busy}…
        </p>
      )}
      {body}
      {import.meta.env.DEV && screen.kind !== "diagnostics" && storage && (
        <p className="small secondary">
          Development: history in {storageLabel}.{" "}
          <LinkButton
            onClick={() => {
              setScreen({ kind: "diagnostics" });
            }}
          >
            Diagnostics
          </LinkButton>
        </p>
      )}
    </main>
  );
}
