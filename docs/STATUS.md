# Status

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

Phase 1 (Word engine and family logic), in plan mode, after Phase 0 is reviewed and merged.
