// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The task pane: which screen is showing, and one action at a time. Every action
// goes through `Lineage` (packages/shell), which reads and writes the document,
// the engine and storage; this file only draws and routes.

import type {
  AsyncEngine,
  Candidate,
  CardDecision,
  DeckPanel,
  DocumentHost,
  Engine,
  Family,
  LinkAccess,
  MatchedBy,
  Opened,
  Returning,
  ReturnReview,
  SendArgs,
  SentList,
  Storage,
  UpdateRun,
  Updates,
  VersionChoice,
  WhatChanged,
} from "@lineage/shell";
import { EngineError, Lineage, goesToCells, userMessage } from "@lineage/shell";
import { useCallback, useEffect, useMemo, useState } from "react";
import { Diagnostics } from "../dev/Diagnostics";
import { errorKind, pageStart, timing } from "../metrics";
import type { StorageChoice } from "../storage/choose";
import { UpdatesView } from "./changes";
import { DeckHome, DeckVersionReview, UpdateReport } from "./deck";
import { MakeVersion, SaveCheckpoint, SendCopy } from "./forms";
import type { Action } from "./home";
import { Elsewhere, MasterHome, SignIn, StartTracking, VersionHome } from "./home";
import { Button, ErrorBanner, LinkButton } from "./parts";
import { CameBack, ChooseReturned, OpenToReview, ReviewReturned } from "./returned";
import { WhatIveSent } from "./sent";
import type { MadeLink } from "./share";
import { ShareLink } from "./share";
import type { CellActions } from "./workbook";
import { WorkbookReview } from "./workbook";

type Screen =
  | { kind: "home" }
  | { kind: "saveCheckpoint"; changes: WhatChanged | null }
  | { kind: "makeVersion" }
  | { kind: "updates"; updates: Updates }
  /** A tailored deck open in PowerPoint brings changes in itself (decision 0019). */
  | { kind: "deckUpdates"; updates: Updates }
  | { kind: "sendCopy"; family: Family; goesWith: string[] }
  | { kind: "sent"; list: SentList; edited?: readonly string[] }
  | {
      kind: "share";
      sent: string;
      recipient: string;
      /** Where the copy was just saved, if it was. */
      ref?: string;
      made: (MadeLink & { access: LinkAccess }) | null;
    }
  | { kind: "chooseReturned" }
  /** `fromSent`: a shared copy they edited, opened from What I've sent. */
  | { kind: "cameBack"; returning: Returning; fromSent?: boolean }
  | {
      kind: "openToReview";
      family: string;
      node: string;
      name: string;
      returned: string;
      canReviewHere: boolean;
    }
  | { kind: "returnedReview"; review: ReturnReview }
  | { kind: "updateReport"; run: UpdateRun }
  /** A tailored workbook open in Excel brings changes in itself (decision 0023). */
  | { kind: "workbookUpdates"; updates: Updates }
  /** A workbook that came back, kept into the open workbook. */
  | { kind: "workbookReturned"; review: ReturnReview }
  | { kind: "diagnostics" };

export interface AppProps {
  /** In this thread, or in a Web Worker (Excel, decision 0025). */
  engine: Engine | AsyncEngine;
  host: DocumentHost;
  storage: StorageChoice;
  /** Signs in and returns the person's storage (OneDrive). */
  signIn?: () => Promise<Storage>;
  /** Opens My documents in the browser (the footer link). */
  openMyDocuments?: (() => void) | undefined;
}

export function App({ engine, host, storage: choice, signIn, openMyDocuments }: AppProps) {
  const [storage, setStorage] = useState<Storage | null>(
    choice.kind === "ready" ? choice.storage : null,
  );
  const [opened, setOpened] = useState<Opened | null>(null);
  const [screen, setScreen] = useState<Screen>({ kind: "home" });
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<{
    text: string;
    open?: () => void;
    /** "Share a link" to the copy just made (decision 0032). */
    share?: () => void;
  } | null>(null);
  const [crashed, setCrashed] = useState(false);
  /** The pitch deck panel, for a master deck (decision 0018). */
  const [deckPanel, setDeckPanel] = useState<DeckPanel | null>(null);
  /** For an untracked document: whether it looks like a copy that came back. */
  const [untrackedReturning, setUntrackedReturning] = useState<Returning | null>(null);
  /**
   * Someone editing this file with the person changed it since the panel
   * opened or the last checkpoint (decision 0032): what changed includes
   * their edits, which Lineage can't tell apart in Word.
   */
  const [coAuthored, setCoAuthored] = useState(false);
  useEffect(() => {
    host
      .watchCoAuthors?.(() => {
        setCoAuthored(true);
      })
      .catch((e: unknown) => {
        console.error(e);
      });
  }, [host]);
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

  /** "What I've sent", with any copies changed through their edit links. */
  const showSent = useCallback(async (l: Lineage, search?: string) => {
    const list = await l.sentList(search);
    const edited = l.canShare ? await l.sharedEdits() : [];
    setScreen({ kind: "sent", list, edited });
  }, []);

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

  // "Go to cell" and the side by side, on workbook screens.
  const cells = useMemo<CellActions>(
    () => ({
      goTo: goesToCells(host)
        ? (sheet, address) => {
            host.goToCell(sheet, address).catch((e: unknown) => {
              console.error(e);
              setError(userMessage(e));
            });
          }
        : undefined,
      sideBySide: lineage ? (sheet, address) => lineage.cellSideBySide(sheet, address) : undefined,
    }),
    [host, lineage],
  );

  // A master deck shows the pitch deck panel: what just changed, and the
  // versions to update. Read again whenever the panel is read.
  const isMasterDeck = opened?.kind === "master" && opened.file.format === "pptx";
  useEffect(() => {
    if (!lineage || !isMasterDeck || screen.kind !== "home") {
      return;
    }
    let live = true;
    lineage.deckPanel().then(
      (panel) => {
        if (live) {
          setDeckPanel(panel);
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
  }, [lineage, isMasterDeck, opened, screen.kind]);

  /** "Update N versions", and choices on what a previous run left. */
  const updateVersions = (versions: string[], choices: VersionChoice[], note?: string) => {
    setNotice(null);
    void run("Updating the versions", async (l) => {
      const result = await l.updateVersions(
        versions,
        choices,
        note === undefined || note === "" ? undefined : note,
      );
      setOpened(await l.refresh());
      setScreen({ kind: "updateReport", run: result });
    });
  };

  const refresh = async (l: Lineage) => {
    setOpened(await l.refresh());
    setScreen({ kind: "home" });
  };

  const openFile = opened?.kind === "master" || opened?.kind === "version" ? opened.file : null;

  /** "Review" on a file that came back: its review in the open file. */
  const reviewReturned = (returned: string) => {
    setNotice(null);
    void run("Getting their changes", async (l) => {
      const review = await l.returnedReview(returned);
      setScreen(
        review.view.workbook
          ? { kind: "workbookReturned", review }
          : { kind: "returnedReview", review },
      );
    });
  };

  /** The person confirmed which copy came back: keep it, then review it where it belongs. */
  const confirmReturned = (returning: Returning, candidate: Candidate, how: MatchedBy) => {
    void run("Keeping the file", async (l) => {
      const kept = await l.recordReturned(returning, candidate, how);
      if (openFile?.familyId === kept.family && openFile.nodeId === kept.node) {
        setOpened(await l.refresh());
        const review = await l.returnedReview(kept.returned);
        setScreen(
          review.view.workbook
            ? { kind: "workbookReturned", review }
            : { kind: "returnedReview", review },
        );
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
        const family = await l.family();
        // What goes with a workbook's copy is said before it's saved (decision 0026).
        const goesWith = openFile?.format === "xlsx" ? await l.goesWithCopy() : [];
        setScreen({ kind: "sendCopy", family, goesWith });
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
  } else if (import.meta.env.DEV && screen.kind === "diagnostics") {
    // Development only: the guard keeps Diagnostics and the dev-folder client
    // out of production builds (the hosting plugin's checks fail otherwise).
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
          if (screen.fromSent) {
            void run("Getting what you've sent", (l) => showSent(l));
            return;
          }
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
    body = <Elsewhere storage={storageLabel} where={opened.where} />;
  } else if (screen.kind === "chooseReturned") {
    body = (
      <ChooseReturned
        busy={busy !== null}
        format={openFile?.format === "xlsx" ? "xlsx" : "docx"}
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
        onApply={(cards: CardDecision[], wordsOnly: boolean) =>
          void run("Keeping your choices", async (l) => {
            const { wrote } = await l.decideReturned(review.returned, cards, { wordsOnly });
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
        cells={cells}
        busy={busy !== null}
        onBack={home}
        onSave={(note) =>
          void run("Saving a checkpoint", async (l) => {
            const saved = await l.saveCheckpoint(note);
            setCoAuthored(false);
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
  } else if (screen.kind === "updateReport") {
    body = (
      <UpdateReport
        run={screen.run}
        busy={busy !== null}
        onChoose={(versions, choices) => {
          updateVersions(versions, choices);
        }}
        onDone={home}
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
  } else if (screen.kind === "workbookUpdates") {
    const updates = screen.updates;
    body = (
      <WorkbookReview
        title={updates.headline}
        groups={updates.groups.flatMap((g) =>
          g.view.workbook ? [{ key: g.checkpoint, label: g.label, view: g.view.workbook }] : [],
        )}
        verb="Bring in"
        canBringIn={lineage?.bringsIntoOpenWorkbook === true}
        busy={busy !== null}
        onOpenMaster={(checkpoint) =>
          void run("Opening the master's version", async (l) => {
            await l.openCheckpoint(checkpoint);
          })
        }
        onBack={home}
        onApply={(cards) =>
          void run("Bringing the changes into this workbook", async (l) => {
            const plan = await l.bringInWorkbook(cards);
            await refresh(l);
            setNotice({
              text:
                plan.brought > 0
                  ? `${plan.text.replace(/^Brings in/, "Brought in")}. Lineage saved a checkpoint of this workbook.`
                  : "Done. Your choices are saved.",
            });
          })
        }
      />
    );
  } else if (screen.kind === "workbookReturned") {
    const review = screen.review;
    body = (
      <WorkbookReview
        title={review.headline}
        groups={
          review.view.workbook
            ? [{ key: review.returned, label: null, view: review.view.workbook }]
            : []
        }
        verb="Keep"
        canBringIn={lineage?.bringsIntoOpenWorkbook === true}
        busy={busy !== null}
        fromOutside
        onBack={home}
        onApply={(cards) =>
          void run("Keeping their changes in this workbook", async (l) => {
            const plan = await l.keepIntoWorkbook(review.returned, cards);
            await refresh(l);
            setNotice({
              text:
                plan.brought > 0
                  ? `${plan.text.replace(/^Keeps/, "Kept")}. Lineage saved a checkpoint of this workbook.`
                  : "Done. Your choices are saved.",
            });
          })
        }
      />
    );
  } else if (screen.kind === "deckUpdates") {
    body = (
      <DeckVersionReview
        updates={screen.updates}
        node={opened.file.nodeId}
        busy={busy !== null}
        onBack={home}
        onBringIn={(choices) =>
          void run("Bringing the changes into this deck", async (l) => {
            const line = await l.bringInOpenDeck(choices);
            await refresh(l);
            setNotice({
              text:
                line.result === "updated"
                  ? `${line.text}. Lineage saved a checkpoint of this deck.`
                  : line.text,
            });
          })
        }
      />
    );
  } else if (screen.kind === "sendCopy") {
    body = (
      <SendCopy
        family={screen.family}
        goesWith={screen.goesWith}
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
              ...(l.canShare
                ? {
                    share: () => {
                      setNotice(null);
                      setScreen({
                        kind: "share",
                        sent: sent.sent,
                        recipient: args.recipient,
                        ref: sent.ref,
                        made: null,
                      });
                    },
                  }
                : {}),
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
            await showSent(l, search);
          })
        }
        onOpen={(sent) => void run("Opening the copy", (l) => l.openSentCopy(sent))}
        onReview={reviewReturned}
        onCameBack={cameBackLink}
        edited={screen.edited ?? []}
        {...(lineage?.canShare
          ? {
              onShare: (sent: string, recipient: string) => {
                setScreen({ kind: "share", sent, recipient, made: null });
              },
              onStopSharing: (sent: string) =>
                void run("Stopping the link", async (l) => {
                  await l.stopSharing(sent);
                  await showSent(l);
                  setNotice({ text: "Stopped. The link doesn't work any more." });
                }),
              onReviewShared: (sent: string) =>
                void run("Reading their copy", async (l) => {
                  const { bytes, name } = await l.sharedFile(sent);
                  setScreen({
                    kind: "cameBack",
                    returning: await l.recognise(bytes, name),
                    fromSent: true,
                  });
                }),
            }
          : {})}
      />
    );
  } else if (screen.kind === "share") {
    const share = screen;
    body = (
      <ShareLink
        recipient={share.recipient}
        storage={storageLabel}
        made={share.made}
        busy={busy !== null}
        onBack={() => {
          void run("Getting what you've sent", (l) => showSent(l));
        }}
        onShare={(access) =>
          void run("Making the link", async (l) => {
            const made = await l.shareLink(share.sent, access, share.ref);
            setScreen({ ...share, made: { url: made.url, who: made.who, access } });
          })
        }
      />
    );
  } else if (opened.kind === "master" && opened.file.format === "pptx") {
    body = deckPanel ? (
      <DeckHome
        // A fresh look at the deck starts the ticks afresh.
        key={[
          deckPanel.master.latest.id,
          ...(deckPanel.justChanged?.cards.map((c) => c.id) ?? []),
          ...deckPanel.targets.map((t) => `${t.id}:${String(t.checked)}`),
        ].join(" ")}
        panel={deckPanel}
        busy={busy !== null}
        onUpdate={(versions, note) => {
          updateVersions(versions, [], note);
        }}
        onCheck={() => {
          setNotice(null);
          void run("Looking for changes", async (l) => {
            setDeckPanel(await l.deckPanel());
          });
        }}
        onOpen={(ref) => void run("Opening", (l) => l.openStored(ref))}
        onOpenSent={(sent) => void run("Opening what they saw", (l) => l.openSentCopy(sent))}
        onAction={onAction}
        onSent={() =>
          void run("Getting what you've sent", async (l) => {
            await showSent(l);
          })
        }
        onMyDocuments={openMyDocuments}
      />
    ) : (
      <p role="status">Opening…</p>
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
            await showSent(l);
          })
        }
        onCameBack={cameBackLink}
        onReviewReturned={reviewReturned}
        onMyDocuments={openMyDocuments}
      />
    );
  } else {
    const deck = opened.file.format === "pptx";
    const bringsIn = deck && lineage?.bringsIntoOpenDeck === true;
    body = (
      <VersionHome
        panel={opened.panel}
        busy={busy !== null}
        deck={deck}
        bringsIn={bringsIn}
        onAction={onAction}
        onReview={() =>
          void run("Getting the updates", async (l) => {
            const updates = await l.updates();
            setScreen(
              deck
                ? { kind: "deckUpdates", updates }
                : opened.file.format === "xlsx"
                  ? { kind: "workbookUpdates", updates }
                  : { kind: "updates", updates },
            );
          })
        }
        onSent={() =>
          void run("Getting what you've sent", async (l) => {
            await showSent(l);
          })
        }
        onCameBack={deck ? undefined : cameBackLink}
        onReviewReturned={reviewReturned}
        onMyDocuments={openMyDocuments}
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
          {(notice.open ?? notice.share) && (
            <div className="row row--wrap">
              {notice.open && <Button onClick={notice.open}>Open it</Button>}
              {notice.share && <Button onClick={notice.share}>Share a link</Button>}
            </div>
          )}
        </div>
      )}
      {busy && (
        <p className="secondary small" role="status">
          {busy}…
        </p>
      )}
      {host.readsLastSaved === true &&
        (screen.kind === "home" || screen.kind === "saveCheckpoint") && (
          <p className="small secondary" role="note">
            In Excel on the web, Lineage reads this workbook as last saved in OneDrive, and Excel
            saves as you go. The first time, Microsoft asks to let Lineage read your files; it reads
            only the workbook that's open.
          </p>
        )}
      {coAuthored &&
        (opened?.kind === "master" || opened?.kind === "version") &&
        (screen.kind === "home" || screen.kind === "saveCheckpoint") && (
          <p className="small secondary" role="note">
            Someone you share this file with edited it while it was open here. What changed includes
            their edits.
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
