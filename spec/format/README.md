# Lineage open change format

A JSON format for "what changed" between two states of one document. The Lineage engine produces it, and any other tool may read or write it. It is public API.

## Versions

| Version | Schema                                                                                 | Status                   |
| ------- | -------------------------------------------------------------------------------------- | ------------------------ |
| v0      | [`v0/change-list.schema.json`](v0/change-list.schema.json) (JSON Schema draft 2020-12) | Draft. Word (.docx) and PowerPoint (.pptx) |

## v0 in brief

A change list is `{ specVersion: "0", format: "docx" | "pptx", changes: [...] }`. Each change has:

- `id`: stable within the list.
- `kind`: one of `insert`, `delete`, `modify`, `move`, `format`.
- `location`: depends on the format. For docx: `part`, `blockIndex`, plus optional `blockAnchor`, `charRange` and `cell`. `cell` points inside the table at `blockIndex`: a `row`, then optionally a `column` and the `blockIndex` inside that cell (a nested table nests another `cell`). A `cell` with only a `row` means the whole row. For pptx: `part` (the slide), `slideIndex` (its position in show order), plus optional `slideId` (which survives reordering), `shapeId` (for a change to one shape) or `target` (`slide`, `notes`, `settings`, `animations` or `design` for changes that are not to one shape).
- `fromLocation`: only on `move`, and required there. A moved block that was also edited has different `before` and `after` text.
- `before` and `after`: text, with `null` for the side that doesn't exist (`before` on an insert, `after` on a delete).
- `formattingOnly`: `true` when only formatting changed. It is always `true` for `format`.
- `confidence`: a **whole** percentage from 0 to 100. Integers on purpose: the format never carries floats.
- `summary`: optional plain-English text for people.

Unknown fields are rejected everywhere (`additionalProperties: false`).

## Examples and tests

- `v0/examples/valid/`: must pass.
- `v0/examples/invalid/`: must fail, each for exactly one reason, which its file name says.

Both are checked in Rust (`crates/core/tests/spec_examples.rs`, using `boon`) and in TypeScript (`packages/shell/src/spec.test.ts`, using `ajv`), so the two languages agree.

## Versioning rules

- While v0 is a draft, it may change in any way. Every change updates both test suites and the examples.
- From v1 on:
  - **Within a major version:** adding optional fields or new `kind` / `format` values is allowed. Readers must ignore `kind` values they don't know, and say so to the user rather than guess.
  - **Breaking changes** (removing or renaming fields, changing meaning or types) need a new major version in a new folder. The old folder stays.
- `specVersion` always names the major version as a string.
