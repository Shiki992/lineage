# Status

## Session 0: orientation (2026-10-14 to 15). Done.

- Read CLAUDE.md, BLUEPRINT.md, PROMPTS.md and every board of the blueprint canvas.
- Repo connected to https://github.com/Shiki992/lineage (branch `main`, from the GitHub initial commit with LICENSE and .gitignore). Nothing committed yet.
- Rust installed: stable 1.99.0 with clippy, rustfmt and `wasm32-unknown-unknown`. Visual Studio 2026 C++ tools present. Node 24.3.0 present.
- pnpm 12.11.2 installed through npm (corepack 0.33 can't run pnpm 12's native binary). `%APPDATA%\npm` added to the user PATH.
- Copyright holder and DCO sign-off: Hrishikesh Vedantam. Repo-local git `user.name` set.
- Decisions: 0001 no command line for users, 0002 change states, 0003 accepted changes land as tracked changes, then a checkpoint. CLAUDE.md, BLUEPRINT.md and PROMPTS.md updated to match. PROMPTS.md moved into docs/.
- `test/` holds the maintainer's own Word documents (42 resumes, 3 business documents). They contain real personal data, so `/test/` is git-ignored. Use them for local fidelity runs only.
  - Contents, by structure only: 4 resumes saved by Word (including the master), 38 written by a script; the business documents mix both. Tables, images and multiple sections appear. There are no tracked changes, fields or footnotes, so generated fixtures must cover those.

## Not done / open

- SECURITY.md contact: no email given. Plan: point to GitHub private vulnerability reporting on the repo (the maintainer must turn it on in repo settings).
- LibreOffice is not installed, so its smoke tests will be skipped.
- BLUEPRINT open decision 5 (start from files made by hand): which phase.
- Whether a change left pending (never clicked) should also go quiet after the master moves on by a set number of checkpoints, like the undetected case in 0002.
- Canvas mismatches still to fix on the canvas or in BLUEPRINT: solid dark boxes used for both "offered change" and milestone; Send a copy defaults to PDF and has no "leave out the ID" switch; "slide notes" as a review channel; Drive properties vs ledger; "Restore as the master" and "Share family"; EMEA "caught up at v2" vs "took v5".
- Re-verify Docs API suggest-mode writes before Phase 5.

## Next

Phase 0 (repo foundations), in plan mode.
