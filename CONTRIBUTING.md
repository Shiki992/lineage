# Contributing to Lineage

Thank you for helping. Lineage is for people who never want to see git, so the bar for user-facing work is plain language and files that never get damaged.

Before you start, read [CLAUDE.md](CLAUDE.md) (the project rules, written for people and coding agents alike), [docs/BLUEPRINT.md](docs/BLUEPRINT.md) and [docs/STATUS.md](docs/STATUS.md).

## Ground rules

- **Never corrupt a user's file.** Every write path has a round-trip test.
- **Never merge silently when unsure.** Show both sides and let the person choose.
- **No real documents or personal data** in the repository, issues or logs. Example resumes and decks are invented.
- **Plain language in every screen.** No git words in the UI; see the glossary in CLAUDE.md.
- **New dependencies need a decision note** in `docs/decisions/` saying why, and what else was considered.

## Sign-off (DCO)

Every commit must be signed off under the [Developer Certificate of Origin 1.1](https://developercertificate.org/). Signing off means you certify that you wrote the change, or otherwise have the right to submit it under the project's licence.

Add the line with `-s`:

```bash
git commit -s -m "fix(core): keep bookmarks when a paragraph moves"
```

This appends `Signed-off-by: Your Name <you@example.com>`, using your git `user.name` and `user.email`. CI rejects pull requests that contain commits without it.

## Commits and pull requests

- Small commits, using [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `test:`, `docs:`, `build:`, `ci:`, `refactor:`, `style:`, `chore:`, with an optional scope such as `feat(core):`.
- One topic per pull request. Say what changed, why, and how you tested it.
- CI must be green: formatting, clippy with warnings as errors, Rust tests on Linux and Windows, cargo-deny, licence headers, fixture reproducibility, TypeScript lint, typecheck and tests, and DCO.

## Licence header

Every Rust, TypeScript and JavaScript source file starts with:

```
// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0
```

`cargo xtask check-headers` lists any file without it.

## Adding a test fixture

See [fixtures/docx/README.md](fixtures/docx/README.md). In short:

1. Prefer a **synthetic** fixture: add it to `xtask/src/fixtures.rs`, run `cargo xtask fixtures`, and add a row to the fixtures README.
2. A **hand-made** file (for quirks only a real app produces) must use invented content. Record the app and version that saved it, and its licence.
3. Every engine bug gets a fixture that reproduces it and a test.

## Local checks

Run these before opening a pull request:

```bash
cargo fmt --all --check
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo test --workspace
```

```bash
cargo deny check
```

```bash
cargo xtask check-headers
```

```bash
cargo xtask check-fixtures
```

```bash
pnpm lint
```

```bash
pnpm typecheck
```

```bash
pnpm test
```

```bash
pnpm format:check
```

`cargo deny` needs `cargo install --locked cargo-deny --version 0.20.2`. On Windows with Word installed, `scripts/word-smoke.ps1` opens every fixture in Word as an extra check.

## Code of conduct

Everyone taking part follows our [Code of Conduct](CODE_OF_CONDUCT.md).
