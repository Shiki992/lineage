// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Workbook screens (decisions 0022, 0023): "What changed" in the five
// categories, with "Go to cell" and a cell side by side, and the review of
// the master's updates or a returned workbook's changes, brought into the
// open workbook by Lineage when the person presses the button.

import type {
  CardDecision,
  CellDetail,
  Decision,
  SideBySide,
  WhatChanged,
  WorkbookCard,
  WorkbookView,
} from "@lineage/shell";
import { useId, useState } from "react";
import { Back, Button, LinkButton } from "./parts";

/** What a screen can do with a cell of the open workbook. */
export interface CellActions {
  /** Shows the cell in Excel; absent when the host can't. */
  goTo?: ((sheet: string, address: string) => void) | undefined;
  /** The cell beside the same cell at the latest checkpoint. */
  sideBySide?: ((sheet: string, address: string) => Promise<SideBySide>) | undefined;
}

/** A single cell ("B4"), not a range or whole rows. */
const SINGLE = /^[A-Z]{1,3}[1-9][0-9]*$/;

function Detail({ label, cell }: { label: string; cell: CellDetail | null }) {
  return (
    <div className="side stack" style={{ gap: 2 }}>
      <p className="small secondary">{label}</p>
      {cell ? (
        <dl className="small detail">
          <dt>Cell</dt>
          <dd className="mono">{cell.address}</dd>
          <dt>Shows</dt>
          <dd>{cell.shown === "" ? "(empty)" : cell.shown}</dd>
          {cell.formula && (
            <>
              <dt>Formula</dt>
              <dd className="mono">{cell.formula}</dd>
            </>
          )}
          <dt>{cell.formula ? "Last result" : "Stored"}</dt>
          <dd className="mono">{cell.value === "" ? "(empty)" : cell.value}</dd>
          <dt>Kind</dt>
          <dd>{cell.kind}</dd>
          {cell.format && (
            <>
              <dt>Format</dt>
              <dd className="mono">{cell.format}</dd>
            </>
          )}
        </dl>
      ) : (
        <p className="small">Not in this version.</p>
      )}
    </div>
  );
}

function Choices({
  card,
  chosen,
  onChoose,
}: {
  card: WorkbookCard;
  chosen: Decision | undefined;
  onChoose: (card: string, decision: Decision | undefined) => void;
}) {
  return (
    <div className="row" role="group" aria-label="Your decision" style={{ flexWrap: "wrap" }}>
      {card.choices.map((c) => (
        <Button
          key={c.decision}
          aria-pressed={chosen === c.decision}
          onClick={() => {
            onChoose(card.id, chosen === c.decision ? undefined : c.decision);
          }}
        >
          {c.label}
        </Button>
      ))}
    </div>
  );
}

function Card({
  card,
  cells,
  chosen,
  onChoose,
}: {
  card: WorkbookCard;
  cells?: CellActions | undefined;
  chosen?: Decision | undefined;
  onChoose?: ((card: string, decision: Decision | undefined) => void) | undefined;
}) {
  const [side, setSide] = useState<SideBySide | "loading" | "failed" | null>(null);
  const address = card.address;
  const showSide = (
    sheet: string,
    at: string,
    load: (s: string, a: string) => Promise<SideBySide>,
  ) => {
    setSide("loading");
    load(sheet, at).then(setSide, (e: unknown) => {
      console.error(e);
      setSide("failed");
    });
  };
  return (
    <li className="card stack" style={{ gap: 6 }}>
      <p style={{ fontWeight: 600 }}>{card.summary}</p>
      {card.place && <p className="secondary small">{card.place}</p>}
      {card.cells > 1 && <p className="secondary small">{String(card.cells)} cells</p>}
      {card.why && <p className="secondary small">{card.why}</p>}
      {card.conflict && card.mine && (
        <div className="sides">
          <div className="side side--mine">
            <p className="small secondary">Yours</p>
            <p className="mono small">{card.mine.formula ?? card.mine.shown}</p>
          </div>
          <div className="side side--masters">
            <p className="small secondary">The master's</p>
            <p className="mono small">{card.after?.formula ?? card.after?.shown ?? ""}</p>
          </div>
        </div>
      )}
      {card.byHand && <p className="warning small">{card.byHand}</p>}
      {address && cells && (cells.goTo !== undefined || cells.sideBySide !== undefined) && (
        <div className="row" style={{ flexWrap: "wrap" }}>
          {cells.goTo && (
            <LinkButton
              onClick={() => {
                cells.goTo?.(card.sheet, address);
              }}
            >
              Go to {address.includes(":") ? "cells" : "cell"}
            </LinkButton>
          )}
          {cells.sideBySide && SINGLE.test(address) && side === null && (
            <LinkButton
              onClick={() => {
                if (cells.sideBySide) {
                  showSide(card.sheet, address, cells.sideBySide);
                }
              }}
            >
              Side by side
            </LinkButton>
          )}
        </div>
      )}
      {side === "loading" && (
        <p className="small secondary" role="status">
          Looking at the cell…
        </p>
      )}
      {side === "failed" && <p className="small warning">Lineage couldn't show this cell.</p>}
      {side !== null && typeof side === "object" && (
        <div className="sides" aria-label="Side by side">
          <Detail label="At the last checkpoint" cell={side.before} />
          <Detail label="Now" cell={side.after} />
        </div>
      )}
      {onChoose && card.choices.length > 0 && (
        <Choices card={card} chosen={chosen} onChoose={onChoose} />
      )}
    </li>
  );
}

/** One workbook view: its groups, rows Lineage couldn't match, and parts it keeps untracked. */
export function WorkbookChanges({
  view,
  cells,
  chosen,
  onChoose,
}: {
  view: WorkbookView;
  cells?: CellActions | undefined;
  chosen?: ReadonlyMap<string, Decision> | undefined;
  onChoose?: ((card: string, decision: Decision | undefined) => void) | undefined;
}) {
  const base = useId();
  return (
    <div className="stack">
      {view.groups.map((group, i) => (
        <section
          key={group.category}
          className="stack"
          style={{ gap: 8 }}
          aria-labelledby={`${base}-${String(i)}`}
        >
          <h2 id={`${base}-${String(i)}`}>{group.title}</h2>
          <p className="secondary small">{group.summary}</p>
          <ul className="list">
            {group.cards.map((card) => (
              <Card
                key={card.id}
                card={card}
                cells={cells}
                chosen={chosen?.get(card.id)}
                onChoose={onChoose}
              />
            ))}
          </ul>
          {group.more > 0 && <p className="secondary small">And {String(group.more)} more.</p>}
        </section>
      ))}
      {view.unmatched.length > 0 && (
        <section
          className="warning stack"
          style={{ gap: 4 }}
          aria-label="Rows Lineage couldn't match"
        >
          {view.unmatched.map((u) => (
            <p key={`${u.sheet}:${u.summary}`} className="small">
              {u.summary}
            </p>
          ))}
        </section>
      )}
      {view.untracked.length > 0 && (
        <section className="stack" style={{ gap: 4 }} aria-labelledby={`${base}-kept`}>
          <h2 id={`${base}-kept`}>Kept as they are</h2>
          <ul className="small secondary">
            {view.untracked.map((line) => (
              <li key={line}>{line}</li>
            ))}
          </ul>
        </section>
      )}
      {view.notes.map((note) => (
        <p key={note} className="secondary small">
          {note}
        </p>
      ))}
    </div>
  );
}

/** "What changed" in a workbook, read-only, for saving a checkpoint. */
export function WorkbookWhatChanged({
  view,
  cells,
}: {
  view: WhatChanged & { workbook: WorkbookView };
  cells?: CellActions | undefined;
}) {
  return (
    <div className="stack">
      <p role="status">{view.headline}</p>
      <WorkbookChanges view={view.workbook} cells={cells} />
    </div>
  );
}

function plural(n: number, one: string, many: string): string {
  return `${String(n)} ${n === 1 ? one : many}`;
}

/**
 * The master's updates, or a returned workbook's changes, decided card by
 * card and brought into the open workbook together.
 */
export function WorkbookReview({
  title,
  groups,
  verb,
  canBringIn,
  busy,
  onApply,
  onBack,
}: {
  title: string;
  groups: readonly { key: string; label: string | null; view: WorkbookView }[];
  /** "Bring in" for updates, "Keep" for a returned workbook. */
  verb: "Bring in" | "Keep";
  /** False when this Excel can't carry the changes out (decision 0020). */
  canBringIn: boolean;
  busy: boolean;
  onApply: (cards: CardDecision[]) => void;
  onBack: () => void;
}) {
  const [chosen, setChosen] = useState<ReadonlyMap<string, Decision>>(new Map());
  const cards = groups.flatMap((g) => g.view.groups.flatMap((w) => w.cards));
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
  const decided: CardDecision[] = cards.flatMap((card) => {
    const decision = chosen.get(card.id);
    return decision ? card.ids.map((id) => ({ card: id, decision })) : [];
  });
  const bringing = cards.filter((c) => {
    const d = chosen.get(c.id);
    return d !== undefined && d !== "decline" && d !== "keepMine";
  }).length;
  const open = cards.filter((c) => c.choices.length > 0 && !chosen.has(c.id)).length;
  const action =
    bringing > 0 ? `${verb} ${plural(bringing, "change", "changes")}` : "Save my choices";
  return (
    <div className="stack">
      <Back onClick={onBack} />
      <h1>{title}</h1>
      {!canBringIn && (
        <p className="warning">
          This version of Excel can't take the changes. Update Excel to bring them in; until then,
          this is what changed.
        </p>
      )}
      {groups.map((g) => (
        <section key={g.key} className="stack" aria-label={g.label ?? title}>
          {g.label && <h2>{g.label}</h2>}
          <WorkbookChanges
            view={g.view}
            chosen={chosen}
            onChoose={canBringIn ? choose : undefined}
          />
        </section>
      ))}
      {canBringIn && chosen.size > 0 && (
        <section className="panel stack" style={{ gap: 8 }} aria-label={verb}>
          <p role="status">
            {bringing > 0
              ? `${plural(bringing, "change", "changes")} to ${verb.toLowerCase()}.`
              : "Nothing to bring in."}
            {open > 0 ? ` ${plural(open, "card waits", "cards wait")} for later.` : ""}
          </p>
          <p className="secondary small">
            Lineage makes the changes in this workbook, checks them, and saves a checkpoint. If
            anything goes wrong, it takes its changes back out.
          </p>
          <Button
            primary
            wide
            disabled={busy || decided.length === 0}
            onClick={() => {
              onApply(decided);
            }}
          >
            {action}
          </Button>
        </section>
      )}
    </div>
  );
}
