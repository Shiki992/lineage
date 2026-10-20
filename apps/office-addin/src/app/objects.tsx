// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Macros, queries and pivot tables in a workbook's "What changed" (Phase
// 5B, decisions 0027 to 0030): the code view, a pivot table's fields before
// and after, and "Copy code" for what comes in by hand. Lineage never runs
// any of this code.

import type { CodeLine, FieldLists, PivotFields } from "@lineage/shell";
import { useId, useState } from "react";
import { Button } from "./parts";

/** Said when a workbook came back with new macro code (decision 0027). */
export const NEW_MACRO_CODE =
  "This file came back with new macro code. Lineage never runs it. Check each macro before you bring it in.";

/** A macro's or query's changed lines, with a few unchanged lines around them. */
export function CodeView({ lines }: { lines: readonly CodeLine[] }) {
  const label = useId();
  return (
    <div className="stack" style={{ gap: 4 }}>
      <p id={label} className="small secondary">
        <span className="code-key code-key--removed">− Removed</span>{" "}
        <span className="code-key code-key--added">+ Added</span>
      </p>
      <ol className="code" aria-labelledby={label}>
        {lines.map((line, i) => {
          if (line.kind === "skipped") {
            return (
              <li key={i} className="code__skipped small secondary">
                {line.text}
              </li>
            );
          }
          const number = line.after ?? line.before;
          const mark = line.kind === "removed" ? "−" : line.kind === "added" ? "+" : " ";
          const word =
            line.kind === "removed" ? "Removed" : line.kind === "added" ? "Added" : "Unchanged";
          return (
            <li key={i} className={`code__line code__line--${line.kind}`}>
              <span className="code__num" aria-hidden="true">
                {number === null ? "" : String(number)}
              </span>
              <span className="code__mark" aria-hidden="true">
                {mark}
              </span>
              <span className="visually-hidden">
                {word}
                {number === null ? "" : ` line ${String(number)}`}:{" "}
              </span>
              <code className="code__text">{line.text}</code>
            </li>
          );
        })}
      </ol>
    </div>
  );
}

const FIELD_ROWS: readonly [keyof PivotFields, string][] = [
  ["rows", "Rows"],
  ["columns", "Columns"],
  ["values", "Values"],
  ["filters", "Filters"],
];

function listed(fields: PivotFields | null, key: keyof PivotFields): string {
  if (!fields) {
    return "—";
  }
  return fields[key].length === 0 ? "none" : fields[key].join(", ");
}

/** A pivot table's fields before and after, as the PivotTable Fields pane lists them. */
export function FieldsView({ fields }: { fields: FieldLists }) {
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

/**
 * "Copy code": the whole module or query, to paste in by hand. For code
 * from a file that came back, the person confirms first, every time.
 */
export function CopyCode({
  code,
  where,
  fromOutside,
}: {
  code: string;
  /** "the VBA editor (Alt+F11)" or "the query's Advanced Editor". */
  where: string;
  fromOutside: boolean;
}) {
  const [state, setState] = useState<"idle" | "confirm" | "copied" | "manual">("idle");
  const copy = () => {
    navigator.clipboard.writeText(code).then(
      () => {
        setState("copied");
      },
      () => {
        setState("manual");
      },
    );
  };
  return (
    <div className="stack" style={{ gap: 6 }}>
      {state === "idle" && (
        <Button
          onClick={() => {
            if (fromOutside) {
              setState("confirm");
            } else {
              copy();
            }
          }}
        >
          Copy code
        </Button>
      )}
      {state === "confirm" && (
        <div
          className="warning stack"
          style={{ gap: 8 }}
          role="group"
          aria-label="Copy their code?"
        >
          <p className="small">
            This code came from outside. Lineage never runs it, but Excel will once it is in your
            workbook and macros are on. Copy it only if you trust it.
          </p>
          <div className="row" style={{ flexWrap: "wrap" }}>
            <Button onClick={copy}>Copy their code</Button>
            <Button
              onClick={() => {
                setState("idle");
              }}
            >
              Cancel
            </Button>
          </div>
        </div>
      )}
      {state === "copied" && (
        <p className="small" role="status">
          Copied. Paste it in {where}.
        </p>
      )}
      {state === "manual" && (
        <div className="stack" style={{ gap: 4 }}>
          <p className="small" role="status">
            Lineage couldn't copy it. Select the code below and copy it, then paste it in {where}.
          </p>
          <textarea
            className="input mono small"
            readOnly
            rows={8}
            value={code}
            aria-label="The code"
            onFocus={(e) => {
              e.currentTarget.select();
            }}
          />
        </div>
      )}
    </div>
  );
}
