# Status

## Phase 1: Word engine and family logic (2026-10-15). Done.

Branch `phase-1-word-core`, from `phase-0-foundations`. Not pushed. Decisions: [0005 package and XML](decisions/0005-docx-package-and-xml.md), [0006 dependencies](decisions/0006-phase-1-dependencies.md), [0007 merging and clean copies](decisions/0007-merge-and-clean-copies.md).

**Done**

- **Package layer** (`crates/core/src/package`, `xml`):
  - rawzip reader with size, name, CRC and compression checks;
  - miniz_oxide inflate capped at the declared size;
  - our own writer, which copies unchanged entries as their original compressed bytes.
  - XML is parsed with roxmltree and changed only by byte-range splices, so untouched XML stays byte-identical.
  - The writer re-parses every part it changed and refuses to return malformed XML.
- **Canonical model** (`docx/`):
  - stories (body, headers and footers by section, footnotes, endnotes) → blocks (paragraphs, tables with rows and cells, section breaks, opaque) → text, atoms and markers, with byte spans;
  - styles and numbering referenced, not flattened;
  - accepted and rejected views of existing tracked changes, matching Word (checked: whitespace without `xml:space`, `w:cr`, text boxes, field results, paragraphs before tables).
- **What changed** (`diff/`):
  - patience LCS on content-and-formatting keys, with ties treated as one changed line so repeated lines pair with their counterparts;
  - formatting-only pairs, moves, similarity pairing (whole-percentage Dice), table rows and cells;
  - a word-level diff inside paragraphs;
  - the v0 change list, with a new optional `cell` location (schema, examples, Rust and TypeScript tests updated).
- **Three-way merge** (`merge/`): clean, already there, overlap (both edited, deleted vs edited, edited vs deleted, both added here, formatting vs text) or by hand, per paragraph, cell, row, section break or opaque block.
- **Tracked-change writer** (`write/`):
  - `w:ins`, `w:del`, `w:rPrChange` and `w:pPrChange`, row and table insertions and deletions, and new external hyperlinks, all by "Lineage";
  - moves shown as a deletion plus an insertion;
  - `Check` says in advance what can be written.
  - **No empty paragraph is ever left** at the end of the document or a cell, or before a table: the last paragraph mark changes hands Word's way (`write/ends.rs`), so accepting gives the master's version and rejecting gives the original, exactly.
- **Custom properties and clean copies** (`docx/props.rs`, `clean.rs`), **summaries** (`summary.rs`) and the **what-changed view model** (`view.rs`).
- **Family logic** (`crates/lineage`):
  - JSON Lines ledger with validated ids and hashes, and a `FamilyState` fold;
  - content-addressed snapshots;
  - change states and the merge base (decision 0002), with detection of text typed in by hand and of Lineage changes rejected in Word;
  - fingerprints, pure commands for every button, panel views.
- **Harness** (`crates/harness`): memory and local-folder storage, and `Panel` with one method per button. **The Phase 1 "done when" scenario passes end to end** in memory and on disk (`tests/personal_resume.rs`), plus 8 more scenarios.
- **Fixtures:** 38 synthetic .docx files (`crates/testkit`, moved out of xtask; the Phase 0 ten are byte-identical). All open in Word.
- **Tests:** 177 Rust tests pass, plus 18 TypeScript tests.
  - The 7 property tests pass at 10,000 cases each. They found three bugs, now fixed with regression tests: crossing pairs, repeated-line pairing, and the end of the document.
- **Word check** (`scripts/word-smoke.ps1 -Compare`):
  - All 57 engine outputs on fixtures (writer outputs and clean copies) open in Word.
  - The 54 that can be compared match Word's accept-all and reject-all text exactly. Files with DATE fields, maths or symbols are opened but not compared, because Word shows those in its own way.
  - Paragraph styles after accept and reject were checked for the end-of-document cases.
- **Your documents** (`tests/local_fidelity.rs`, local only, counts only), 45 files:
  - every file reads, writes back with every part and entry identical, opens in both views and cleans to the same text;
  - for all 44 neighbouring pairs, bringing every change in reproduces the other file and rejecting gives the original back;
  - all 89 outputs open in Word, and the 44 merges match Word's accept-all and reject-all text.
  - This run found two real bugs (self-closing empty paragraphs, paragraphs before tables), now fixed with regression tests. 60 changes between neighbours were left to bring in by hand (mostly section breaks and table layout).
- **Fuzzing:** `fuzz/` with `docx_reader` and `zip_reader`, and a CI job on a pinned nightly.
  - Not run locally: on Windows, the AddressSanitizer runtime in Visual Studio does not match nightly Rust's, and WSL has no Rust installed.
  - `the_reader_never_panics_on_damaged_files` covers damaged input on stable in every test run.

**Wasm size** (release, `opt-level = "s"`, LTO, before wasm-opt; measured with a temporary export that reaches the whole engine, then removed):

- 1,296,730 bytes raw, 357,725 bytes gzipped, plus about 6.8 KB of JS glue;
- compiles in about 1 ms in Node (lazy tiering).

The committed wasm crate still exports only the Phase 0 bridge (21,855 bytes). Task-pane budgets are set in Phase 2.

**Known issues**

- Brought in by hand for now (each shown with its reason; see 0007):
  - footnote and endnote changes;
  - section breaks;
  - table columns, merged cells and table formatting;
  - new pictures or other new parts;
  - content in tables of contents and block content controls;
  - paragraphs that already have unaccepted tracked changes.
- UTF-16 parts are read-only.
- Fuzzing has only run in CI, not locally.
- A sent copy drops the document title with the other metadata. Make it an option if people want titles kept.

## Phase 2 and Phase 3

Not started. The maintainer asked to continue into both.

## Phase 0: repository foundations (2026-10-15). Done locally; CI result below.

Branch `phase-0-foundations`. Decisions: [0004-stack](decisions/0004-stack.md).

**Done**

- **Cargo workspace:** `crates/core`, `crates/lineage`, `crates/wasm`, `crates/harness` and `xtask`.
  - Rust 1.99.0 pinned; clippy warnings are errors in CI.
  - `forbid(unsafe_code)` in core, lineage, harness and xtask.
- **pnpm workspace:** TypeScript 6.0.3 (strict), ESLint 10 with `strictTypeChecked`, Prettier and Vitest 5. `@lineage/shell` wraps the engine.
- **Bridge proven:** `engineVersion()` and `echoBytes()` go Rust → wasm → TypeScript.
  - Byte buffers up to 1 MB round-trip unchanged (`packages/shell/src/bridge.test.ts`).
  - `scripts/build-wasm.mjs` checks the wasm-bindgen CLI matches `Cargo.lock`.
- **`spec/format/v0`:** change-list JSON Schema (draft 2020-12), with 4 valid and 6 invalid examples. Tested in Rust (boon) and TypeScript (ajv).
- **Fixtures:** `cargo xtask fixtures` generates 10 deterministic .docx files with invented content into `fixtures/docx/synthetic/`.
  - All ten open in Microsoft Word with the expected comments, footnotes, revisions, sections, image, fields and table (`scripts/word-smoke.ps1`).
  - `cargo xtask check-fixtures` catches drift, and `cargo xtask check-headers` catches missing licence headers. Both were confirmed to fail on bad input.
- **Community docs:** README, CONTRIBUTING (DCO, Conventional Commits, fixtures), CODE_OF_CONDUCT (Contributor Covenant 2.1), SECURITY (GitHub private reporting), GOVERNANCE, NOTICE. Placeholder READMEs for later packages and apps.
- **CI** (`.github/workflows/ci.yml`):
  - Rust on Linux and Windows: fmt, clippy, tests, header and fixture checks.
  - cargo-deny.
  - wasm build and TypeScript lint, typecheck, tests and format.
  - DCO check on pull requests.
- **Local results:** cargo fmt, clippy (`-D warnings`), `cargo test --workspace` (20 tests), `cargo deny check`, both xtask checks, and `pnpm lint`, `typecheck`, `test` (16 tests) and `format:check` all pass.
- **Clean clone** of the pushed branch (Windows): `cargo test --workspace --locked`, both xtask checks, and `pnpm install --frozen-lockfile`, `lint`, `typecheck`, `test` and `format:check` all pass.

**Wasm baseline** (release, `opt-level = "s"`, LTO, before wasm-opt):

- size: 21,378 bytes raw, 8,545 bytes gzipped, plus 6,070 bytes of JS glue,
- Node `initSync`: about 2 ms warm.

Task-pane budgets are set in Phase 2.

**CI:** pending the first run on the pull request.

**Not done / known issues**

- TypeScript 7 is out, but typescript-eslint supports only TypeScript below 6.1, so we're on 6.0.3. Revisit at each phase start.
- LibreOffice is not installed, so the headless smoke test is replaced locally by `scripts/word-smoke.ps1` (Windows with Word). CI has neither.
- GitHub private vulnerability reporting must be turned on in the repository settings (Security → Private vulnerability reporting) for SECURITY.md to work.
- The fixture set is 10 files; Phase 1 grows it to 30 or more.
- **Windows long paths.** pnpm 12's recursive runner writes a state file under `node_modules/.pnpm-task-run-state-v1/` with a long hashed name. In a deeply nested clone (folder path over about 120 characters), `pnpm test` fails with "The system cannot find the path specified". Fix: clone to a shorter path, or turn on Windows long-path support. The README says so.

## Session 0: orientation (2026-10-14 to 15). Done.

- Read CLAUDE.md, BLUEPRINT.md, PROMPTS.md and every board of the blueprint canvas.
- Repo connected to https://github.com/Shiki992/lineage.
- Installed Rust 1.99.0 (with clippy, rustfmt, wasm32 target) and pnpm 12.11.2 through npm (corepack 0.33 can't run pnpm 12's native binary); `%APPDATA%\npm` added to the user PATH. Visual Studio 2026 C++ tools and Node 24.3.0 were already present.
- Copyright holder and DCO sign-off: Hrishikesh Vedantam. Repo-local git `user.name` set.
- Decisions: 0001 no command line for users, 0002 change states, 0003 accepted changes land as tracked changes, then a checkpoint. PROMPTS.md moved into docs/.
- `test/` holds the maintainer's own Word documents (42 resumes, 3 business documents). They contain real personal data, so `/test/` is git-ignored. Use them for local fidelity runs only.
  - By structure only: 4 resumes saved by Word (including the master), 38 written by a script; the business documents mix both. No tracked changes, fields or footnotes.

## Open questions (carried forward)

- BLUEPRINT open decision 5 (start from files made by hand): which phase.
- Whether a change left pending (never clicked) should also go quiet after the master moves on by a set number of checkpoints, like the undetected case in decision 0002.
- Canvas mismatches still to fix on the canvas or in BLUEPRINT:
  - solid dark boxes used for both "offered change" and milestone,
  - Send a copy defaults to PDF and has no "leave out the ID" switch,
  - "slide notes" as a review channel,
  - Drive properties vs the ledger,
  - "Restore as the master" and "Share family",
  - EMEA "caught up at v2" vs "took v5".
- Re-verify Docs API suggest-mode writes before Phase 5.

## Next

Phase 2 (Word add-in), then Phase 3 (returned files).
