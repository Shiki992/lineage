# Build prompts for Claude Code

Paste one prompt per session, in order. Each phase ends with Claude Code updating `docs/STATUS.md` and stopping, so you review before the next one starts.

**Model per phase**
- Engine phases (1, 3, 4, 6): `/model opus` with `/effort high`. Switch to `/model fable` for the merge design, or whenever Opus gets stuck on a fidelity bug.
- UI and integration phases (2, 5): `/model sonnet` is enough. Use Opus for the plan and the final review.

---

## Session 0: orientation (no code)

```
Read CLAUDE.md, docs/BLUEPRINT.md and this file. I've also attached the Lineage
blueprint design canvas (an artifact). Read it for the screens and diagrams on the
"Personal users" page first, then the other two pages for context.

Do not write code yet. Reply with:
1. Ten bullets summarising what Lineage is and what we build first.
2. Anything in the canvas that contradicts BLUEPRINT.md.
3. Your questions, at most eight, ordered by how much they block Phase 0 and Phase 1.
```

---

## Phase 0: repo foundations

```
Phase 0: repository foundations. Start in plan mode and wait for my approval.

Build:
- A Cargo workspace (crates/core, lineage, wasm, harness) and a pnpm workspace
  (packages/, apps/) matching the layout in CLAUDE.md, each with a README and
  build/test scripts. Pinned Rust toolchain, rustfmt, clippy with warnings as errors,
  strict TypeScript, Vitest, ESLint and Prettier.
- A hello-world path proving the bridge: a Rust function compiled to wasm, called
  from a TypeScript test.
- LICENSE (Apache-2.0), CONTRIBUTING.md (DCO sign-off, how to add a fixture),
  CODE_OF_CONDUCT.md, SECURITY.md (how to report a vulnerability), GOVERNANCE.md (short).
- GitHub Actions CI on every PR: cargo fmt, clippy, test, cargo-deny; pnpm lint,
  typecheck, test; wasm build.
- spec/format: v0 JSON Schema for a change list (change id, kind, location,
  before, after, formatting-only flag, confidence) plus a few valid and invalid
  examples with schema tests in both Rust and TypeScript.
- fixtures/docx/README.md describing how fixtures are added and licensed, plus a script
  that generates synthetic .docx fixtures (invented content only).
- Update docs/STATUS.md, and add docs/decisions/NNNN-stack.md (next free number).

Out of scope: any engine logic.
Done when: CI is green on a clean clone, and `cargo test` and `pnpm test` both run.
```

---

## Phase 1: Word engine and family logic (personal core)

```
Phase 1: the personal core for Word, proved by end-to-end scenario tests on local
files. There is no user-facing command line (decision 0001). Start in plan
mode. Show me the canonical model, the diff and merge approach, and the test plan
before writing code.

Build in crates/core (Rust, pure, no I/O):
- Read and write .docx packages, leaving untouched parts and unknown XML exactly
  as they were.
- A canonical model: blocks (paragraphs, tables, section breaks) → runs, with styles
  and numbering referenced, not flattened.
- A diff that produces the format-spec change list and survives moved paragraphs.
- A three-way merge (base, ours, theirs) that auto-applies only non-overlapping
  changes and returns everything else as conflicts.
- A writer that applies chosen changes as real tracked changes (w:ins / w:del).
- Plain-English summaries ("Summary: 'Data engineer' changed to 'Senior data engineer'").

Build in crates/lineage (Rust, pure):
- The event ledger, checkpoints, variants, offers and sent copies from BLUEPRINT §4.
- Change states (merged / declined / pending, with content detection) and the merge
  base rule from BLUEPRINT §4 and decision 0002.
- Identity via custom document properties, plus a fingerprint fallback.
- Content-addressed blobs (the crate computes hashes; the host stores them).
- A "what changed" view model the panel can draw directly: removed and added text
  per paragraph, and both sides for every overlap, with plain-English summaries.

Build in crates/harness (Rust, test-only, local-folder and in-memory storage):
- One function per button the personal screens will have: start tracking, save
  checkpoint, make a tailored version, show what changed, offer to versions, bring in
  or decline each change, send a copy, what I've sent, open what they received.
- Scenario tests that click through those functions the way a person would.

Fixtures and tests:
- At least 30 synthetic .docx fixtures covering headings, lists, tables, headers and
  footers, footnotes, comments, existing tracked changes, images, fields and sections.
- Unchanged round trip: every part is identical (byte or canonical XML).
- Merge cases: different paragraphs merge cleanly; the same paragraph conflicts;
  deleted vs edited conflicts; formatting-only changes are reported separately.
- A sent copy has no comments, tracked changes or extra metadata beyond the two IDs.
- If LibreOffice is installed, every output opens headless without errors.
- proptest invariants: merge(base, x, base) == x; diff then apply reproduces the target.
- A cargo-fuzz target for the .docx reader, run for a short time in CI.
- Change states: a declined change is not offered again; a change copied in by hand
  is detected as merged; a version can take v3 without v2.
- Local-only fidelity run over the maintainer's documents in test/ (git-ignored):
  unchanged round trip for every file. Report counts only, never content.

Out of scope: wasm bindings beyond Phase 0, any add-in, cloud storage, PDF export,
PowerPoint or Excel.
Done when all tests pass and this scenario passes end to end on a fixture: start
tracking a master resume, make two tailored versions, add a job to the master, offer
it, bring it into one version and decline it in the other, send a copy, then open
the sent copy.
```

---

## Phase 2: Word add-in for personal users

```
Phase 2: the Word add-in for personal users, running the Phase 1 engine as wasm in
the task pane. Start in plan mode. First check the current Office.js, add-in
authentication, Microsoft Graph and wasm-in-add-in documentation, and list what you
verified with links.

Build crates/wasm and packages/shell:
- Bindings for every operation the personal screens need, with typed TypeScript
  wrappers and schema validation on both sides of the boundary.
- Record wasm bundle size and start-up time in the task pane, and set budgets with me.

Build apps/office-addin (React task pane, 360px wide, design system from CLAUDE.md):
- The three Word screens from the canvas "Personal users" page: master resume panel,
  Send a copy, and What I've sent. Match their content and states.
- Sign-in with a personal Microsoft account, and packages/storage-onedrive behind the
  same storage interface as the harness's local-folder store.
- Prototype both layouts in BLUEPRINT open decision 1 (variants next to the master vs.
  in the app folder). Recommend one in docs/decisions with the permissions each needs.
- Getting the current document's bytes, writing a variant, and applying offers as
  tracked changes in the open document.

Out of scope: PowerPoint, Excel, Google, enterprise features, publishing to the store.
Done when: sideloaded in Word on the web and desktop, I can start tracking my resume,
make two tailored versions, offer a master change, send a copy and reopen it. Write a
short manual test script in docs/testing/word-personal.md.
```

---

## Phase 3: feedback round trip

```
Phase 3: files that come back. Start in plan mode.

- Recognise a returned .docx by its IDs, then by fingerprint with a confidence score.
  Show uncertain matches as a choice, never a silent guess.
- Compute only the outside person's changes against the checkpoint it left from, even
  if the master has moved on since.
- Review one change at a time in the panel (keep / skip), applied as tracked changes.
- Add the "Came back with N changes" state to What I've sent.
- Fixtures: returned files with the IDs stripped, renamed files, files re-saved by
  LibreOffice and Google Docs export, and files that were heavily reformatted.

Done when: the feedback path in the journey tree works end to end, and the fingerprint
tests report precision and recall on the fixture set.
```

---

## Phase 4: PowerPoint deck variants

```
Phase 4: PowerPoint for founders. Start in plan mode with the slide model and
matching approach.

- crates/core: a .pptx model (slides → shapes → text) with matching by ID, then
  layout, then position and text similarity. Diff, three-way merge and write.
- Offer a changed slide to chosen tailored versions, with a per-version conflict report.
- The pitch deck panel from the canvas, including "Open what they saw" for a sent copy.
- Fixtures: decks with reordered slides, duplicated slides, edited charts (treated as a
  whole object in v0), notes and hidden slides.

Done when: the founder journey works end to end in PowerPoint on the web and desktop.
```

---

## Phase 5: Google Docs and Slides for personal users

```
Phase 5: Google Docs and Slides for personal users. Start in plan mode, and first verify
the current add-on, OAuth and Drive scope documentation, with links.

- packages/storage-gdrive using only the drive.file scope.
- Map Docs API JSON into the canonical sequence model, in Rust (crates/core), with the
  TypeScript shell only fetching and sending JSON. Write offers with
  documents.batchUpdate in suggest mode where supported, and fall back to the sidebar
  review where not. Document which request types fall back.
- Slides: map into the slide model, with the review in the sidebar.
- A 300px sidebar with the same personal screens.
- Cross-format: a .docx downloaded from a Google Doc and sent out must be recognised
  when it returns.

Done when: the job seeker and feedback journeys work in Google Docs, and the founder
journey works in Google Slides.
```

---

## Phase 6: Excel engine

```
Phase 6: the Excel engine (needed for enterprise). Start in plan mode with the grid
model, row and column alignment and formula handling.

- A .xlsx model in crates/core with formulas as the source of truth and cached values
  ignored. Use exact decimal or integer types for cell values; no float round-trips.
- Align inserted and deleted rows and columns before comparing cells. Track named
  ranges. Leave VBA, pivots and external links untouched and report them as unsupported.
- Three-way merge at cell level, and a review experience in the task pane.
- Fixtures include large sheets (50k rows) with a performance budget recorded in STATUS.

Done when: budget-style variants work end to end and the performance budget is met.
```

---

## Phase 7: enterprise on Microsoft 365

```
Phase 7: enterprise Microsoft. Re-read BLUEPRINT §3 and §6 (Microsoft 365 page of the
canvas). Start in plan mode.

- Working copies, catch up, suggest changes, per-change review and approval, protected
  master, archive on accept.
- apps/family-app: the review page and the family tree view from the canvas.
- Per-company Entra app registration with a setup guide for IT, central deployment
  instructions, and an admin policy file.
- Notifications through Outlook or Teams from the user's own account.

Done when: the six enterprise journeys pass a scripted end-to-end test in a test tenant.
```

---

## Phase 8: enterprise on Google Workspace + Slack

```
Phase 8: Google Workspace + Slack. Re-read the Google Workspace + Slack page of the
canvas. Start in plan mode, and verify the Slack, Drive locking and internal-app
documentation first.

- Drive locks for protected masters, and the internal-app setup guide.
- apps/helper: a small self-hostable service (Cloud Run first) for Slack interactivity,
  Drive change alerts and automatic updates. No data leaves the company's project.
- The Slack app: review request messages, Approve only when nothing overlaps, home tab,
  and returned-file detection in channels Lineage was added to.

Done when: the Google journeys pass end to end in a test Workspace and Slack workspace.
```
