// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Small pieces every screen uses: real buttons with 44 px targets, chips, and
// removed and added text marked by more than colour (the same as the Word pane).

import type {
  CodeLine,
  FieldLists,
  PivotFields,
  Segment,
  WhatChanged,
  WorkbookView,
} from "@lineage/shell";
import type { ButtonHTMLAttributes, ReactNode } from "react";

export function Button({
  primary = false,
  className = "",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { primary?: boolean }) {
  const classes = ["button", primary ? "button--primary" : "", className].filter(Boolean).join(" ");
  return <button type="button" className={classes} {...rest} />;
}

export type ChipKind = "master" | "variant" | "accent" | "milestone" | "warning";

export function Chip({ kind, children }: { kind: ChipKind; children: ReactNode }) {
  return <span className={`chip chip--${kind}`}>{children}</span>;
}

/** The format as a small labelled badge: W, P or X, with its name for screen readers. */
export function FormatIcon({ format }: { format: string }) {
  const [letter, name, kind] =
    format === "pptx" || format === "gslides"
      ? ["P", "PowerPoint", "pptx"]
      : format === "xlsx" || format === "xlsm" || format === "gsheet"
        ? ["X", "Excel", "xlsx"]
        : ["W", "Word", "docx"];
  return (
    <span className={`format format--${kind}`} role="img" aria-label={name}>
      {letter}
    </span>
  );
}

export function Segments({ segments }: { segments: readonly Segment[] }) {
  return (
    <p className="what">
      {segments.map((s, i) => {
        const key = `${String(i)}-${s.kind}`;
        switch (s.kind) {
          case "removed":
            return (
              <del key={key}>
                <span className="visually-hidden">removed: </span>
                {s.text}
              </del>
            );
          case "added":
            return (
              <ins key={key}>
                <span className="visually-hidden">added: </span>
                {s.text}
              </ins>
            );
          case "reformatted":
            return (
              <span key={key} className="reformatted">
                <span className="visually-hidden">reformatted: </span>
                {s.text}
              </span>
            );
          default:
            return <span key={key}>{s.text}</span>;
        }
      })}
    </p>
  );
}

type Row =
  | { kind: "skipped"; text: string }
  | { kind: "pair"; before: CodeLine | null; after: CodeLine | null };

/** Unchanged lines on both sides; each run of removed lines beside the added lines that replace it. */
function pairs(lines: readonly CodeLine[]): Row[] {
  const rows: Row[] = [];
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line) {
      break;
    }
    if (line.kind === "skipped") {
      rows.push({ kind: "skipped", text: line.text });
      i += 1;
    } else if (line.kind === "same") {
      rows.push({ kind: "pair", before: line, after: line });
      i += 1;
    } else {
      const removed: CodeLine[] = [];
      const added: CodeLine[] = [];
      for (let next = lines[i]; next?.kind === "removed"; next = lines[i]) {
        removed.push(next);
        i += 1;
      }
      for (let next = lines[i]; next?.kind === "added"; next = lines[i]) {
        added.push(next);
        i += 1;
      }
      for (let k = 0; k < Math.max(removed.length, added.length); k += 1) {
        rows.push({ kind: "pair", before: removed[k] ?? null, after: added[k] ?? null });
      }
    }
  }
  return rows;
}

function CodeCell({ line, side }: { line: CodeLine | null; side: "before" | "after" }) {
  if (!line) {
    return <td className="code__cell code__cell--empty" colSpan={2} />;
  }
  const changed = line.kind !== "same";
  const mark = !changed ? " " : side === "before" ? "−" : "+";
  const word = !changed ? "Unchanged" : side === "before" ? "Removed" : "Added";
  const number = side === "before" ? line.before : line.after;
  return (
    <>
      <td className="code__num" aria-hidden="true">
        {number === null ? "" : String(number)}
      </td>
      <td className={`code__cell${changed ? ` code__cell--${line.kind}` : ""}`}>
        <span className="code__mark" aria-hidden="true">
          {mark}
        </span>
        <span className="visually-hidden">{word}: </span>
        <code>{line.text}</code>
      </td>
    </>
  );
}

/** A macro's or query's code, before and after, side by side (Phase 5B). */
function CodeSideBySide({ lines }: { lines: readonly CodeLine[] }) {
  return (
    <div className="code-scroll">
      <table className="code">
        <caption className="visually-hidden">The code, before and after</caption>
        <thead>
          <tr>
            <th scope="col" colSpan={2}>
              Before <span className="code-key code-key--removed">− removed</span>
            </th>
            <th scope="col" colSpan={2}>
              After <span className="code-key code-key--added">+ added</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {pairs(lines).map((row, i) =>
            row.kind === "skipped" ? (
              <tr key={i}>
                <td colSpan={4} className="code__skipped small secondary">
                  {row.text}
                </td>
              </tr>
            ) : (
              <tr key={i}>
                <CodeCell line={row.before} side="before" />
                <CodeCell line={row.after} side="after" />
              </tr>
            ),
          )}
        </tbody>
      </table>
    </div>
  );
}

const FIELD_ROWS: readonly [keyof PivotFields, string][] = [
  ["rows", "Rows"],
  ["columns", "Columns"],
  ["values", "Values"],
  ["filters", "Filters"],
];

/** A pivot table's fields, before and after. */
function FieldsTable({ fields }: { fields: FieldLists }) {
  const listed = (f: PivotFields | null, key: keyof PivotFields) =>
    !f ? "—" : f[key].length === 0 ? "none" : f[key].join(", ");
  return (
    <table className="fields small">
      <caption className="visually-hidden">The pivot table's fields, before and after</caption>
      <thead>
        <tr>
          <th scope="col">
            <span className="visually-hidden">Area</span>
          </th>
          <th scope="col">Before</th>
          <th scope="col">After</th>
        </tr>
      </thead>
      <tbody>
        {FIELD_ROWS.map(([key, name]) => {
          const before = listed(fields.before, key);
          const after = listed(fields.after, key);
          const changed = before !== after;
          return (
            <tr key={key} className={changed ? "fields__changed" : undefined}>
              <th scope="row">
                {name}
                {changed && <span className="visually-hidden"> (changed)</span>}
              </th>
              <td>{before}</td>
              <td>{after}</td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}

/** A workbook's changes in the five categories (decision 0022), read-only. */
function WorkbookChanges({ headline, view }: { headline: string; view: WorkbookView }) {
  return (
    <div className="stack">
      <h3>{headline}</h3>
      {view.groups.map((g) => (
        <section
          key={`${g.category}-${g.object ?? "cells"}`}
          className="stack"
          style={{ gap: 6 }}
          aria-label={g.title}
        >
          <h4>{g.title}</h4>
          <p className="secondary small">{g.summary}</p>
          {g.object === "macros" && view.signature && <p className="small">{view.signature}</p>}
          <ol className="list">
            {g.cards.map((c) => (
              <li key={c.id} className="card stack" style={{ gap: 4 }}>
                <p>
                  <strong>{c.summary}</strong>
                  {c.place ? <span className="secondary"> · {c.place}</span> : null}
                </p>
                {c.why && <p className="secondary small">{c.why}</p>}
                {c.details.length > 0 && (
                  <ul className="small" style={{ margin: 0, paddingLeft: 18 }}>
                    {c.details.map((d) => (
                      <li key={d}>{d}</li>
                    ))}
                  </ul>
                )}
                {c.code.length > 0 && <CodeSideBySide lines={c.code} />}
                {c.fields && <FieldsTable fields={c.fields} />}
              </li>
            ))}
          </ol>
          {g.more > 0 && <p className="secondary small">And {String(g.more)} more.</p>}
        </section>
      ))}
      {view.unmatched.map((u) => (
        <p key={`${u.sheet}:${u.summary}`} className="warning small">
          {u.summary}
        </p>
      ))}
      {view.untracked.length > 0 && (
        <section className="stack" style={{ gap: 4 }} aria-label="Kept as they are">
          <h4>Kept as they are</h4>
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

export function Changes({ view }: { view: WhatChanged }) {
  if (view.workbook) {
    return <WorkbookChanges headline={view.headline} view={view.workbook} />;
  }
  return (
    <div className="stack">
      <h3>{view.headline}</h3>
      {view.changes.length > 0 && (
        <ol className="list">
          {view.changes.map((c) => (
            <li key={c.id} className="card stack" style={{ gap: 6 }}>
              <p>
                <strong>{c.summary}</strong>
                {c.place ? <span className="secondary"> · {c.place}</span> : null}
              </p>
              <Segments segments={c.text} />
            </li>
          ))}
        </ol>
      )}
      {view.formatting.length > 0 && (
        <details>
          <summary>
            {view.formatting.length === 1
              ? "1 formatting change"
              : `${String(view.formatting.length)} formatting changes`}
          </summary>
          <ul className="list">
            {view.formatting.map((c) => (
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

export function Alert({ children }: { children: ReactNode }) {
  return (
    <div className="alert" role="alert">
      {children}
    </div>
  );
}

export function Loading({ what }: { what: string }) {
  return (
    <p className="secondary" role="status" aria-live="polite">
      {what}
    </p>
  );
}
