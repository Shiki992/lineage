// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The first screen for each kind of file: the canvas "Personal users" master
// resume panel, a tailored version's panel, and the screens before a file is
// tracked. Every label comes from the engine (glossary words), except the few
// that only exist here.

import type { MasterPanel, Recognition, VersionPanel } from "@lineage/shell";
import type { SitePage } from "../office/my-documents";
import { Button, Header, LatestCheckpoint, LinkButton } from "./parts";
import { ReturnedCallout } from "./returned";

export type Action = "makeVersion" | "saveCheckpoint" | "sendCopy";

const ACTIONS: Record<string, Action> = {
  "Make a tailored version": "makeVersion",
  "Save checkpoint": "saveCheckpoint",
  "Send a copy": "sendCopy",
};

export function Actions({
  labels,
  busy,
  onAction,
}: {
  labels: readonly string[];
  busy: boolean;
  onAction: (a: Action) => void;
}) {
  return (
    <div className="stack" style={{ gap: 8 }}>
      {labels.map((label, i) => {
        const action = ACTIONS[label];
        return action ? (
          <Button
            key={label}
            primary={i === 0}
            wide
            disabled={busy}
            onClick={() => {
              onAction(action);
            }}
          >
            {label}
          </Button>
        ) : null;
      })}
    </div>
  );
}

/** The footer: "What I've sent", "A file came back" and "My documents". */
export function Footer({
  label,
  onSent,
  onCameBack,
  onMyDocuments,
}: {
  label: string;
  onSent: () => void;
  onCameBack: (() => void) | undefined;
  onMyDocuments: (() => void) | undefined;
}) {
  return (
    <footer className="row" style={{ flexWrap: "wrap" }}>
      <LinkButton onClick={onSent}>{label}</LinkButton>
      {onCameBack && <LinkButton onClick={onCameBack}>A file came back</LinkButton>}
      {onMyDocuments && <LinkButton onClick={onMyDocuments}>My documents</LinkButton>}
    </footer>
  );
}

export function MasterHome({
  panel,
  busy,
  onOffer,
  onOpen,
  onAction,
  onSent,
  onCameBack,
  onReviewReturned,
  onMyDocuments,
}: {
  panel: MasterPanel;
  busy: boolean;
  onOffer: (versions: readonly string[]) => void;
  onOpen: (storageRef: string) => void;
  onAction: (action: Action) => void;
  onSent: () => void;
  onCameBack?: () => void;
  onReviewReturned?: (returned: string) => void;
  /** Opens My documents in the browser; absent when this build doesn't know where it is. */
  onMyDocuments?: (() => void) | undefined;
}) {
  return (
    <div className="stack">
      <Header name={panel.fileName} chip={panel.chip} kind="master" line={panel.follows} />
      {onReviewReturned && (
        <ReturnedCallout rows={panel.returned} busy={busy} onReview={onReviewReturned} />
      )}
      <LatestCheckpoint card={panel.latest} />
      {panel.callout && (
        <section className="callout stack" style={{ gap: 8 }} aria-label="Changes to offer">
          <p>{panel.callout.text}</p>
          <Button
            primary
            disabled={busy}
            onClick={() => {
              onOffer(panel.callout?.versions ?? []);
            }}
          >
            {panel.callout.button}
          </Button>
        </section>
      )}
      {panel.versions.length > 0 && (
        <section className="stack" style={{ gap: 8 }} aria-labelledby="versions">
          <h2 id="versions">Tailored versions</h2>
          <ul className="list">
            {panel.versions.map((v) => (
              <li key={v.id} className="card row row--spread">
                <div>
                  <p>{v.name}</p>
                  <p className="secondary small">{v.status}</p>
                </div>
                <Button
                  aria-label={`${v.button} ${v.name}`}
                  disabled={busy}
                  onClick={() => {
                    onOpen(v.storageRef);
                  }}
                >
                  {v.button}
                </Button>
              </li>
            ))}
          </ul>
        </section>
      )}
      <Actions labels={panel.actions} busy={busy} onAction={onAction} />
      <Footer
        label={panel.footer}
        onSent={onSent}
        onCameBack={onCameBack}
        onMyDocuments={onMyDocuments}
      />
    </div>
  );
}

export function VersionHome({
  panel,
  busy,
  deck = false,
  bringsIn = false,
  onReview,
  onAction,
  onSent,
  onCameBack,
  onReviewReturned,
  onMyDocuments,
}: {
  panel: VersionPanel;
  busy: boolean;
  /**
   * A deck. Its updates are brought in from this panel when PowerPoint can
   * change its slides (`bringsIn`, decision 0019), else from the master deck's
   * panel once it's closed (decision 0018).
   */
  deck?: boolean;
  bringsIn?: boolean;
  onReview: () => void;
  onAction: (action: Action) => void;
  onSent: () => void;
  onCameBack?: (() => void) | undefined;
  onReviewReturned?: (returned: string) => void;
  /** Opens My documents in the browser; absent when this build doesn't know where it is. */
  onMyDocuments?: (() => void) | undefined;
}) {
  return (
    <div className="stack">
      <Header name={panel.fileName} chip={panel.chip} kind="variant" line={panel.status} />
      {onReviewReturned && (
        <ReturnedCallout rows={panel.returned} busy={busy} onReview={onReviewReturned} />
      )}
      {panel.updates && (
        <section className="callout stack" style={{ gap: 8 }} aria-label="Updates from the master">
          <p>{panel.updates}</p>
          {deck && !bringsIn ? (
            <p className="small">
              PowerPoint can't change a deck while it's open. Close this deck, then choose Update
              versions in the master deck: this deck gets the changes, and a checkpoint from before.
            </p>
          ) : (
            <Button primary disabled={busy} onClick={onReview}>
              Review them
            </Button>
          )}
        </section>
      )}
      <LatestCheckpoint card={panel.latest} />
      <Actions labels={panel.actions} busy={busy} onAction={onAction} />
      <Footer
        label={panel.footer}
        onSent={onSent}
        onCameBack={onCameBack}
        onMyDocuments={onMyDocuments}
      />
    </div>
  );
}

export function StartTracking({
  name,
  saved,
  storage,
  busy,
  onStart,
  cameBack,
  onCameBack,
}: {
  name: string;
  saved: boolean;
  storage: string;
  busy: boolean;
  onStart: () => void;
  /** Set when this file looks like a copy that came back. */
  cameBack?: Recognition | null;
  onCameBack?: () => void;
}) {
  const looksReturned = cameBack && cameBack.verdict !== "unknown";
  return (
    <div className="stack">
      <h1>{name}</h1>
      {looksReturned && (
        <section className="callout stack" style={{ gap: 8 }} aria-label="This file came back">
          <p style={{ fontWeight: 600 }}>
            {cameBack.verdict === "certain"
              ? `This looks like a copy that came back: ${cameBack.headline.replace(/^This came back from /, "from ")}.`
              : "This looks like a copy you sent that came back."}
          </p>
          <p className="small">
            Review its changes instead of tracking it as a new file. Lineage shows only what changed
            since it left.
          </p>
          <Button primary disabled={busy} onClick={onCameBack}>
            Review it as a file that came back
          </Button>
        </section>
      )}
      <p>
        Lineage keeps a history of this file and lets you make tailored versions that follow it.
        When you change this file, each version is offered the change, and you choose what to bring
        in.
      </p>
      <p className="secondary small">History is kept in {storage}. Your file stays where it is.</p>
      {!saved && (
        <p className="warning" id="save-first">
          Save this file first, then start tracking it.
        </p>
      )}
      <Button
        primary={!looksReturned}
        wide
        disabled={busy || !saved}
        aria-describedby={saved ? undefined : "save-first"}
        onClick={onStart}
      >
        Start tracking
      </Button>
    </div>
  );
}

/**
 * A tracked file whose history isn't in this person's storage. In someone
 * else's drive, it was shared with them: they edit it as usual, and its owner
 * sees their changes (decision 0032). Nothing here writes to the file.
 */
export function Elsewhere({
  storage,
  where,
}: {
  storage: string;
  where: "sharedWithYou" | "unknown";
}) {
  if (where === "sharedWithYou") {
    return (
      <div className="stack">
        <h1>Shared with you</h1>
        <p>
          The person who shared this file keeps its history in their OneDrive. Edit it as usual;
          they&apos;ll see your changes in Lineage.
        </p>
        <p className="small secondary">
          Lineage leaves this file as it is: it doesn&apos;t start tracking it, or change it.
        </p>
      </div>
    );
  }
  return (
    <div className="stack">
      <h1>History not found</h1>
      <p>This file is tracked by Lineage, but its history isn&apos;t in {storage}.</p>
      <ul className="small">
        <li>If you started tracking it, sign in with the account you used then.</li>
        <li>If someone shared it with you, its history is in their OneDrive.</li>
      </ul>
    </div>
  );
}

export function SignIn({
  busy,
  onSignIn,
  onPage,
}: {
  busy: boolean;
  onSignIn: () => void;
  /** Opens Privacy or Terms in the browser; absent when this build doesn't know where they are. */
  onPage?: ((page: SitePage) => void) | undefined;
}) {
  return (
    <div className="stack">
      <h1>Lineage</h1>
      <p>Sign in with your Microsoft account to keep Lineage's history in your OneDrive.</p>
      <p className="secondary small">
        Lineage asks only for its own folder in your OneDrive (Apps › Lineage). Nothing is sent
        anywhere else.
      </p>
      <Button primary wide disabled={busy} onClick={onSignIn}>
        Sign in with Microsoft
      </Button>
      {onPage && (
        <nav className="row" aria-label="About Lineage">
          <LinkButton
            onClick={() => {
              onPage("privacy");
            }}
          >
            Privacy
          </LinkButton>
          <LinkButton
            onClick={() => {
              onPage("terms");
            }}
          >
            Terms
          </LinkButton>
        </nav>
      )}
    </div>
  );
}
