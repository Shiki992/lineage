// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Files that come back (BLUEPRINT §3, "Asking for feedback"): choose or drop
// the file, see which copy it is (or choose, when Lineage isn't sure), then go
// through their changes one at a time, keeping or skipping each. Nothing is
// written until Apply, and kept changes land as tracked changes by Lineage.
// For a file saved by another app, kept changes bring only their words, in the
// file's own formatting, unless the person asks for theirs (decision 0014).

import type {
  Candidate,
  CardDecision,
  ChangeCard,
  Decision,
  MatchedBy,
  OverlapCard,
  Recognition,
  ReturnReview,
  ReturnedRow,
  Segment,
} from "@lineage/shell";
import { useId, useRef, useState } from "react";
import { Back, Button, LinkButton, Segments } from "./parts";

/** Files over this are refused before they are read. */
export const MAX_RETURNED_BYTES = 50 * 1024 * 1024;

function isDocx(name: string): boolean {
  return /\.docx$/i.test(name.trim());
}

/** "A file came back": the file picker and a drop zone. */
export function ChooseReturned({
  busy,
  onFile,
  onBack,
}: {
  busy: boolean;
  onFile: (bytes: Uint8Array, name: string) => void;
  onBack: () => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const hint = useId();

  const take = (file: File | undefined) => {
    if (!file) {
      return;
    }
    if (!isDocx(file.name)) {
      setProblem("Choose a Word file (.docx). Lineage can't read other kinds of file yet.");
      return;
    }
    if (file.size > MAX_RETURNED_BYTES) {
      setProblem("That file is too large for Lineage to read.");
      return;
    }
    setProblem(null);
    void file.arrayBuffer().then((buffer) => {
      onFile(new Uint8Array(buffer), file.name);
    });
  };

  return (
    <div className="stack">
      <Back onClick={onBack} />
      <h1>A file came back</h1>
      <p>
        Choose the file someone sent back to you. Lineage finds which copy it is and shows only
        their changes. Nothing in your file changes until you decide.
      </p>
      <div
        className={over ? "drop drop--over" : "drop"}
        onDragOver={(e) => {
          e.preventDefault();
          setOver(true);
        }}
        onDragLeave={() => {
          setOver(false);
        }}
        onDrop={(e) => {
          e.preventDefault();
          setOver(false);
          if (!busy) {
            take(e.dataTransfer.files[0]);
          }
        }}
      >
        <p id={hint}>Drop the file here, or</p>
        <Button
          primary
          disabled={busy}
          aria-describedby={hint}
          onClick={() => {
            input.current?.click();
          }}
        >
          Choose file
        </Button>
        <input
          ref={input}
          type="file"
          accept=".docx,application/vnd.openxmlformats-officedocument.wordprocessingml.document"
          className="visually-hidden"
          tabIndex={-1}
          aria-hidden="true"
          data-testid="returned-file"
          onChange={(e) => {
            take(e.target.files?.[0]);
            e.target.value = "";
          }}
        />
      </div>
      {problem && (
        <p className="warning" role="alert">
          {problem}
        </p>
      )}
      <p className="secondary small">
        Lineage keeps the file exactly as it came back, in your history.
      </p>
    </div>
  );
}

function CandidateText({ candidate }: { candidate: Candidate }) {
  return (
    <span className="stack" style={{ gap: 2 }}>
      <span style={{ fontWeight: 600 }}>{candidate.title}</span>
      <span className="small">{candidate.detail}</span>
      <span className="secondary small">{candidate.score}</span>
    </span>
  );
}

/** "Which copy is this?": a sure match to confirm, a choice, or not recognised. */
export function CameBack({
  name,
  recognition,
  busy,
  onConfirm,
  onBack,
}: {
  /** The returned file's name. */
  name: string;
  recognition: Recognition;
  busy: boolean;
  onConfirm: (candidate: Candidate, matchedBy: MatchedBy) => void;
  onBack: () => void;
}) {
  const [chosen, setChosen] = useState<number | "none" | null>(null);
  const group = useId();
  const first = recognition.candidates[0];

  if (recognition.verdict === "unknown" || !first) {
    return (
      <div className="stack">
        <Back onClick={onBack} />
        <p className="secondary small">{name}</p>
        <h1>{recognition.headline}</h1>
        {recognition.note && <p>{recognition.note}</p>}
        <Button wide onClick={onBack}>
          Choose another file
        </Button>
      </div>
    );
  }

  if (recognition.verdict === "certain") {
    return (
      <div className="stack">
        <Back onClick={onBack} />
        <p className="secondary small">{name}</p>
        <h1>{recognition.headline}</h1>
        {recognition.note && <p>{recognition.note}</p>}
        <section className="card" aria-label="The copy it came from">
          <CandidateText candidate={first} />
        </section>
        <Button
          primary
          wide
          disabled={busy}
          onClick={() => {
            onConfirm(first, recognition.byIds ? "ids" : "content");
          }}
        >
          Review their changes
        </Button>
      </div>
    );
  }

  const picked = typeof chosen === "number" ? recognition.candidates[chosen] : undefined;
  return (
    <div className="stack">
      <Back onClick={onBack} />
      <p className="secondary small">{name}</p>
      <h1>{recognition.headline}</h1>
      {recognition.note && <p>{recognition.note}</p>}
      <fieldset className="choices">
        <legend className="visually-hidden">The copy it came from</legend>
        {recognition.candidates.map((c, i) => (
          <label key={`${c.checkpoint}-${c.sent ?? "unsent"}`} className="choice card">
            <input
              type="radio"
              name={group}
              checked={chosen === i}
              onChange={() => {
                setChosen(i);
              }}
            />
            <CandidateText candidate={c} />
          </label>
        ))}
        <label className="choice card">
          <input
            type="radio"
            name={group}
            checked={chosen === "none"}
            onChange={() => {
              setChosen("none");
            }}
          />
          <span>None of these</span>
        </label>
      </fieldset>
      <Button
        primary
        wide
        disabled={busy || chosen === null}
        onClick={() => {
          if (picked) {
            onConfirm(picked, "person");
          } else {
            onBack();
          }
        }}
      >
        {chosen === "none" ? "Choose another file" : "Continue"}
      </Button>
    </div>
  );
}

/** Where the changes are reviewed, when that file isn't the open one. */
export function OpenToReview({
  name,
  hereName,
  busy,
  onOpen,
  onReviewHere,
  onBack,
}: {
  /** The file the copy was made from. */
  name: string;
  /** The open file, when its changes could go here instead. */
  hereName: string | null;
  busy: boolean;
  onOpen: () => void;
  onReviewHere: () => void;
  onBack: () => void;
}) {
  return (
    <div className="stack">
      <Back onClick={onBack} />
      <h1>Lineage kept the file</h1>
      <p>
        Its changes are reviewed in “{name}”, the file the copy was made from. Open it, then choose
        Review in its panel.
      </p>
      <Button primary wide disabled={busy} onClick={onOpen}>
        Open “{name}”
      </Button>
      {hereName && (
        <LinkButton disabled={busy} onClick={onReviewHere}>
          Review them here, in “{hereName}”, instead
        </LinkButton>
      )}
    </div>
  );
}

/** Files that came back, on the master or version panel. */
export function ReturnedCallout({
  rows,
  busy,
  onReview,
}: {
  rows: readonly ReturnedRow[];
  busy: boolean;
  onReview: (returned: string) => void;
}) {
  if (rows.length === 0) {
    return null;
  }
  return (
    <section className="callout stack" style={{ gap: 8 }} aria-label="Files that came back">
      {rows.map((row) => (
        <div key={row.id} className="row row--spread">
          <div>
            <p>{row.text}</p>
            <p className="small">{row.detail}</p>
          </div>
          <Button
            primary
            disabled={busy}
            aria-label={`${row.button}: ${row.text}`}
            onClick={() => {
              onReview(row.id);
            }}
          >
            {row.button}
          </Button>
        </div>
      ))}
    </section>
  );
}

type Step =
  | { kind: "change"; card: ChangeCard }
  | { kind: "overlap"; card: OverlapCard }
  | { kind: "formatting"; cards: ChangeCard[] }
  | { kind: "summary" };

function count(n: number, one: string, many: string): string {
  return `${String(n)} ${n === 1 ? one : many}`;
}

/** True when a side changes words (not only formatting). */
function changesWords(segments: readonly Segment[]): boolean {
  return segments.some((s) => s.kind === "added" || s.kind === "removed");
}

/** The review of a returned file, one change at a time. */
export function ReviewReturned({
  review,
  isSource,
  busy,
  onApply,
  onOpenSource,
  onOpenReturned,
  onBack,
}: {
  review: ReturnReview;
  /** True when the open file is the one the copy was made from. */
  isSource: boolean;
  busy: boolean;
  /** `wordsOnly`: kept changes bring their words in this file's formatting. */
  onApply: (cards: CardDecision[], wordsOnly: boolean) => void;
  onOpenSource: () => void;
  onOpenReturned: () => void;
  onBack: () => void;
}) {
  const v = review.view;
  const steps: Step[] = [
    ...v.changes.map((card) => ({ kind: "change" as const, card })),
    ...v.overlaps.map((card) => ({ kind: "overlap" as const, card })),
    ...v.byHand.map((card) => ({ kind: "change" as const, card })),
    ...(v.formatting.length > 0 ? [{ kind: "formatting" as const, cards: v.formatting }] : []),
    { kind: "summary" as const },
  ];
  const singles = steps.filter((s) => s.kind === "change" || s.kind === "overlap").length;
  const [at, setAt] = useState(0);
  const [chosen, setChosen] = useState<ReadonlyMap<string, Decision>>(new Map());
  // Lineage suggests words only for a file another app saved; the person can
  // ask for their formatting too.
  const suggested = review.formatting.wordsOnly;
  const [theirFormatting, setTheirFormatting] = useState(false);
  const wordsOnly = suggested && !theirFormatting;
  const formattingBox = useId();
  const step = steps[Math.min(at, steps.length - 1)] ?? { kind: "summary" as const };

  const decide = (card: string, decision: Decision, advance = true) => {
    setChosen((current) => new Map(current).set(card, decision));
    if (advance) {
      setAt((i) => Math.min(i + 1, steps.length - 1));
    }
  };
  const decided = [...chosen.entries()].map(([card, decision]) => ({ card, decision }));
  const keeping = decided.filter(
    (d) => d.decision !== "decline" && d.decision !== "keepMine",
  ).length;
  const total = v.changes.length + v.overlaps.length + v.byHand.length + v.formatting.length;

  const header = (
    <>
      <Back onClick={onBack} />
      <h1>{review.headline}</h1>
      <p className="secondary small">{review.from}</p>
      {review.notes.map((note) => (
        <p key={note} className="small">
          {note}
        </p>
      ))}
      {wordsOnly && review.formatting.note && (
        <p className="callout small">{review.formatting.note}</p>
      )}
      {suggested && !wordsOnly && (
        <p className="secondary small">
          Their formatting will come with their words, as you chose.
        </p>
      )}
      <div className="row" style={{ flexWrap: "wrap" }}>
        {!isSource && (
          <LinkButton disabled={busy} onClick={onOpenSource}>
            Open “{review.fromName}” instead
          </LinkButton>
        )}
        <LinkButton disabled={busy} onClick={onOpenReturned}>
          Open what came back
        </LinkButton>
      </div>
    </>
  );

  if (total === 0) {
    return (
      <div className="stack">
        {header}
        <p role="status">Nothing left to review.</p>
      </div>
    );
  }

  const choiceButtons = (
    card: string,
    choices: readonly { decision: Decision; label: string }[],
  ) => (
    <div className="row" role="group" aria-label="Your decision" style={{ flexWrap: "wrap" }}>
      {choices.map((c) => (
        <Button
          key={c.decision}
          primary={c.decision === "bringIn" || c.decision === "useMasters"}
          aria-pressed={chosen.get(card) === c.decision}
          disabled={busy}
          onClick={() => {
            decide(card, c.decision);
          }}
        >
          {c.label}
        </Button>
      ))}
    </div>
  );

  const nav = (
    <div className="row row--spread">
      <LinkButton
        disabled={at === 0}
        onClick={() => {
          setAt((i) => Math.max(i - 1, 0));
        }}
      >
        ← Previous
      </LinkButton>
      <LinkButton
        onClick={() => {
          setAt((i) => Math.min(i + 1, steps.length - 1));
        }}
      >
        Decide later →
      </LinkButton>
    </div>
  );

  let body: React.ReactNode;
  if (step.kind === "change") {
    const card = step.card;
    body = (
      <section className="card stack" style={{ gap: 6 }} aria-labelledby="this-change">
        <h2 id="this-change">{card.summary}</h2>
        {card.place && <p className="secondary small">{card.place}</p>}
        {card.text.length > 0 && <Segments segments={card.text} wordsOnly={wordsOnly} />}
        {card.byHand && <p className="warning small">{card.byHand}</p>}
        {choiceButtons(card.id, card.choices)}
      </section>
    );
  } else if (step.kind === "overlap") {
    const card = step.card;
    body = (
      <section className="card stack" style={{ gap: 6 }} aria-labelledby="this-change">
        <h2 id="this-change">{card.summary}</h2>
        {card.place && <p className="secondary small">{card.place}</p>}
        <div className="sides">
          <div className="side side--mine">
            <p className="small secondary">Yours</p>
            <Segments segments={card.mine} />
          </div>
          <div className="side side--masters">
            <p className="small secondary">{review.theirs}</p>
            <Segments segments={card.masters} wordsOnly={wordsOnly && changesWords(card.masters)} />
          </div>
        </div>
        {choiceButtons(card.id, card.choices)}
      </section>
    );
  } else if (step.kind === "formatting") {
    const cards = step.cards;
    body = (
      <section className="card stack" style={{ gap: 8 }} aria-labelledby="formatting">
        <h2 id="formatting">
          {suggested ? "The formatting changed" : "They also changed formatting"} in{" "}
          {count(cards.length, "place", "places")}
        </h2>
        <p className="small">
          {review.formatting.formattingNote ??
            "Fonts, sizes and spacing often change when a file is opened in another app, such as Google Docs."}{" "}
          You can skip them all, or go through them below.
        </p>
        <Button
          primary
          disabled={busy}
          onClick={() => {
            setChosen((current) => {
              const next = new Map(current);
              for (const c of cards) {
                next.set(c.id, "decline");
              }
              return next;
            });
            setAt((i) => Math.min(i + 1, steps.length - 1));
          }}
        >
          Skip all formatting
        </Button>
        <details>
          <summary>Go through them</summary>
          <ul className="list">
            {cards.map((c) => (
              <li key={c.id} className="stack" style={{ gap: 4 }}>
                <p>{c.summary}</p>
                <div className="row" role="group" aria-label={`Your decision: ${c.summary}`}>
                  {c.choices.map((choice) => (
                    <Button
                      key={choice.decision}
                      aria-pressed={chosen.get(c.id) === choice.decision}
                      disabled={busy}
                      onClick={() => {
                        decide(c.id, choice.decision, false);
                      }}
                    >
                      {choice.label}
                    </Button>
                  ))}
                </div>
              </li>
            ))}
          </ul>
        </details>
      </section>
    );
  } else {
    body = (
      <section className="panel stack" style={{ gap: 8 }} aria-label="Apply your decisions">
        <p role="status">
          {decided.length === 0
            ? "You haven't kept or skipped anything yet."
            : `${count(keeping, "change", "changes")} to keep, ${count(decided.length - keeping, "to skip", "to skip")}.${
                decided.length < total
                  ? ` ${count(total - decided.length, "change waits", "changes wait")} for later.`
                  : ""
              }`}
        </p>
        {suggested && (
          <label className="row" htmlFor={formattingBox} style={{ minHeight: "var(--target)" }}>
            <input
              id={formattingBox}
              type="checkbox"
              checked={theirFormatting}
              onChange={(e) => {
                setTheirFormatting(e.target.checked);
              }}
            />
            <span>Bring in their formatting too</span>
          </label>
        )}
        <p className="secondary small">
          {wordsOnly
            ? "Kept changes appear in your file in your formatting, as tracked changes by Lineage, so you can review them in Word."
            : "Kept changes appear in your file as tracked changes by Lineage, so you can review them in Word."}
        </p>
        <Button
          primary
          wide
          disabled={busy || decided.length === 0}
          onClick={() => {
            onApply(decided, wordsOnly);
          }}
        >
          Apply
        </Button>
        <LinkButton
          onClick={() => {
            setAt(0);
          }}
        >
          Go through the changes again
        </LinkButton>
      </section>
    );
  }

  return (
    <div className="stack">
      {header}
      {step.kind === "change" || step.kind === "overlap" ? (
        <p className="secondary small" role="status">
          Change {String(Math.min(at, singles - 1) + 1)} of {String(singles)}
        </p>
      ) : null}
      {body}
      {step.kind !== "summary" && nav}
      {v.alreadyThere.length > 0 && (
        <details>
          <summary>Already in your file ({v.alreadyThere.length})</summary>
          <ul className="list">
            {v.alreadyThere.map((c) => (
              <li key={c.id} className="card">
                {c.summary}
              </li>
            ))}
          </ul>
        </details>
      )}
    </div>
  );
}
