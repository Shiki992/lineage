// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// "What I've sent": every copy that left, newest first, by month, each opened
// exactly as it was sent.

import type { SentList } from "@lineage/shell";
import { useId, useState } from "react";
import { Back, Button, Chip } from "./parts";

export function WhatIveSent({
  list,
  busy,
  onSearch,
  onOpen,
  onBack,
}: {
  list: SentList;
  busy: boolean;
  onSearch: (search: string) => void;
  onOpen: (sent: string) => void;
  onBack: () => void;
}) {
  const [search, setSearch] = useState("");
  const id = useId();
  const empty = list.groups.length === 0;
  return (
    <div className="stack">
      <Back onClick={onBack} />
      <h1>{list.title}</h1>
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
                <div className="row row--spread">
                  <p className="secondary small">
                    {row.format} · {row.date}
                  </p>
                  <Button
                    aria-label={`${row.button} what ${row.recipient} received`}
                    disabled={busy}
                    onClick={() => {
                      onOpen(row.id);
                    }}
                  >
                    {row.button}
                  </Button>
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
