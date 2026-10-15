# Lineage

Lineage is open-source version control for Word, PowerPoint and Excel files, and later Google Docs, Slides and Sheets, for people who never want to see git. Every working copy or tailored version is a real file. Lineage keeps the family tree that links them, offers changes from a master to its versions, and records every copy that leaves.

Read `docs/BLUEPRINT.md` for the full product design and `docs/PROMPTS.md` for the build phases. Check `docs/STATUS.md` before starting work.

## Audiences, in build order

1. **Personal users (now).** Job seekers with one master resume and tailored versions. Founders with one pitch deck and versions for each investor or customer.
2. **Enterprise on Microsoft 365.**
3. **Enterprise on Google Workspace + Slack.**

Build only what the current phase needs. Do not build enterprise features (approvals, protected masters, admin settings, Slack) during personal phases, but keep the data model able to hold them later.

## Non-negotiables

- **Never corrupt a user's file.** Every write path has a round-trip test.
- **Never merge silently when unsure.** If both sides changed the same thing, or the engine cannot map a change with confidence, show both and let the person choose.
- **Copies that leave are clean.** No history, comments, tracked changes or hidden data inside. Only a minimal Lineage ID (family ID and checkpoint ID) so the file can be recognised if it comes back, and the user can turn even that off.
- **No servers run by the project.** History lives in the user's own storage: OneDrive or Google Drive. Tests use a local folder.
- **No command line for users.** Lineage is for non-technical people: every action is a button in the panel. What changed is shown in the panel, with removed and added text marked, and the person decides.
- **Least privilege.** Ask for the narrowest storage permission that works. On Google use `drive.file`. On OneDrive prefer the app-folder permission where it is enough.
- **Plain language in every screen.** Never show git words to users. Use the glossary below.
- **Accessible by default.** Real buttons and labels, 44px touch targets, 4.5:1 text contrast.

## Glossary: code name → what users see

| In code | In the UI |
|---|---|
| family | document family |
| master | master (personal: "master resume", "master deck") |
| checkpoint | checkpoint |
| variant | tailored version (personal), variant (enterprise) |
| working copy | working copy (enterprise only) |
| suggestion | suggest changes (enterprise only) |
| apply / merge | bring in, accept |
| catch up | catch up. Implemented as a three-way merge, never a history rewrite |
| copy change | copy this change to other versions |
| milestone | milestone |
| sent copy | sent copy, "what I've sent" |
| returned file | "this file came back" |
| diff | what changed |

## Architecture

Functional core, thin shell. Everything computational is Rust: document bytes, diff, merge, matching returned files, summaries and family logic. TypeScript only draws UI and calls platform APIs.

```
spec/format/               Open change format (JSON Schema). Public API, versioned.
crates/core                Rust. Read package, canonical models, diff, three-way merge,
                           conflicts, writers, plain-English summaries. No I/O.
crates/lineage             Rust. Family logic: ledger events, checkpoints, variants, offers,
                           sent copies, identity (custom document properties + fingerprint).
                           Pure: takes events and blobs in, returns new events and blobs. No I/O.
crates/wasm                wasm-bindgen bindings exposing core + lineage to JavaScript.
crates/harness             Rust, test-only. Local-folder and in-memory storage that drives
                           end-to-end scenarios. Never shipped; no user-facing command line.
packages/shell             TypeScript. Typed wrapper around the wasm module + storage interface.
packages/storage-onedrive  TypeScript. Microsoft Graph calls only.
packages/storage-gdrive    TypeScript. Drive API calls only, drive.file scope.
apps/office-addin          TypeScript/React task pane: Word, then PowerPoint, then Excel.
apps/google-addon          Google Docs and Slides add-on (later phase).
apps/family-app            Web app for the family tree and reviews (enterprise phase).
```

Dependencies point inward only: apps → packages → crates/wasm → lineage → core → spec. `core` and `lineage` never do I/O. **TypeScript never parses or edits document XML.** It passes bytes to the wasm module and gets bytes back.

## Stack

Rust engine compiled to WebAssembly, with thin TypeScript shells.

**Rust (crates/)**
- Stable toolchain pinned in `rust-toolchain.toml`; one Cargo workspace.
- `#![forbid(unsafe_code)]` in `core` and `lineage`.
- Choose zip and XML crates that leave untouched XML exactly as it was, including unknown elements, attributes and namespaces. Record the choice and alternatives in `docs/decisions/`.
- Integers and decimals are explicit types. Never route cell values or measurements through floats unless the source format stores floats.
- Supply chain: `cargo-deny` (licences, advisories, duplicate crates) in CI. Keep the dependency tree small, and justify every new crate in a decision note.
- Tests: unit tests, `proptest` for diff and merge invariants, and `cargo-fuzz` targets for every parser. Returned files are untrusted input.
- WebAssembly: `wasm-bindgen` (verify the current recommended toolchain before setting it up). Track bundle size and start-up time in `docs/STATUS.md`.

**TypeScript (packages/, apps/)**
- pnpm workspaces, strict TypeScript, Vitest, committed lockfile with exact versions.
- React for task panes and the family app. Any other dependency needs a decision note.
- Validate everything crossing the wasm boundary or arriving from a platform API against a schema.

## How to work in this repo

- **Plan first.** Start every phase in plan mode. Propose the plan, files and tests, then wait for approval before writing code.
- **One phase per session.** At the end, update `docs/STATUS.md` with what is done, what is not and known issues, then stop. Do not start the next phase unprompted.
- **Record decisions** in `docs/decisions/NNNN-short-title.md` (context, decision, alternatives, consequences).
- **Check current docs before using any platform API** (Office.js, Microsoft Graph, Google Docs/Drive/Slides, Slack). These change often. Link the doc page in a comment wherever behaviour is subtle.
- **Tests first for `crates/core`.** Every bug gets a fixture and a test. Fixtures live in `fixtures/<format>/` with a README giving each file's origin and licence. Only commit files we may redistribute. Example resumes and decks must be invented, never real people's data. The maintainer's own documents live in `test/` (git-ignored) for local fidelity runs only; never copy them into `fixtures/` or quote their content in commits, issues or logs.
- **Fidelity checks.** Open → write unchanged → compare parts. Edit → write → reopen → compare. If LibreOffice is installed, open every output headless as a smoke test.
- **Commits.** Small, Conventional Commits, with a DCO sign-off line. Apache-2.0 header on source files.
- **Never** commit secrets, tokens, client IDs tied to a person, or real personal documents.

## Design system (from the Lineage blueprint canvas)

- Type: IBM Plex Sans for UI, IBM Plex Mono for cell references and IDs.
- Ink `#18212B`, secondary text `#4A5563`, page `#F4F5F7`, surface `#FFFFFF`, border `#D5D9E0`, strong border `#AEB5C0`.
- Accent (primary actions, working copies): `#0E7C7B`; text on tint `#0B4F4E`; tint `#E3F2F1`.
- Variants: `#4F46B5`; text `#2E2880`; tint `#ECEBFA`.
- Master: `#5B6573`; tint `#E9ECF1`. Milestone: solid `#18212B` with white text.
- Overlap or warning: tint `#FBF3E4`, border `#E7CF9F`, text `#6B3D06`. Destructive text: `#9A3412`.
- Radius 8 for controls, 10 for buttons and cards, 12 for panels. Task pane width 360 (Office), 300 (Google).
