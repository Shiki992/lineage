// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// "What I've sent": every copy that left, newest first, by month, each opened
// exactly as it was sent, and shared as a link if the person chooses
// (decision 0032).

import type { SentList, SentRow } from "@lineage/shell";
import { useId, useState } from "react";
import { Back, Button, Chip, LinkButton } from "./parts";

/** A live link's actions: copy it, or stop it after saying what that does. */
function LinkActions({ row, busy, onStop }: { row: SentRow; busy: boolean; onStop: () => void }) {
  const [state, setState] = useState<"idle" | "copied" | "manual" | "confirm">("idle");
  const url = row.link?.url ?? "";
  if (state === "confirm") {
    return (
      <div className="callout stack" style={{ gap: 6 }} role="group" aria-label="Stop sharing">
        <p>Stop sharing with {row.recipient}? The link stops working for everyone who has it.</p>
        <div className="row row--wrap">
          <Button className="button--destructive" disabled={busy} onClick={onStop}>
            Stop sharing
          </Button>
          <Button
            onClick={() => {
              setState("idle");
            }}
          >
            Keep sharing
          </Button>
        </div>
      </div>
    );
  }
  return (
    <div className="stack" style={{ gap: 4 }}>
      <div className="row row--wrap">
        <Button
          aria-label={`Copy the link for ${row.recipient}`}
          onClick={() => {
            navigator.clipboard.writeText(url).then(
              () => {
                setState("copied");
              },
              () => {
                setState("manual");
              },
            );
          }}
        >
          Copy link
        </Button>
        <LinkButton
          aria-label={`Stop sharing with ${row.recipient}`}
          disabled={busy}
          onClick={() => {
            setState("confirm");
          }}
        >
          Stop sharing
        </LinkButton>
      </div>
      {state === "copied" && (
        <p className="small secondary" role="status">
          Copied.
        </p>
      )}
      {state === "manual" && (
        <p className="small secondary mono" role="status">
          {url}
        </p>
      )}
    </div>
  );
}

export function WhatIveSent({
  list,
  busy,
  onSearch,
  onOpen,
  onReview,
  onCameBack,
  onBack,
  edited = [],
  onShare,
  onStopSharing,
  onReviewShared,
}: {
  list: SentList;
  busy: boolean;
  onSearch: (search: string) => void;
  onOpen: (sent: string) => void;
  /** "Review" on a copy that came back with changes. */
  onReview?: (returned: string) => void;
  onCameBack?: () => void;
  onBack: () => void;
  /** Sent copies whose edit links were used to change them. */
  edited?: readonly string[];
  /** "Share a link"; absent when the storage can't share. */
  onShare?: (sent: string, recipient: string) => void;
  onStopSharing?: (sent: string) => void;
  /** "Review their changes" on a shared copy they edited. */
  onReviewShared?: (sent: string) => void;
}) {
  const [search, setSearch] = useState("");
  const id = useId();
  const empty = list.groups.length === 0;
  return (
    <div className="stack">
      <Back onClick={onBack} />
      <h1>{list.title}</h1>
      {onCameBack && (
        <div>
          <Button disabled={busy} onClick={onCameBack}>
            A file came back
          </Button>
        </div>
      )}
      <form
        role="search"
        className="row"
        onSubmit={(e) => {
          e.preventDefault();
          onSearch(search);
        }}
      >
        <label htmlFor={id} className="visually-hidden">
          Search what you've sent
        </label>
        <input
          id={id}
          className="input"
          style={{ flex: 1 }}
          type="search"
          placeholder="Search by who, version or role"
          value={search}
          onChange={(e) => {
            setSearch(e.target.value);
          }}
        />
        <Button type="submit" disabled={busy}>
          Search
        </Button>
      </form>
      {empty && (
        <p className="secondary">
          {search.trim() === ""
            ? "Nothing sent yet. Use “Send a copy” to make a clean copy and record it."
            : "No copies match that."}
        </p>
      )}
      {list.groups.map((group) => (
        <section
          key={group.month}
          className="stack"
          style={{ gap: 8 }}
          aria-labelledby={`${id}-${group.month}`}
        >
          <h2 id={`${id}-${group.month}`}>{group.month}</h2>
          <ul className="list">
            {group.rows.map((row) => (
              <li key={row.id} className="card stack" style={{ gap: 4 }}>
                <div className="row row--spread">
                  <p style={{ fontWeight: 600 }}>{row.recipient}</p>
                  {row.chip && <Chip kind="accent">{row.chip}</Chip>}
                </div>
                <p className="small">{row.version}</p>
                {row.purpose && <p className="secondary small">{row.purpose}</p>}
                {row.link && (
                  <p className="small">
                    {row.link.line}
                    {row.link.live && <span className="secondary"> · {row.link.who}</span>}
                  </p>
                )}
                {edited.includes(row.id) && onReviewShared && (
                  <div className="callout stack" style={{ gap: 6 }}>
                    <p>{row.recipient} changed the shared copy.</p>
                    <div>
                      <Button
                        primary
                        aria-label={`Review ${row.recipient}'s changes to the shared copy`}
                        disabled={busy}
                        onClick={() => {
                          onReviewShared(row.id);
                        }}
                      >
                        Review their changes
                      </Button>
                    </div>
                  </div>
                )}
                {row.link?.live && onStopSharing ? (
                  <LinkActions
                    row={row}
                    busy={busy}
                    onStop={() => {
                      onStopSharing(row.id);
                    }}
                  />
                ) : (
                  onShare && (
                    <div>
                      <LinkButton
                        aria-label={`Share a link with ${row.recipient}`}
                        disabled={busy}
                        onClick={() => {
                          onShare(row.id, row.recipient);
                        }}
                      >
                        Share a link
                      </LinkButton>
                    </div>
                  )
                )}
                <div className="row row--spread">
                  <p className="secondary small">
                    {row.format} · {row.date}
                  </p>
                  {row.returned && row.button === "Review" && onReview ? (
                    <Button
                      primary
                      aria-label={`Review what ${row.recipient} sent back`}
                      disabled={busy}
                      onClick={() => {
                        onReview(row.returned ?? "");
                      }}
                    >
                      {row.button}
                    </Button>
                  ) : (
                    <Button
                      aria-label={`Open what ${row.recipient} received`}
                      disabled={busy}
                      onClick={() => {
                        onOpen(row.id);
                      }}
                    >
                      Open
                    </Button>
                  )}
                </div>
              </li>
            ))}
          </ul>
        </section>
      ))}
      <p className="secondary small">{list.note}</p>
    </div>
  );
}
