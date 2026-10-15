// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Save checkpoint, Make a tailored version and Send a copy (canvas "Personal
// users"). Every field has a visible label; errors are tied to their field.

import type { Family, SendArgs, WhatChanged } from "@lineage/shell";
import { useId, useState } from "react";
import type { SyntheticEvent } from "react";
import { WhatChangedView } from "./changes";
import { Back, Button } from "./parts";

export function SaveCheckpoint({
  changes,
  busy,
  onSave,
  onBack,
}: {
  changes: WhatChanged | null;
  busy: boolean;
  onSave: (note: string) => void;
  onBack: () => void;
}) {
  const [note, setNote] = useState("");
  const id = useId();
  const nothing =
    changes !== null && changes.changes.length === 0 && changes.formatting.length === 0;
  const submit = (event: SyntheticEvent<HTMLFormElement>) => {
    event.preventDefault();
    onSave(note);
  };
  return (
    <form className="stack" onSubmit={submit}>
      <Back onClick={onBack} />
      <h1>Save checkpoint</h1>
      {changes ? <WhatChangedView view={changes} /> : <p role="status">Looking at what changed…</p>}
      <div className="field">
        <label htmlFor={id}>Note (optional)</label>
        <input
          id={id}
          className="input"
          value={note}
          maxLength={200}
          placeholder="For example: Added the Halden job"
          onChange={(e) => {
            setNote(e.target.value);
          }}
        />
      </div>
      <Button type="submit" primary wide disabled={busy || changes === null || nothing}>
        Save checkpoint
      </Button>
    </form>
  );
}

function stem(name: string): string {
  return name.replace(/\.docx$/i, "");
}

export function MakeVersion({
  masterName,
  latest,
  busy,
  onMake,
  onBack,
}: {
  masterName: string;
  latest: string;
  busy: boolean;
  onMake: (name: string) => void;
  onBack: () => void;
}) {
  const [name, setName] = useState(`${stem(masterName)} – `);
  const [error, setError] = useState<string | null>(null);
  const id = useId();
  const submit = (event: SyntheticEvent<HTMLFormElement>) => {
    event.preventDefault();
    const trimmed = name.trim();
    if (trimmed === "" || trimmed === stem(masterName) || trimmed.endsWith("–")) {
      setError("Give the tailored version a name, such as the kind of role it's for.");
      return;
    }
    setError(null);
    onMake(trimmed);
  };
  return (
    <form className="stack" onSubmit={submit} noValidate>
      <Back onClick={onBack} />
      <h1>Make a tailored version</h1>
      <p>
        A copy of your master as it is at {latest}. It follows the master: when you change the
        master, you'll be offered the change.
      </p>
      <div className="field">
        <label htmlFor={id}>Name</label>
        <input
          id={id}
          className="input"
          value={name}
          maxLength={150}
          aria-invalid={error !== null}
          aria-describedby={error ? `${id}-error` : undefined}
          onChange={(e) => {
            setName(e.target.value);
          }}
        />
        {error && (
          <p id={`${id}-error`} className="small" style={{ color: "var(--destructive)" }}>
            {error}
          </p>
        )}
      </div>
      <Button type="submit" primary wide disabled={busy}>
        Make a tailored version
      </Button>
    </form>
  );
}

export function SendCopy({
  family,
  openNode,
  busy,
  onSend,
  onBack,
}: {
  family: Family;
  openNode: string;
  busy: boolean;
  onSend: (node: string, args: SendArgs) => void;
  onBack: () => void;
}) {
  // The open file, and tailored versions Lineage can read from storage.
  const choices = family.nodes.filter(
    (n) => !n.archived && (n.id === openNode || n.role === "variant"),
  );
  const [node, setNode] = useState(openNode);
  const [recipient, setRecipient] = useState("");
  const [purpose, setPurpose] = useState("");
  const [embedId, setEmbedId] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const ids = { who: useId(), purpose: useId(), id: useId(), version: useId() };
  const submit = (event: SyntheticEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (recipient.trim() === "") {
      setError("Say who the copy is for, so you can find it later.");
      return;
    }
    setError(null);
    onSend(node, { recipient: recipient.trim(), purpose: purpose.trim() || null, embedId });
  };
  return (
    <form className="stack" onSubmit={submit} noValidate>
      <Back onClick={onBack} />
      <h1>Send a copy</h1>
      <fieldset className="stack" style={{ border: "none", padding: 0, margin: 0, gap: 8 }}>
        <legend id={ids.version} style={{ fontWeight: 600, marginBottom: 4 }}>
          Which version
        </legend>
        {choices.map((n) => (
          <label key={n.id} className="card row" style={{ minHeight: "var(--target)" }}>
            <input
              type="radio"
              name="version"
              value={n.id}
              checked={node === n.id}
              onChange={() => {
                setNode(n.id);
              }}
            />
            <span>
              {n.displayName}
              {n.latest && <span className="secondary small"> · {n.latest.label}</span>}
            </span>
          </label>
        ))}
      </fieldset>
      <div className="field">
        <label htmlFor={ids.who}>Who it's for</label>
        <input
          id={ids.who}
          className="input"
          value={recipient}
          maxLength={200}
          placeholder="For example: Northgate Analytics"
          aria-invalid={error !== null}
          aria-describedby={error ? `${ids.who}-error` : undefined}
          onChange={(e) => {
            setRecipient(e.target.value);
          }}
        />
        {error && (
          <p id={`${ids.who}-error`} className="small" style={{ color: "var(--destructive)" }}>
            {error}
          </p>
        )}
      </div>
      <div className="field">
        <label htmlFor={ids.purpose}>Role or purpose (optional)</label>
        <input
          id={ids.purpose}
          className="input"
          value={purpose}
          maxLength={200}
          placeholder="For example: Senior data engineer"
          onChange={(e) => {
            setPurpose(e.target.value);
          }}
        />
      </div>
      <div className="field">
        <span style={{ fontWeight: 600 }}>Format</span>
        <p>Word (.docx)</p>
        <p className="secondary small">PDF is coming later.</p>
      </div>
      <label className="row" htmlFor={ids.id} style={{ minHeight: "var(--target)" }}>
        <input
          id={ids.id}
          type="checkbox"
          checked={embedId}
          onChange={(e) => {
            setEmbedId(e.target.checked);
          }}
        />
        <span>Add a Lineage ID, so you can recognise the file if it comes back</span>
      </label>
      <p className="callout small">
        <span aria-hidden="true">🔒 </span>Always a clean copy: no comments, tracked changes, hidden
        text or document properties.
      </p>
      <Button type="submit" primary wide disabled={busy}>
        Save copy and record it
      </Button>
      <p className="secondary small">Later you can open exactly what they received.</p>
    </form>
  );
}
