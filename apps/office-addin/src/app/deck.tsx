// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The pitch deck panel (BLUEPRINT screen 4, decision 0018): what just changed
// in the master deck, the tailored versions to update with what each has, the
// last copy sent with "Open what they saw", and after "Update N versions", a
// line per version, with anything both changed left for the person to choose.

import type { Decision, DeckPanel, UpdateRun, Updates, VersionChoice } from "@lineage/shell";
import { useState } from "react";
import { ChangeView, OverlapView } from "./changes";
import type { Action } from "./home";
import { Actions, Footer } from "./home";
import { Back, Button, Header, LatestCheckpoint } from "./parts";

function updateLabel(n: number): string {
  return n === 1 ? "Update 1 version" : `Update ${String(n)} versions`;
}

export function DeckHome({
  panel,
  busy,
  onUpdate,
  onCheck,
  onOpen,
  onOpenSent,
  onAction,
  onSent,
  onMyDocuments,
}: {
  panel: DeckPanel;
  busy: boolean;
  /** "Update N versions", for the ticked versions; `note` names what changed. */
  onUpdate: (versions: string[], note: string) => void;
  /** PowerPoint has no change event: the person asks for a fresh look. */
  onCheck: () => void;
  onOpen: (storageRef: string) => void;
  onOpenSent: (sent: string) => void;
  onAction: (action: Action) => void;
  onSent: () => void;
  onMyDocuments?: (() => void) | undefined;
}) {
  const master = panel.master;
  const [ticked, setTicked] = useState<ReadonlySet<string>>(
    () => new Set(panel.targets.filter((t) => t.checked).map((t) => t.id)),
  );
  const changed = panel.justChanged;
  const offering = changed !== null || panel.targets.some((t) => t.checked);
  const toggle = (id: string) => {
    setTicked((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  };
  const chosen = panel.targets.filter((t) => ticked.has(t.id)).map((t) => t.id);

  return (
    <div className="stack">
      <Header name={master.fileName} chip={master.chip} kind="master" line={master.follows} />
      {changed && (
        <section className="callout stack" style={{ gap: 8 }} aria-labelledby="just-changed">
          <h2 id="just-changed">{changed.headline}</h2>
          <ul className="list">
            {changed.cards.map((c) => (
              <ChangeView key={c.id} card={c} />
            ))}
          </ul>
        </section>
      )}
      {offering && panel.targets.length > 0 && (
        <fieldset className="stack card" style={{ gap: 8 }}>
          <legend>Which tailored versions get {changed ? "it" : "the master's changes"}?</legend>
          {panel.targets.map((t) => (
            <label key={t.id} className="check">
              <input
                type="checkbox"
                checked={ticked.has(t.id)}
                disabled={busy}
                onChange={() => {
                  toggle(t.id);
                }}
              />
              <span>
                {t.name}
                <span className="secondary small block">{t.context}</span>
              </span>
            </label>
          ))}
          <Button
            primary
            wide
            disabled={busy || chosen.length === 0}
            onClick={() => {
              onUpdate(
                chosen,
                changed ? `Changed ${changed.headline.replace(/^You just changed /, "")}` : "",
              );
            }}
          >
            {updateLabel(chosen.length)}
          </Button>
          <p className="secondary small">
            PowerPoint has no tracked changes, so the versions are changed directly. Each one keeps
            a checkpoint from before, and anything you also changed there waits for your choice.
          </p>
        </fieldset>
      )}
      {/* Always there: more edits can follow the ones a call-out shows. */}
      <div className="row">
        <Button disabled={busy} onClick={onCheck}>
          Check for changes
        </Button>
      </div>
      <LatestCheckpoint card={master.latest} />
      {panel.lastSent && (
        <section className="card row row--spread" aria-label="Last copy sent">
          <p>{panel.lastSent.text}</p>
          <Button
            disabled={busy}
            onClick={() => {
              onOpenSent(panel.lastSent?.sent ?? "");
            }}
          >
            {panel.lastSent.button}
          </Button>
        </section>
      )}
      {master.versions.length > 0 && (
        <section className="stack" style={{ gap: 8 }} aria-labelledby="deck-versions">
          <h2 id="deck-versions">Tailored versions</h2>
          <ul className="list">
            {master.versions.map((v) => (
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
      <Actions labels={master.actions} busy={busy} onAction={onAction} />
      <Footer
        label={master.footer}
        onSent={onSent}
        onCameBack={undefined}
        onMyDocuments={onMyDocuments}
      />
    </div>
  );
}

function changes(n: number): string {
  return n === 1 ? "1 change" : `${String(n)} changes`;
}

/**
 * A tailored deck open in PowerPoint: the master's changes it was offered,
 * brought in from here (decision 0019). Everything only the master changed
 * comes in; what both changed waits for Keep mine or Use the master's; design
 * changes and anything Lineage can't write stay to copy in by hand.
 */
export function DeckVersionReview({
  updates,
  node,
  busy,
  onBringIn,
  onBack,
}: {
  updates: Updates;
  /** This version's node, for the choices. */
  node: string;
  busy: boolean;
  onBringIn: (choices: VersionChoice[]) => void;
  onBack: () => void;
}) {
  const [chosen, setChosen] = useState<ReadonlyMap<string, Decision>>(new Map());
  const choose = (card: string, decision: Decision | undefined) => {
    setChosen((current) => {
      const next = new Map(current);
      if (decision) {
        next.set(card, decision);
      } else {
        next.delete(card);
      }
      return next;
    });
  };
  const views = updates.groups.map((g) => g.view);
  const clean = views.reduce((n, v) => n + v.changes.length + v.formatting.length, 0);
  const overlaps = views.reduce((n, v) => n + v.overlaps.length, 0);
  const theirs = [...chosen.values()].filter((d) => d !== "keepMine" && d !== "decline").length;
  const bringing = clean + theirs;
  const waiting = overlaps - chosen.size;
  const choices: VersionChoice[] = [...chosen.entries()].map(([card, decision]) => ({
    node,
    card,
    decision,
  }));
  const label =
    bringing > 0 ? `Bring in ${changes(bringing)}` : chosen.size > 0 ? "Keep mine" : "Bring in";

  return (
    <div className="stack">
      <Back onClick={onBack} />
      <h1>{updates.headline}</h1>
      {updates.groups.map((group) => {
        const v = group.view;
        const offered =
          v.changes.length + v.formatting.length + v.overlaps.length + v.byHand.length;
        if (offered === 0) {
          return null;
        }
        return (
          <section key={group.checkpoint} className="stack" aria-label={group.label}>
            <h2>{group.label}</h2>
            {v.changes.length + v.formatting.length > 0 && (
              <ul className="list">
                {[...v.changes, ...v.formatting].map((c) => (
                  <ChangeView key={c.id} card={c} />
                ))}
              </ul>
            )}
            {v.overlaps.length > 0 && (
              <>
                <h3>You both changed these</h3>
                <ul className="list">
                  {v.overlaps.map((c) => (
                    <OverlapView key={c.id} card={c} chosen={chosen.get(c.id)} onChoose={choose} />
                  ))}
                </ul>
              </>
            )}
            {v.byHand.length > 0 && (
              <>
                <h3>Copy these in by hand</h3>
                <p className="secondary small">
                  Lineage can't make these changes safely, such as a new design or a slide this deck
                  has no layout for. Copy them in PowerPoint if you want them.
                </p>
                <ul className="list">
                  {v.byHand.map((c) => (
                    <ChangeView key={c.id} card={c} />
                  ))}
                </ul>
              </>
            )}
          </section>
        );
      })}
      {(bringing > 0 || chosen.size > 0) && (
        <section className="panel stack" style={{ gap: 8 }} aria-label="Bring in">
          <p role="status">
            {bringing > 0 ? `${changes(bringing)} to bring in.` : "Nothing to bring in."}
            {waiting > 0
              ? ` ${changes(waiting)} you both made ${waiting === 1 ? "waits" : "wait"} until you choose.`
              : ""}
          </p>
          <p className="secondary small">
            PowerPoint has no tracked changes, so each changed slide is replaced in this deck.
            Unsaved edits are kept as a checkpoint first, and the checkpoint before is the way back.
          </p>
          <Button
            primary
            wide
            disabled={busy}
            onClick={() => {
              onBringIn(choices);
            }}
          >
            {label}
          </Button>
        </section>
      )}
    </div>
  );
}

/** After "Update N versions": a line per version, and choices for what both changed. */
export function UpdateReport({
  run,
  busy,
  onChoose,
  onDone,
}: {
  run: UpdateRun;
  busy: boolean;
  onChoose: (versions: string[], choices: VersionChoice[]) => void;
  onDone: () => void;
}) {
  const [chosen, setChosen] = useState<ReadonlyMap<string, Decision>>(new Map());
  const key = (node: string, card: string) => `${node}:${card}`;
  const choose = (node: string) => (card: string, decision: Decision | undefined) => {
    setChosen((current) => {
      const next = new Map(current);
      if (decision) {
        next.set(key(node, card), decision);
      } else {
        next.delete(key(node, card));
      }
      return next;
    });
  };
  const choices: VersionChoice[] = [...chosen.entries()].map(([k, decision]) => {
    const [node = "", card = ""] = k.split(":");
    return { node, card, decision };
  });
  const waiting = run.versions.some((v) => v.left.overlaps.length > 0);

  return (
    <div className="stack">
      <Back onClick={onDone} />
      <h1 role="status">{run.headline}</h1>
      <ul className="list" aria-label="Versions">
        {run.versions.map((v) => (
          <li key={v.node} className="card stack" style={{ gap: 6 }}>
            <h2>{v.name}</h2>
            <p>{v.text}</p>
            {v.savedEdits && (
              <p className="secondary small">
                Your own edits to this version were saved as a checkpoint first.
              </p>
            )}
            {v.left.overlaps.length > 0 && (
              <ul className="list">
                {v.left.overlaps.map((c) => (
                  <OverlapView
                    key={c.id}
                    card={c}
                    chosen={chosen.get(key(v.node, c.id))}
                    onChoose={choose(v.node)}
                  />
                ))}
              </ul>
            )}
            {v.left.byHand.length > 0 && (
              <ul className="list">
                {v.left.byHand.map((c) => (
                  <ChangeView key={c.id} card={c} />
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>
      {waiting ? (
        <Button
          primary
          wide
          disabled={busy || choices.length === 0}
          onClick={() => {
            onChoose([...new Set(choices.map((c) => c.node))], choices);
          }}
        >
          Apply my choices
        </Button>
      ) : (
        <Button primary wide disabled={busy} onClick={onDone}>
          Done
        </Button>
      )}
    </div>
  );
}
