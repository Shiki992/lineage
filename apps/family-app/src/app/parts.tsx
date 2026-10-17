// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Small pieces every screen uses: real buttons with 44 px targets, chips, and
// removed and added text marked by more than colour (the same as the Word pane).

import type { Segment, WhatChanged } from "@lineage/shell";
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
      : format === "xlsx" || format === "gsheet"
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

/** "What changed" between two checkpoints: content first, formatting apart. */
export function Changes({ view }: { view: WhatChanged }) {
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
