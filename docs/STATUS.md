# Status

## Phase 2: Word add-in for personal users (2026-10-15). Built and checked in Word for Windows; the OneDrive and Word-on-the-web run is the maintainer's.

Branch `phase-2-word-addin`, from `phase-1-word-core` at `125f18d`. Not pushed. Decisions:

- [0008 Word add-in platform](decisions/0008-word-addin-platform.md)
- [0009 OneDrive layout](decisions/0009-onedrive-layout.md)
- [0010 wasm boundary and budgets](decisions/0010-wasm-boundary-and-budgets.md)
- [0011 dependencies](decisions/0011-phase-2-dependencies.md)

Manual test script: [docs/testing/word-personal.md](testing/word-personal.md).

**Done**

- **Engine boundary** (`crates/wasm`):
  - `Engine` exposes every operation the personal screens need, each a plain Rust function in `api.rs` with native tests;
  - inputs refuse unknown fields and check ids, hashes and times;
  - a fetched-blob cache reports missing blobs a snapshot at a time;
  - golden replies (`packages/shell/test/golden/`) keep Rust and TypeScript in step.
- **Engine additions:**
  - `check::written` (did a write land in Word as written);
  - `SeededIds`;
  - `decide_cards` (the harness uses it too);
  - `blob::referenced` and `manifest_parts`.
- **`packages/shell`:**
  - valibot schemas for every value crossing the boundary;
  - the typed `Engine`;
  - `Storage` mirroring the harness trait, with `MemoryStorage` and a conformance suite;
  - `DocumentHost`;
  - `Lineage`, the TypeScript twin of `harness::Panel`. It writes the document before appending to the ledger, reads every write back and puts the original back if it didn't land, and on a conflicting append undoes and runs again.
- **`packages/storage-onedrive`:**
  - Graph with token refresh and `Retry-After`;
  - ledger appends through upload sessions with `If-Match`;
  - downloads through `downloadUrl`;
  - both layouts of BLUEPRINT open decision 1, tested against a fake Graph.
- **`apps/office-addin`:** an add-in-only manifest with a Home-tab button, and every screen from the canvas "Personal users" page:
  - master panel, tailored version panel;
  - updates with − and + marks, bring in or decline, overlaps with both sides, by-hand reasons;
  - save checkpoint, make a tailored version;
  - send a copy (Word only, "PDF is coming later", the Lineage ID switch, the clean-copy note);
  - what I've sent.

  Also: sign-in through nested app authentication, the Word host through Office.js, and dev-only local-folder storage, timings and a diagnostics screen.

- **Development scripts:** `dev-cert.ps1` (run by the maintainer) and `sideload-word.ps1` replace Microsoft's two CLI tools; `make-icons.mjs` draws the icons; `check-budgets.mjs` runs in CI.

**Checked in Word** (Word 16.0.20430, Current Channel, Windows 11; history in the dev folder):

- **Write spike** (decision 0008): all 57 of the engine's Word smoke-test outputs went into an open document through `insertFileFromBase64` and were read back. 57 of 57 landed exactly, with every Lineage revision still by "Lineage". The first run found the check too strict about empty note parts (fixed).
- **The Phase 2 scenario, clicked through in the pane** on the synthetic resume:
  1. Start tracking; make "Resume – data roles" and "Resume – platform roles".
  2. Add a job to the master in Word; what changed shows both paragraphs marked +; save v2; offer to both versions.
  3. Open data roles from the panel and bring both changes in. Word's own tracked-changes API lists one insertion by **Lineage**, and Track Changes is back off.
  4. Open platform roles and decline both; nothing is written.
  5. Send a copy to Northgate Analytics (with the id) and one to a mentor (without it).
  6. Open what Northgate received: a new, unsaved document with no revisions or comments.
- **Ids:** written through Office.js custom properties, they are read by the engine from Word's bytes.
- **Audit with the Rust engine alone** (`crates/harness/tests/dev_folder.rs`): 2 families, 7 checkpoints restore, every blob matches, both versions on disk are checkpoints of their nodes, and both sent copies match the record.
- **`scripts/word-smoke.ps1 -Compare` on both versions:** accept all and reject all match the engine.

**Budgets** (decision 0010; the first three fail CI):

|                                | Budget                   | Measured                                                                                                                                      |
| ------------------------------ | ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| wasm, raw                      | 1.5 MB                   | 907,598 bytes                                                                                                                                 |
| wasm, gzipped                  | 420 KB                   | 324,081 bytes                                                                                                                                 |
| JavaScript, gzipped            | 250 KB                   | 154,272 bytes                                                                                                                                 |
| Engine ready in Word           | 500 ms cold, 200 ms warm | 15 loads: 30 to 90 ms warm, up to 251 ms on the first load after Word starts                                                                  |
| Panel ready, local storage     | 800 ms                   | median 496 ms; 2 of 15 over (816 and 867 ms), both in Word's first seconds after starting                                                     |
| One engine command on a resume | 300 ms                   | 1 to 52 ms on the synthetic resume. On the maintainer's 44 documents under 100 KB: start tracking at most 249 ms, what changed at most 144 ms |

- **Word's own work comes on top:** reading the document takes about 0.2 to 0.6 s, and bringing changes in about 1.1 s (write, read back, check, save).
- **`wasm-opt` was measured and dropped:** 9.4% smaller raw but 7% larger gzipped.

**Bugs found and fixed** (each with a regression test):

- **Start tracking failed on 40 of the maintainer's 45 documents.** Their custom properties part is an empty `<Properties/>`, and adding two properties expanded the tag twice. The local fidelity run now writes and rereads the ids on every document: 45 of 45.
- **Any file that had been through Word looked changed.** Word re-saves files in its own way, so sending a copy straight after bringing changes in made an empty checkpoint. "Changed" now means the engine finds a change in a story, in either view.
- **The write check was too strict.** Word keeps footnote and endnote parts with only separators once a document has had notes, and the check counted those empty stories.
- **A new section break was summarised as "Added a blank line".**
- **By-hand cards had no buttons**, so such a change could never stop being offered. They now offer Decline.

**Tests:**

- **Rust:** 200 pass, 3 ignored (local fidelity, dev-folder audit, Word read-back).
- **TypeScript:** 127 pass (shell 75, OneDrive 21, add-in 31).
- Lint, typecheck, formatting, licence headers and fixture checks pass.

**Not done**

- **Sign-in and OneDrive in a real account, and Word on the web.** They need the maintainer's app registration (`docs/setup/microsoft-app-registration.md`); the steps are in `docs/testing/word-personal.md`. Until then, OneDrive is tested against the fake Graph only. Still open:
  - whether OneDrive re-checks `If-Match` when an upload session finishes (the diagnostics screen has a button for it);
  - the real-account comparison of the two layouts.
- **PDF copies:** skipped in v0 (decision 0008).
- **No sign-in fallback** for Office without nested app authentication: the panel says to update Office.
- **No blob cache across sessions:** start-up didn't need one.
- **Store listing and production hosting:** out of scope.

**Known issues**

- Files over 4 MB can't be changed while open (an `insertFileFromBase64` limit); the panel says so.
- Word asks the person to confirm whenever the add-in opens a file ("Open" on a version).
- After Word restarts, the Lineage button sometimes appears only after **Home › Add-ins › Developer Add-ins › Lineage** has been used once. Seen with the sideloaded development manifest.
- In the Word run, the bullet under a new job was placed "Under 'Experience'" rather than under the new job's heading. The wording is minor, but it differs from the synthetic case.
- A version's storage ref is its path. Renaming or moving it in OneDrive loses the link until Phase 3 can find it by its ids.
- Some screen captures show WebView2 task panes as blank. For automated checks, start Word with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` for that one launch and drive the pane through the DevTools protocol, then close that Word.
- With the organisation's sensitivity labels, Word may mark a document changed on opening ("Confidential*"). That is not Lineage, but it makes Word ask to save.

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

## Phase 3

Not started. It comes after the maintainer's OneDrive and Word-on-the-web run of Phase 2.

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

1. The maintainer registers the app and runs `docs/testing/word-personal.md` on Word for Windows and Word on the web, with OneDrive.
2. Fix whatever that run finds.
3. Phase 3 (returned files).
