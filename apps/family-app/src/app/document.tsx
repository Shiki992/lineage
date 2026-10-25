// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// A document's page: Versions (each file's checkpoints, compare any two,
// restore as a new version, milestones), Sent copies (open exactly as sent)
// and the Family tree. Nothing on this page changes a file that exists.

import type {
  DocumentPage,
  DocumentView,
  FileHistory,
  GraphEdge,
  GraphNode,
  SentRow,
  TimelineEntry,
  WhatChanged,
} from "@lineage/shell";
import { userMessage } from "@lineage/shell";
import type { KeyboardEvent, ReactNode } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import type { Documents, MissingFile } from "./documents";
import { saveFile, sentFileName } from "./documents";
import { Alert, Button, Changes, Chip, FormatIcon, Loading } from "./parts";
import type { Tab } from "./route";
import { TABS, go, href } from "./route";
import { Tree, describeNode } from "./tree";

type State =
  | { readonly kind: "loading" }
  | {
      readonly kind: "ready";
      readonly view: DocumentView;
      readonly missing: readonly MissingFile[];
    }
  | { readonly kind: "error"; readonly message: string };

const TAB_NAMES: Record<Tab, string> = {
  versions: "Versions",
  sent: "Sent copies",
  tree: "Family tree",
};

export function DocumentScreen({
  documents,
  family,
  tab,
}: {
  documents: Documents;
  family: string;
  tab: Tab;
}) {
  const [state, setState] = useState<State>({ kind: "loading" });
  const [reload, setReload] = useState(0);
  const [notice, setNotice] = useState<string | undefined>();
  const refresh = useCallback((message?: string) => {
    setNotice(message);
    setReload((n) => n + 1);
  }, []);

  useEffect(() => {
    const shown = { still: true };
    (async () => {
      const { view } = await documents.open(family);
      const missing = view.page.canChange ? await documents.missingFiles(view.page) : [];
      if (shown.still) {
        setState({ kind: "ready", view, missing });
      }
    })().catch((error: unknown) => {
      console.error(error);
      if (shown.still) {
        setState({ kind: "error", message: userMessage(error) });
      }
    });
    return () => {
      shown.still = false;
    };
  }, [documents, family, reload]);

  return (
    <article className="stack">
      <p>
        <a className="back" href={href({ page: "library" })}>
          ← My documents
        </a>
      </p>
      {state.kind === "loading" && <Loading what="Reading this document's history…" />}
      {state.kind === "error" && (
        <Alert>
          <p>{state.message}</p>
          <Button
            onClick={() => {
              setState({ kind: "loading" });
              refresh();
            }}
          >
            Try again
          </Button>
        </Alert>
      )}
      {state.kind === "ready" && (
        <Page
          documents={documents}
          view={state.view}
          missing={state.missing}
          tab={tab}
          notice={notice}
          onChanged={refresh}
        />
      )}
    </article>
  );
}

function Page({
  documents,
  view,
  missing,
  tab,
  notice,
  onChanged,
}: {
  documents: Documents;
  view: DocumentView;
  missing: readonly MissingFile[];
  tab: Tab;
  notice: string | undefined;
  onChanged: (message?: string) => void;
}) {
  const { page } = view;
  const tabs = useRef(new Map<Tab, HTMLButtonElement>());
  const onTabKey = (event: KeyboardEvent<HTMLButtonElement>) => {
    const i = TABS.indexOf(tab);
    const next =
      event.key === "ArrowRight"
        ? TABS[(i + 1) % TABS.length]
        : event.key === "ArrowLeft"
          ? TABS[(i + TABS.length - 1) % TABS.length]
          : event.key === "Home"
            ? TABS[0]
            : event.key === "End"
              ? TABS[TABS.length - 1]
              : undefined;
    if (next) {
      event.preventDefault();
      go({ page: "document", family: page.familyId, tab: next });
      tabs.current.get(next)?.focus();
    }
  };

  return (
    <>
      <header className="stack" style={{ gap: 4 }}>
        <div className="row">
          <FormatIcon format={page.format} />
          <h1>{page.name}</h1>
        </div>
        <p className="secondary">{page.kind}</p>
        {page.badges.length > 0 && (
          <p className="row" style={{ gap: 4 }}>
            {page.badges.map((b) => (
              <Chip key={b} kind="warning">
                {b}
              </Chip>
            ))}
          </p>
        )}
      </header>
      {page.note && <p className="warning">{page.note}</p>}
      {notice && (
        <p className="callout" role="status">
          {notice}
        </p>
      )}
      <div className="tabs" role="tablist" aria-label="This document">
        {TABS.map((t) => (
          <button
            key={t}
            ref={(el) => {
              if (el) {
                tabs.current.set(t, el);
              }
            }}
            type="button"
            role="tab"
            id={`tab-${t}`}
            className="tab"
            aria-selected={t === tab}
            aria-controls={`panel-${t}`}
            tabIndex={t === tab ? 0 : -1}
            onClick={() => {
              go({ page: "document", family: page.familyId, tab: t });
            }}
            onKeyDown={onTabKey}
          >
            {TAB_NAMES[t]}
          </button>
        ))}
      </div>
      <section
        className="stack"
        role="tabpanel"
        id={`panel-${tab}`}
        aria-labelledby={`tab-${tab}`}
        tabIndex={0}
      >
        {tab === "versions" && (
          <Versions documents={documents} view={view} missing={missing} onChanged={onChanged} />
        )}
        {tab === "sent" && <Sent documents={documents} view={view} />}
        {tab === "tree" && <FamilyTree documents={documents} view={view} onChanged={onChanged} />}
      </section>
    </>
  );
}

// ------------------------------------------------------------------ Versions

interface Picked {
  readonly id: string;
  readonly name: string;
}

function stem(name: string): string {
  return name.replace(/\.(docx|pptx|xlsx|xlsm)$/i, "");
}

/** "docx", "pptx", "xlsx" or "xlsm", from a file's name. */
function formatOf(name: string): string {
  return /\.(docx|pptx|xlsx|xlsm)$/i.exec(name)?.[1]?.toLowerCase() ?? "docx";
}

/** The extension the family's copies have: a workbook with macros stays .xlsm. */
function copyFormat(page: DocumentPage): string {
  const master = page.files[0];
  return page.format === "xlsx" && master && formatOf(master.name) === "xlsm"
    ? "xlsm"
    : page.format;
}

/** The app a format opens in, for "In Word, …". */
function appFor(format: string): string {
  switch (format) {
    case "pptx":
      return "PowerPoint";
    case "xlsx":
      return "Excel";
    default:
      return "Word";
  }
}

function Versions({
  documents,
  view,
  missing,
  onChanged,
}: {
  documents: Documents;
  view: DocumentView;
  missing: readonly MissingFile[];
  onChanged: (message?: string) => void;
}) {
  const { page } = view;
  const [showArchived, setShowArchived] = useState(false);
  const [picked, setPicked] = useState<readonly Picked[]>([]);
  const [compared, setCompared] = useState<
    { kind: "busy" } | { kind: "done"; view: WhatChanged } | { kind: "error"; message: string }
  >();
  const archived = page.files.filter((f) => f.archived).length;
  const files = page.files.filter((f) => showArchived || !f.archived);

  const toggle = (id: string, name: string) => {
    setCompared(undefined);
    setPicked((p) =>
      p.some((x) => x.id === id) ? p.filter((x) => x.id !== id) : [...p, { id, name }].slice(-2),
    );
  };
  const compare = async () => {
    const [a, b] = picked;
    if (!a || !b) {
      return;
    }
    setCompared({ kind: "busy" });
    try {
      setCompared({ kind: "done", view: await documents.compare(page.familyId, a.id, b.id) });
    } catch (error) {
      setCompared({ kind: "error", message: userMessage(error) });
    }
  };

  return (
    <div className="stack">
      <div className="row row--spread">
        <h2>Versions</h2>
        {archived > 0 && (
          <label className="check">
            <input
              type="checkbox"
              checked={showArchived}
              onChange={(e) => {
                setShowArchived(e.target.checked);
              }}
            />
            Show archived ({archived})
          </label>
        )}
      </div>
      {page.canChange && (
        <div className="compare-bar card" aria-live="polite">
          {picked.length < 2 ? (
            <p className="secondary">
              {picked.length === 0
                ? "Tick two checkpoints to see what changed between them."
                : `Tick one more checkpoint to compare with ${picked[0]?.name ?? ""}.`}
            </p>
          ) : (
            <Button primary onClick={() => void compare()}>
              Compare {picked[0]?.name} with {picked[1]?.name}
            </Button>
          )}
          {compared?.kind === "busy" && <Loading what="Comparing…" />}
          {compared?.kind === "error" && <Alert>{compared.message}</Alert>}
          {compared?.kind === "done" && <Changes view={compared.view} />}
        </div>
      )}
      {files.map((file) => (
        <FileCard
          key={file.node}
          documents={documents}
          familyId={page.familyId}
          canChange={page.canChange}
          file={file}
          missing={missing.some((m) => m.node === file.node)}
          view={view}
          picked={picked}
          onPick={toggle}
          onChanged={onChanged}
        />
      ))}
    </div>
  );
}

function FileCard({
  documents,
  familyId,
  canChange,
  file,
  missing,
  view,
  picked,
  onPick,
  onChanged,
}: {
  documents: Documents;
  familyId: string;
  canChange: boolean;
  file: FileHistory;
  missing: boolean;
  view: DocumentView;
  picked: readonly Picked[];
  onPick: (id: string, name: string) => void;
  onChanged: (message?: string) => void;
}) {
  const kind = file.role === "master" ? "master" : "variant";
  return (
    <section className="panel stack" aria-label={file.name}>
      <div className="stack" style={{ gap: 4 }}>
        <div className="row">
          <h3>{file.name}</h3>
          <Chip kind={kind}>{file.chip}</Chip>
          {file.archived && <span className="secondary small">Archived</span>}
        </div>
        <p className="secondary small">{file.status}</p>
        {file.role === "master" && (
          <p className="secondary small">
            {appFor(view.page.format)} last saw it at{" "}
            <span className="mono">{file.storageRef}</span>. If you moved it, open it in{" "}
            {appFor(view.page.format)} and Lineage will find it again.
          </p>
        )}
      </div>
      {missing && (
        <Relink
          documents={documents}
          familyId={familyId}
          file={file}
          view={view}
          onChanged={onChanged}
        />
      )}
      <ol className="timeline">
        {file.entries.map((entry) => (
          <Entry
            key={entry.id}
            documents={documents}
            familyId={familyId}
            canChange={canChange}
            file={file}
            entry={entry}
            picked={picked.some((p) => p.id === entry.id)}
            onPick={onPick}
            onChanged={onChanged}
          />
        ))}
      </ol>
    </section>
  );
}

function Entry({
  documents,
  familyId,
  canChange,
  file,
  entry,
  picked,
  onPick,
  onChanged,
}: {
  documents: Documents;
  familyId: string;
  canChange: boolean;
  file: FileHistory;
  entry: TimelineEntry;
  picked: boolean;
  onPick: (id: string, name: string) => void;
  onChanged: (message?: string) => void;
}) {
  const [form, setForm] = useState<"restore" | "milestone" | undefined>();
  const name = `${stem(file.name)} ${entry.label}`;
  return (
    <li className="timeline__entry">
      <div className="row row--spread row--wrap">
        <div className="stack" style={{ gap: 2 }}>
          <p>
            <strong>{entry.label}</strong> <span className="secondary">· {entry.date}</span>
            {entry.milestone && (
              <>
                {" "}
                <Chip kind="milestone">{entry.milestone}</Chip>
              </>
            )}
          </p>
          <p>{entry.note}</p>
          {entry.sent && <p className="secondary small">{entry.sent}</p>}
          {entry.author && <p className="secondary small">Saved by {entry.author}</p>}
        </div>
        {canChange && (
          <div className="row row--wrap actions">
            <label className="check">
              <input
                type="checkbox"
                aria-label={`Compare ${name}`}
                checked={picked}
                onChange={() => {
                  onPick(entry.id, name);
                }}
              />
              Compare
            </label>
            <Button
              aria-expanded={form === "restore"}
              aria-label={`Restore as a new version from ${name}`}
              onClick={() => {
                setForm(form === "restore" ? undefined : "restore");
              }}
            >
              Restore as a new version
            </Button>
            <Button
              aria-expanded={form === "milestone"}
              aria-label={`${entry.milestone ? "Change milestone" : "Mark as milestone"} on ${name}`}
              onClick={() => {
                setForm(form === "milestone" ? undefined : "milestone");
              }}
            >
              {entry.milestone ? "Change milestone" : "Mark as milestone"}
            </Button>
          </div>
        )}
      </div>
      {form === "restore" && (
        <RestoreForm
          documents={documents}
          familyId={familyId}
          checkpoint={entry.id}
          suggested={`${stem(file.name)} (${entry.label})`}
          format={formatOf(file.name)}
          from={name}
          onDone={(message) => {
            setForm(undefined);
            onChanged(message);
          }}
        />
      )}
      {form === "milestone" && (
        <MilestoneForm
          documents={documents}
          familyId={familyId}
          entry={entry}
          onDone={(message) => {
            setForm(undefined);
            onChanged(message);
          }}
        />
      )}
    </li>
  );
}

function useAction(): {
  busy: boolean;
  error: string | undefined;
  run: (work: () => Promise<void>) => Promise<void>;
} {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | undefined>();
  const run = async (work: () => Promise<void>) => {
    setBusy(true);
    setError(undefined);
    try {
      await work();
    } catch (e) {
      console.error(e);
      setError(userMessage(e));
    } finally {
      setBusy(false);
    }
  };
  return { busy, error, run };
}

function Form({
  label,
  onSubmit,
  children,
  error,
}: {
  label: string;
  onSubmit: () => Promise<void>;
  children: ReactNode;
  error: string | undefined;
}) {
  return (
    <form
      className="card stack inline-form"
      aria-label={label}
      onSubmit={(e) => {
        e.preventDefault();
        void onSubmit();
      }}
    >
      {children}
      {error && <Alert>{error}</Alert>}
    </form>
  );
}

function RestoreForm({
  documents,
  familyId,
  checkpoint,
  suggested,
  format,
  from,
  onDone,
}: {
  documents: Documents;
  familyId: string;
  checkpoint: string;
  suggested: string;
  /** The family's format: the new version's extension. */
  format: string;
  from: string;
  onDone: (message: string) => void;
}) {
  const [name, setName] = useState(suggested);
  const { busy, error, run } = useAction();
  useEffect(() => {
    let live = true;
    void documents.freeName(suggested, format).then(
      (free) => {
        if (live) {
          setName(stem(free));
        }
      },
      () => undefined,
    );
    return () => {
      live = false;
    };
  }, [documents, suggested, format]);
  return (
    <Form
      label={`Restore ${from} as a new version`}
      error={error}
      onSubmit={() =>
        run(async () => {
          const made = await documents.restoreAsNewVersion(familyId, checkpoint, name, format);
          onDone(`Saved “${made.name}” as a new tailored version. Nothing else was changed.`);
        })
      }
    >
      <p className="small">
        A new tailored version is saved with exactly what {from} held. Your other files stay as they
        are.
      </p>
      <div className="field">
        <label htmlFor={`restore-${checkpoint}`}>Name of the new version</label>
        <input
          id={`restore-${checkpoint}`}
          className="input"
          value={name}
          required
          maxLength={200}
          onChange={(e) => {
            setName(e.target.value);
          }}
        />
      </div>
      <div className="row">
        <button type="submit" className="button button--primary" disabled={busy || !name.trim()}>
          {busy ? "Saving…" : "Save the new version"}
        </button>
      </div>
    </Form>
  );
}

function MilestoneForm({
  documents,
  familyId,
  entry,
  onDone,
}: {
  documents: Documents;
  familyId: string;
  entry: TimelineEntry;
  onDone: (message: string) => void;
}) {
  const [label, setLabel] = useState(entry.milestone ?? "");
  const { busy, error, run } = useAction();
  return (
    <Form
      label={`Milestone for ${entry.label}`}
      error={error}
      onSubmit={() =>
        run(async () => {
          await documents.markMilestone(familyId, entry.id, label.trim());
          onDone(`${entry.label} is marked “${label.trim()}”.`);
        })
      }
    >
      <div className="field">
        <label htmlFor={`milestone-${entry.id}`}>Milestone</label>
        <input
          id={`milestone-${entry.id}`}
          className="input"
          value={label}
          required
          maxLength={60}
          placeholder="Sent to ten companies"
          onChange={(e) => {
            setLabel(e.target.value);
          }}
        />
      </div>
      <div className="row">
        <button type="submit" className="button button--primary" disabled={busy || !label.trim()}>
          {busy ? "Saving…" : "Save milestone"}
        </button>
        {entry.milestone && (
          <Button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                await documents.markMilestone(familyId, entry.id, null);
                onDone(`The milestone on ${entry.label} is taken off.`);
              })
            }
          >
            Remove milestone
          </Button>
        )}
      </div>
    </Form>
  );
}

function Relink({
  documents,
  familyId,
  file,
  view,
  onChanged,
}: {
  documents: Documents;
  familyId: string;
  file: FileHistory;
  view: DocumentView;
  onChanged: (message?: string) => void;
}) {
  const [choices, setChoices] = useState<readonly string[] | undefined>();
  const [chosen, setChosen] = useState<string>();
  const { busy, error, run } = useAction();
  return (
    <div className="warning stack" role="group" aria-label={`${file.name} is not found`}>
      <p>
        <strong>File not found.</strong> “{file.name}” isn&apos;t where Lineage kept it. Its history
        is safe. If you moved or renamed it, choose it here.
      </p>
      {choices === undefined ? (
        <div>
          <Button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                setChoices(await documents.unlinkedFiles(view.page));
              })
            }
          >
            Choose the file
          </Button>
        </div>
      ) : choices.length === 0 ? (
        <p>
          There are no other files in Lineage&apos;s folder. You can restore its last checkpoint as
          a new version instead.
        </p>
      ) : (
        <form
          className="stack"
          onSubmit={(e) => {
            e.preventDefault();
            if (chosen) {
              void run(async () => {
                await documents.relink(familyId, file.node, chosen);
                onChanged(`“${file.name}” is linked to “${chosen}” now.`);
              });
            }
          }}
        >
          <fieldset className="choices">
            <legend>Files in Lineage&apos;s folder</legend>
            {choices.map((c) => (
              <label key={c} className="choice">
                <input
                  type="radio"
                  name={`relink-${file.node}`}
                  value={c}
                  checked={chosen === c}
                  onChange={() => {
                    setChosen(c);
                  }}
                />
                {c}
              </label>
            ))}
          </fieldset>
          <div>
            <button type="submit" className="button button--primary" disabled={busy || !chosen}>
              Link this file
            </button>
          </div>
        </form>
      )}
      {error && <p role="alert">{error}</p>}
    </div>
  );
}

// ------------------------------------------------------------------ Sent copies

function Sent({ documents, view }: { documents: Documents; view: DocumentView }) {
  const { page } = view;
  const { sent } = page;
  const empty = sent.groups.every((g) => g.rows.length === 0);
  // Copies changed through their edit links (decision 0032): only said here;
  // they're reviewed in the panel, where the file is open.
  const [edited, setEdited] = useState<readonly string[]>([]);
  useEffect(() => {
    let live = true;
    documents.sharedEdits(page).then(
      (ids) => {
        if (live) {
          setEdited(ids);
        }
      },
      (e: unknown) => {
        console.error(e);
      },
    );
    return () => {
      live = false;
    };
  }, [documents, page]);
  return (
    <div className="stack">
      <h2>Sent copies</h2>
      <p className="secondary">{sent.note}</p>
      {empty ? (
        <p className="panel">
          No copies sent yet. In {appFor(page.format)}, choose Send a copy in the Lineage panel.
        </p>
      ) : (
        sent.groups.map((g) => (
          <section key={g.month} className="stack" aria-label={g.month}>
            <h3>{g.month}</h3>
            <ul className="list">
              {g.rows.map((row) => (
                <SentCard
                  key={row.id}
                  documents={documents}
                  familyId={page.familyId}
                  row={row}
                  format={copyFormat(page)}
                  canOpen={page.canChange}
                  edited={edited.includes(row.id)}
                  app={appFor(page.format)}
                />
              ))}
            </ul>
          </section>
        ))
      )}
    </div>
  );
}

function SentCard({
  documents,
  familyId,
  row,
  format,
  canOpen,
  edited,
  app,
}: {
  documents: Documents;
  familyId: string;
  row: SentRow;
  format: string;
  canOpen: boolean;
  /** Changed through its edit link since Lineage last saw it. */
  edited: boolean;
  /** "Word", "PowerPoint" or "Excel". */
  app: string;
}) {
  const { busy, error, run } = useAction();
  return (
    <li className="card row row--spread row--wrap">
      <div className="stack" style={{ gap: 2 }}>
        <p>
          <strong>{row.recipient}</strong>
          {row.purpose && <span className="secondary"> · {row.purpose}</span>}
        </p>
        <p className="small">
          {row.version} · {row.format} · {row.date}
        </p>
        {row.chip && (
          <p>
            <Chip kind="accent">{row.chip}</Chip>
          </p>
        )}
        {row.link && (
          <p className="small">
            {row.link.line}
            {row.link.live && <span className="secondary"> · {row.link.who}</span>}
          </p>
        )}
        {edited && (
          <p className="small warning-text">
            {row.recipient} changed the shared copy. Review it from What I&apos;ve sent in the
            Lineage panel in {app}.
          </p>
        )}
        {error && <p role="alert">{error}</p>}
      </div>
      <Button
        disabled={!canOpen || busy}
        aria-label={`${busy ? "Opening" : "Open as sent"} to ${row.recipient}`}
        onClick={() =>
          void run(async () => {
            const bytes = await documents.sentCopy(familyId, row.id);
            saveFile(bytes, sentFileName(row.version, row.recipient, format));
          })
        }
      >
        {busy ? "Opening…" : "Open as sent"}
      </Button>
    </li>
  );
}

// ------------------------------------------------------------------ Family tree

function FamilyTree({
  documents,
  view,
  onChanged,
}: {
  documents: Documents;
  view: DocumentView;
  onChanged: (message?: string) => void;
}) {
  const { graph, page } = view;
  const [showArchived, setShowArchived] = useState(false);
  const [selected, setSelected] = useState<string | undefined>();
  const archived = graph.lanes.filter((l) => l.archived).length;
  const node = graph.nodes.find((n) => n.id === selected);
  return (
    <div className="stack">
      <div className="row row--spread">
        <h2>Family tree</h2>
        {archived > 0 && (
          <label className="check">
            <input
              type="checkbox"
              checked={showArchived}
              onChange={(e) => {
                setShowArchived(e.target.checked);
              }}
            />
            Show archived ({archived})
          </label>
        )}
      </div>
      <Tree graph={graph} showArchived={showArchived} selected={selected} onSelect={setSelected} />
      <Legend />
      {node ? (
        // A new point starts with its forms closed.
        <Details
          key={node.id}
          documents={documents}
          view={view}
          node={node}
          onChanged={onChanged}
        />
      ) : (
        <p className="secondary">
          Choose a point to see what it is{page.canChange ? " and what you can do with it" : ""}.
        </p>
      )}
    </div>
  );
}

function Legend() {
  return (
    <ul className="legend small" aria-label="Key">
      <li>
        <span className="legend__swatch legend__swatch--master" aria-hidden="true" /> Master
        checkpoint
      </li>
      <li>
        <span className="legend__swatch legend__swatch--version" aria-hidden="true" /> Tailored
        version checkpoint
      </li>
      <li>
        <span className="legend__swatch legend__swatch--sent" aria-hidden="true" /> Copy sent
      </li>
      <li>
        <span className="legend__swatch legend__swatch--returned" aria-hidden="true" /> Came back
      </li>
    </ul>
  );
}

/** One connection of a point, as a sentence: "Sent to Mentor", "Made from master v1". */
export function linkText(edge: GraphEdge, here: string, name: (id: string) => string): string {
  const outgoing = edge.from === here;
  const other = name(outgoing ? edge.to : edge.from);
  switch (edge.kind) {
    case "sent":
      return outgoing ? other : `Sent from ${other}`;
    case "cameBack":
      return outgoing
        ? `Came back: ${other}`
        : `Came back from ${other.replace(/^Copy sent to /, "")}`;
    default:
      return outgoing ? `${edge.label ?? ""} → ${other}` : `${edge.label ?? ""}, from ${other}`;
  }
}

function Details({
  documents,
  view,
  node,
  onChanged,
}: {
  documents: Documents;
  view: DocumentView;
  node: GraphNode;
  onChanged: (message?: string) => void;
}) {
  const { graph, page } = view;
  const label = (id: string) => {
    const n = graph.nodes.find((x) => x.id === id);
    return n ? (describeNode(n, graph.lanes).split(",")[0] ?? "") : "";
  };
  const links = graph.edges.filter(
    (e) => (e.to === node.id || e.from === node.id) && e.kind !== "next",
  );
  const file = page.files.find((f) => f.entries.some((e) => e.id === node.id));
  const entry = file?.entries.find((e) => e.id === node.id);
  const sentRow = page.sent.groups.flatMap((g) => g.rows).find((r) => r.id === node.id);
  const [form, setForm] = useState<"restore" | "milestone" | undefined>();
  const previous = file?.entries[file.entries.findIndex((e) => e.id === node.id) + 1];
  const [compared, setCompared] = useState<WhatChanged | undefined>();
  const { busy, error, run } = useAction();

  return (
    <section className="panel stack" aria-label="Details" aria-live="polite">
      <h3>{describeNode(node, graph.lanes).split(",")[0]}</h3>
      <p>{node.detail}</p>
      <p className="secondary small">{node.when}</p>
      {node.milestone && (
        <p>
          <Chip kind="milestone">{node.milestone}</Chip>
        </p>
      )}
      {links.length > 0 && (
        <ul className="list small">
          {links.map((e) => (
            <li key={`${e.from}-${e.to}-${e.kind}`}>{linkText(e, node.id, label)}</li>
          ))}
        </ul>
      )}
      {page.canChange && file && entry && (
        <div className="row row--wrap">
          {previous && (
            <Button
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  setCompared(await documents.compare(page.familyId, previous.id, entry.id));
                })
              }
            >
              Compare with {previous.label}
            </Button>
          )}
          <Button
            onClick={() => {
              setForm(form === "restore" ? undefined : "restore");
            }}
          >
            Restore as a new version
          </Button>
          <Button
            onClick={() => {
              setForm(form === "milestone" ? undefined : "milestone");
            }}
          >
            {entry.milestone ? "Change milestone" : "Mark as milestone"}
          </Button>
        </div>
      )}
      {page.canChange && sentRow && (
        <div>
          <Button
            disabled={busy}
            onClick={() =>
              void run(async () => {
                saveFile(
                  await documents.sentCopy(page.familyId, sentRow.id),
                  sentFileName(sentRow.version, sentRow.recipient, page.format),
                );
              })
            }
          >
            Open as sent
          </Button>
        </div>
      )}
      {error && <Alert>{error}</Alert>}
      {compared && <Changes view={compared} />}
      {form === "restore" && file && entry && (
        <RestoreForm
          documents={documents}
          familyId={page.familyId}
          checkpoint={entry.id}
          suggested={`${stem(file.name)} (${entry.label})`}
          format={formatOf(file.name)}
          from={`${stem(file.name)} ${entry.label}`}
          onDone={onChanged}
        />
      )}
      {form === "milestone" && entry && (
        <MilestoneForm
          documents={documents}
          familyId={page.familyId}
          entry={entry}
          onDone={onChanged}
        />
      )}
    </section>
  );
}
