// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// "My documents": every document family the person tracks, with search, a
// format filter and sorting. The list draws from the library index at once,
// then from every ledger once they are checked (decision 0015).

import type { LibraryEntry } from "@lineage/shell";
import { userMessage } from "@lineage/shell";
import { useEffect, useMemo, useState } from "react";
import type { Documents, LibraryView } from "./documents";
import { Alert, Chip, FormatIcon, Loading } from "./parts";
import { href } from "./route";

type Sort = "recent" | "name" | "format";
type Format = "all" | "docx" | "pptx" | "xlsx";

type State =
  | { readonly kind: "loading" }
  | { readonly kind: "ready"; readonly library: LibraryView; readonly checking: boolean }
  | { readonly kind: "error"; readonly message: string };

function formatOf(entry: LibraryEntry): Exclude<Format, "all"> {
  if (entry.format === "pptx" || entry.format === "gslides") {
    return "pptx";
  }
  if (entry.format === "xlsx" || entry.format === "gsheet") {
    return "xlsx";
  }
  return "docx";
}

export function arrange(
  entries: readonly LibraryEntry[],
  search: string,
  format: Format,
  sort: Sort,
): LibraryEntry[] {
  const needle = search.trim().toLowerCase();
  const shown = entries.filter(
    (e) =>
      (format === "all" || formatOf(e) === format) &&
      (needle === "" || `${e.name} ${e.masterFile ?? ""} ${e.kind}`.toLowerCase().includes(needle)),
  );
  const byName = (a: LibraryEntry, b: LibraryEntry) => a.name.localeCompare(b.name);
  switch (sort) {
    case "name":
      return shown.sort(byName);
    case "format":
      return shown.sort((a, b) => formatOf(a).localeCompare(formatOf(b)) || byName(a, b));
    default:
      return shown.sort((a, b) => b.lastAt - a.lastAt || byName(a, b));
  }
}

export function LibraryScreen({ documents, where }: { documents: Documents; where: string }) {
  const [state, setState] = useState<State>({ kind: "loading" });
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    let live = true;
    documents
      .list((quick) => {
        if (live) {
          setState({ kind: "ready", library: quick, checking: true });
        }
      })
      .then((library) => {
        if (live) {
          setState({ kind: "ready", library, checking: false });
        }
      })
      .catch((error: unknown) => {
        console.error(error);
        if (live) {
          setState({ kind: "error", message: userMessage(error) });
        }
      });
    return () => {
      live = false;
    };
  }, [documents, attempt]);

  return (
    <section className="stack" aria-labelledby="library-title">
      <h1 id="library-title">My documents</h1>
      {state.kind === "loading" && <Loading what={`Reading your history from ${where}…`} />}
      {state.kind === "error" && (
        <Alert>
          <p>{state.message}</p>
          <button
            type="button"
            className="button"
            onClick={() => {
              setState({ kind: "loading" });
              setAttempt((n) => n + 1);
            }}
          >
            Try again
          </button>
        </Alert>
      )}
      {state.kind === "ready" && <Entries library={state.library} checking={state.checking} />}
    </section>
  );
}

function Entries({ library, checking }: { library: LibraryView; checking: boolean }) {
  const [search, setSearch] = useState("");
  const [format, setFormat] = useState<Format>("all");
  const [sort, setSort] = useState<Sort>("recent");
  const shown = useMemo(
    () => arrange(library.entries, search, format, sort),
    [library.entries, search, format, sort],
  );

  if (library.entries.length === 0) {
    return (
      <div className="panel stack empty">
        <h2>Nothing tracked yet</h2>
        <p>{library.empty ?? "You don't track any documents yet."}</p>
        <ol className="steps">
          <li>Open your master resume or deck in Word.</li>
          <li>Choose Lineage on the Home tab.</li>
          <li>Choose Start tracking. It appears here.</li>
        </ol>
      </div>
    );
  }

  return (
    <div className="stack">
      <div className="toolbar" role="search">
        <div className="field">
          <label htmlFor="search">Search</label>
          <input
            id="search"
            className="input"
            type="search"
            value={search}
            placeholder="Name or file"
            onChange={(e) => {
              setSearch(e.target.value);
            }}
          />
        </div>
        <div className="field">
          <label htmlFor="format">Format</label>
          <select
            id="format"
            className="input"
            value={format}
            onChange={(e) => {
              setFormat(e.target.value as Format);
            }}
          >
            <option value="all">All formats</option>
            <option value="docx">Word</option>
            <option value="pptx">PowerPoint</option>
            <option value="xlsx">Excel</option>
          </select>
        </div>
        <div className="field">
          <label htmlFor="sort">Sort by</label>
          <select
            id="sort"
            className="input"
            value={sort}
            onChange={(e) => {
              setSort(e.target.value as Sort);
            }}
          >
            <option value="recent">Last change</option>
            <option value="name">Name</option>
            <option value="format">Format</option>
          </select>
        </div>
      </div>
      <p className="secondary small" role="status" aria-live="polite">
        {checking
          ? "Checking for changes…"
          : shown.length === library.entries.length
            ? `${String(shown.length)} ${shown.length === 1 ? "document" : "documents"}`
            : `${String(shown.length)} of ${String(library.entries.length)} documents`}
      </p>
      {shown.length === 0 ? (
        <p className="panel">No documents match “{search.trim()}”.</p>
      ) : (
        <ul className="list" aria-label="Documents">
          {shown.map((entry) => (
            <li key={entry.familyId}>
              <a
                className="row-link card"
                href={href({ page: "document", family: entry.familyId, tab: "versions" })}
              >
                <FormatIcon format={entry.format} />
                <span className="stack" style={{ gap: 2 }}>
                  <span className="row-title">{entry.name}</span>
                  <span className="secondary small">
                    {entry.kind}
                    {entry.masterFile ? ` · ${entry.masterFile}` : ""}
                  </span>
                  <span className="small">{entry.summary}</span>
                  <span className="secondary small">{entry.lastActivity}</span>
                  {entry.attention && (
                    <span>
                      <Chip kind="accent">{entry.attention}</Chip>
                    </span>
                  )}
                  {entry.damage && <span className="warning-text small">{entry.damage}</span>}
                </span>
              </a>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
