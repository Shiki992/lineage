// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// "What changed", and the updates a tailored version is offered. Removed text
// is marked − and struck through, added text + and underlined; a moved
// paragraph shows as a deletion where it was and an insertion where it is now
// (decision 0007). Overlaps show both sides, and changes Lineage can't write
// show why. The person decides each card; nothing is written until Apply.

import type {
  CardDecision,
  ChangeCard,
  Choice,
  Decision,
  OverlapCard,
  Updates,
  WhatChanged,
} from "@lineage/shell";
import { useState } from "react";
import { Back, Button, Segments } from "./parts";
import type { CellActions } from "./workbook";
import { WorkbookWhatChanged } from "./workbook";

function Place({ place }: { place: string | null }) {
  return place ? <p className="secondary small">{place}</p> : null;
}

function Choices({
  card,
  choices,
  chosen,
  onChoose,
}: {
  card: string;
  choices: readonly Choice[];
  chosen: Decision | undefined;
  onChoose: (card: string, decision: Decision | undefined) => void;
}) {
  return (
    <div className="row" role="group" aria-label="Your decision" style={{ flexWrap: "wrap" }}>
      {choices.map((c) => (
        <Button
          key={c.decision}
          aria-pressed={chosen === c.decision}
          onClick={() => {
            onChoose(card, chosen === c.decision ? undefined : c.decision);
          }}
        >
          {c.label}
        </Button>
      ))}
    </div>
  );
}

export function ChangeView({
  card,
  chosen,
  onChoose,
}: {
  card: ChangeCard;
  chosen?: Decision | undefined;
  onChoose?: ((card: string, decision: Decision | undefined) => void) | undefined;
}) {
  return (
    <li className="card stack" style={{ gap: 6 }}>
      <h3>{card.summary}</h3>
      <Place place={card.place} />
      {card.text.length > 0 && <Segments segments={card.text} />}
      {card.byHand && <p className="warning small">{card.byHand}</p>}
      {onChoose && card.choices.length > 0 && (
        <Choices card={card.id} choices={card.choices} chosen={chosen} onChoose={onChoose} />
      )}
    </li>
  );
}

export function OverlapView({
  card,
  chosen,
  onChoose,
}: {
  card: OverlapCard;
  chosen: Decision | undefined;
  onChoose: (card: string, decision: Decision | undefined) => void;
}) {
  return (
    <li className="card stack" style={{ gap: 6 }}>
      <h3>{card.summary}</h3>
      <Place place={card.place} />
      <div className="sides">
        <div className="side side--mine">
          <p className="small secondary">Yours</p>
          <Segments segments={card.mine} />
        </div>
        <div className="side side--masters">
          <p className="small secondary">The master's</p>
          <Segments segments={card.masters} />
        </div>
      </div>
      <Choices card={card.id} choices={card.choices} chosen={chosen} onChoose={onChoose} />
    </li>
  );
}

/** A read-only "what changed", for saving a checkpoint. */
export function WhatChangedView({
  view,
  cells,
}: {
  view: WhatChanged;
  cells?: CellActions | undefined;
}) {
  const workbook = view.workbook;
  if (workbook) {
    return <WorkbookWhatChanged view={{ ...view, workbook }} cells={cells} />;
  }
  return (
    <div className="stack">
      <p role="status">{view.headline}</p>
      {view.changes.length > 0 && (
        <ul className="list" aria-label="Changes">
          {view.changes.map((c) => (
            <ChangeView key={c.id} card={c} />
          ))}
        </ul>
      )}
      {view.formatting.length > 0 && (
        <section className="stack" style={{ gap: 8 }} aria-labelledby="formatting-only">
          <h2 id="formatting-only">Formatting only</h2>
          <ul className="list">
            {view.formatting.map((c) => (
              <ChangeView key={c.id} card={c} />
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}

function count(n: number, one: string, many: string): string {
  return `${String(n)} ${n === 1 ? one : many}`;
}

/** The updates from the master, decided one by one, then applied together. */
export function UpdatesView({
  updates,
  busy,
  onApply,
  onBack,
}: {
  updates: Updates;
  busy: boolean;
  onApply: (cards: CardDecision[]) => void;
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
  const decided = [...chosen.entries()].map(([card, decision]) => ({ card, decision }));
  const bringing = decided.filter(
    (d) => d.decision !== "decline" && d.decision !== "keepMine",
  ).length;
  const leaving = decided.length - bringing;
  const total = updates.groups.reduce(
    (n, g) =>
      n +
      g.view.changes.length +
      g.view.formatting.length +
      g.view.overlaps.length +
      g.view.byHand.length,
    0,
  );

  return (
    <div className="stack">
      <Back onClick={onBack} />
      <h1>{updates.headline}</h1>
      {updates.groups.map((group) => {
        const v = group.view;
        const offered =
          v.changes.length + v.formatting.length + v.overlaps.length + v.byHand.length;
        if (offered === 0 && v.alreadyThere.length === 0) {
          return null;
        }
        return (
          <section key={group.checkpoint} className="stack" aria-label={group.label}>
            <h2>{group.label}</h2>
            {v.changes.length > 0 && (
              <ul className="list">
                {v.changes.map((c) => (
                  <ChangeView key={c.id} card={c} chosen={chosen.get(c.id)} onChoose={choose} />
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
            {v.formatting.length > 0 && (
              <>
                <h3>Formatting only</h3>
                <ul className="list">
                  {v.formatting.map((c) => (
                    <ChangeView key={c.id} card={c} chosen={chosen.get(c.id)} onChoose={choose} />
                  ))}
                </ul>
              </>
            )}
            {v.byHand.length > 0 && (
              <>
                <h3>Copy these in by hand</h3>
                <p className="secondary small">
                  Lineage can't make these changes safely yet. Copy them in Word if you want them;
                  Lineage notices them when you next save a checkpoint.
                </p>
                <ul className="list">
                  {v.byHand.map((c) => (
                    <ChangeView key={c.id} card={c} chosen={chosen.get(c.id)} onChoose={choose} />
                  ))}
                </ul>
              </>
            )}
            {v.alreadyThere.length > 0 && (
              <details>
                <summary>Already in this version ({v.alreadyThere.length})</summary>
                <ul className="list">
                  {v.alreadyThere.map((c) => (
                    <ChangeView key={c.id} card={c} />
                  ))}
                </ul>
              </details>
            )}
          </section>
        );
      })}
      {total > 0 && (
        <section className="panel stack" style={{ gap: 8 }} aria-label="Apply your decisions">
          <p role="status">
            {decided.length === 0
              ? "Choose what to bring in."
              : `${count(bringing, "change", "changes")} to bring in, ${count(leaving, "to leave out", "to leave out")}.${
                  decided.length < total
                    ? ` ${count(total - decided.length, "update waits", "updates wait")} for later.`
                    : ""
                }`}
          </p>
          <p className="secondary small">
            Brought-in changes appear in your file as tracked changes by Lineage, so you can review
            them in Word.
          </p>
          <Button
            primary
            wide
            disabled={busy || decided.length === 0}
            onClick={() => {
              onApply(decided);
            }}
          >
            Apply
          </Button>
        </section>
      )}
    </div>
  );
}
