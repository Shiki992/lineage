// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Small pieces every screen uses. Real buttons and labels, 44 px targets
// (styles/app.css), and removed and added text marked by more than colour.

import type { CheckpointCard, Segment } from "@lineage/shell";
import type { ButtonHTMLAttributes, ReactNode } from "react";

export function Button({
  primary = false,
  wide = false,
  className = "",
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { primary?: boolean; wide?: boolean }) {
  const classes = [
    "button",
    primary ? "button--primary" : "",
    wide ? "button--wide" : "",
    className,
  ]
    .filter(Boolean)
    .join(" ");
  return <button type="button" className={classes} {...rest} />;
}

export function LinkButton(props: ButtonHTMLAttributes<HTMLButtonElement>) {
  return <button type="button" className="button button--link" {...props} />;
}

export function Chip({
  kind,
  children,
}: {
  kind: "master" | "variant" | "accent";
  children: ReactNode;
}) {
  return <span className={`chip chip--${kind}`}>{children}</span>;
}

/** The file's name, its chip and a line under it. */
export function Header({
  name,
  chip,
  kind,
  line,
}: {
  name: string;
  chip: string;
  kind: "master" | "variant";
  line: string;
}) {
  return (
    <header className="stack" style={{ gap: 4 }}>
      <div className="row">
        <h1>{name}</h1>
        <Chip kind={kind}>{chip}</Chip>
      </div>
      <p className="secondary">{line}</p>
    </header>
  );
}

export function LatestCheckpoint({ card }: { card: CheckpointCard }) {
  return (
    <section className="card" aria-label="Latest checkpoint">
      <div className="row row--spread">
        <h2>Latest checkpoint</h2>
        <Chip kind="accent">{card.label}</Chip>
      </div>
      <p>{card.note}</p>
      <p className="secondary small">{card.when}</p>
    </section>
  );
}

/**
 * A paragraph with removed text (−, struck through) and added text (+,
 * underlined). `wordsOnly` draws reformatted text plainly, for changes that
 * will bring their words but not their formatting.
 */
export function Segments({
  segments,
  wordsOnly = false,
}: {
  segments: readonly Segment[];
  wordsOnly?: boolean;
}) {
  return (
    <p className="what">
      {segments.map((s, i) => {
        const key = `${String(i)}-${s.kind}`;
        const kind = wordsOnly && s.kind === "reformatted" ? "same" : s.kind;
        switch (kind) {
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

export function ErrorBanner({ message, onDismiss }: { message: string; onDismiss: () => void }) {
  return (
    <div className="alert stack" role="alert" style={{ gap: 8 }}>
      <p>{message}</p>
      <div>
        <Button onClick={onDismiss}>Dismiss</Button>
      </div>
    </div>
  );
}

export function Back({ onClick }: { onClick: () => void }) {
  return (
    <div>
      <LinkButton onClick={onClick}>← Back</LinkButton>
    </div>
  );
}
