// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The task pane: which screen is showing, and one action at a time. Every action
// goes through `Lineage` (packages/shell), which reads and writes the document,
// the engine and storage; this file only draws and routes.

import type {
  Candidate,
  CardDecision,
  DocumentHost,
  Engine,
  Family,
  MatchedBy,
  Opened,
  Returning,
  ReturnReview,
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
import { CameBack, ChooseReturned, OpenToReview, ReviewReturned } from "./returned";
import { WhatIveSent } from "./sent";

type Screen =
  | { kind: "home" }
  | { kind: "saveCheckpoint"; changes: WhatChanged | null }
  | { kind: "makeVersion" }
  | { kind: "updates"; updates: Updates }
  | { kind: "sendCopy"; family: Family }
  | { kind: "sent"; list: SentList }
  | { kind: "chooseReturned" }
  | { kind: "cameBack"; returning: Returning }
  | {
      kind: "openToReview";
      family: string;
      node: string;
      name: string;
      returned: string;
      canReviewHere: boolean;
    }
  | { kind: "returnedReview"; review: ReturnReview }
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
  /** For an untracked document: whether it looks like a copy that came back. */
  const [untrackedReturning, setUntrackedReturning] = useState<Returning | null>(null);
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
        // An untracked document may be a copy that came back without its ids:
        // check in the background, so the panel shows at once.
        if (found.kind === "untracked") {
          lineage.recogniseOpen().then(
            (returning) => {
              if (live && returning.recognition.verdict !== "unknown") {
                setUntrackedReturning(returning);
              }
            },
            (e: unknown) => {
              console.error(e);
            },
          );
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

  // Another window may have changed the history meanwhile (a copy that came
  // back is recorded from its own window): read it again when the panel comes
  // back into view on its home screen. Found in the Phase 3 Word run.
  const showingPanel =
    screen.kind === "home" && (opened?.kind === "master" || opened?.kind === "version");
  useEffect(() => {
    if (!lineage || !showingPanel) {
      return;
    }
    const again = () => {
      if (document.visibilityState === "visible") {
        lineage.refresh().then(setOpened, (e: unknown) => {
          console.error(e);
        });
      }
    };
    window.addEventListener("focus", again);
    document.addEventListener("visibilitychange", again);
    return () => {
      window.removeEventListener("focus", again);
      document.removeEventListener("visibilitychange", again);
    };
  }, [lineage, showingPanel]);

  const home = useCallback(() => {
    setScreen({ kind: "home" });
  }, []);

  const refresh = async (l: Lineage) => {
    setOpened(await l.refresh());
    setScreen({ kind: "home" });
  };

  const openFile = opened?.kind === "master" || opened?.kind === "version" ? opened.file : null;

  /** "Review" on a file that came back: its review in the open file. */
  const reviewReturned = (returned: string) => {
    setNotice(null);
    void run("Getting their changes", async (l) => {
      setScreen({ kind: "returnedReview", review: await l.returnedReview(returned) });
    });
  };

  /** The person confirmed which copy came back: keep it, then review it where it belongs. */
  const confirmReturned = (returning: Returning, candidate: Candidate, how: MatchedBy) => {
    void run("Keeping the file", async (l) => {
      const kept = await l.recordReturned(returning, candidate, how);
      if (openFile?.familyId === kept.family && openFile.nodeId === kept.node) {
        setOpened(await l.refresh());
        setScreen({ kind: "returnedReview", review: await l.returnedReview(kept.returned) });
        return;
      }
      setScreen({
        kind: "openToReview",
        family: kept.family,
        node: kept.node,
        name: kept.nodeName,
        returned: kept.returned,
        canReviewHere: openFile?.familyId === kept.family,
      });
    });
  };

  const cameBackLink = () => {
    setNotice(null);
    setScreen({ kind: "chooseReturned" });
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
  } else if (screen.kind === "cameBack") {
    // Drawn before the untracked screen: an untracked file may be a copy that
    // came back without its ids.
    const returning = screen.returning;
    body = (
      <CameBack
        name={returning.name}
        recognition={returning.recognition}
        busy={busy !== null}
        onBack={() => {
          setScreen(
            opened.kind === "cameBack" || opened.kind === "untracked"
              ? { kind: "home" }
              : { kind: "chooseReturned" },
          );
        }}
        onConfirm={(candidate, how) => {
          confirmReturned(returning, candidate, how);
        }}
      />
    );
  } else if (screen.kind === "openToReview") {
    const target = screen;
    body = (
      <OpenToReview
        name={target.name}
        hereName={target.canReviewHere && openFile ? openFile.displayName : null}
        busy={busy !== null}
        onBack={home}
        onOpen={() => void run("Opening", (l) => l.openNode(target.family, target.node))}
        onReviewHere={() => {
          reviewReturned(target.returned);
        }}
      />
    );
  } else if (opened.kind === "untracked") {
    body = (
      <StartTracking
        name={opened.name}
        saved={opened.saved}
        storage={storageLabel}
        busy={busy !== null}
        cameBack={untrackedReturning?.recognition ?? null}
        onCameBack={() => {
          if (untrackedReturning) {
            setScreen({ kind: "cameBack", returning: untrackedReturning });
          }
        }}
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
  } else if (screen.kind === "chooseReturned") {
    body = (
      <ChooseReturned
        busy={busy !== null}
        onBack={home}
        onFile={(bytes, name) =>
          void run("Looking for the copy it came from", async (l) => {
            setScreen({ kind: "cameBack", returning: await l.recognise(bytes, name) });
          })
        }
      />
    );
  } else if (screen.kind === "returnedReview") {
    const review = screen.review;
    const family = openFile?.familyId ?? "";
    body = (
      <ReviewReturned
        review={review}
        isSource={openFile?.nodeId === review.fromNode}
        busy={busy !== null}
        onBack={home}
        onOpenSource={() => void run("Opening", (l) => l.openNode(family, review.fromNode))}
        onOpenReturned={() =>
          void run("Opening what came back", (l) => l.openReturned(family, review.returned))
        }
        onApply={(cards: CardDecision[]) =>
          void run("Keeping your choices", async (l) => {
            const { wrote } = await l.decideReturned(review.returned, cards);
            await refresh(l);
            setNotice({
              text: wrote
                ? "Done. The changes you kept are in your file as tracked changes by Lineage, and a checkpoint was saved."
                : "Done. Nothing was added to your file.",
            });
          })
        }
      />
    );
  } else if (opened.kind === "cameBack") {
    const returning = opened.returning;
    body = (
      <CameBack
        name={returning.name}
        recognition={returning.recognition}
        busy={busy !== null}
        onBack={() => {
          setScreen({ kind: "home" });
        }}
        onConfirm={(candidate, how) => {
          confirmReturned(returning, candidate, how);
        }}
      />
    );
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
        onReview={reviewReturned}
        onCameBack={cameBackLink}
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
        onCameBack={cameBackLink}
        onReviewReturned={reviewReturned}
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
        onCameBack={cameBackLink}
        onReviewReturned={reviewReturned}
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
