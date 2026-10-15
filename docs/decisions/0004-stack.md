# 0004: Stack and dependencies for Phase 0

Date: 2026-10-15. Status: accepted.

## Context

CLAUDE.md fixes the shape: a Rust engine compiled to WebAssembly, thin TypeScript shells, pnpm, strict TypeScript, Vitest, and a decision note for every dependency. Phase 0 sets up the workspaces, the Rust → wasm → TypeScript bridge, CI, the v0 change-list schema and a fixture generator. Versions were checked on 2026-10-15.

## Decision

### Toolchains

| Tool | Version                                                                                   | Why                                                                |
| ---- | ----------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Rust | 1.99.0, pinned in `rust-toolchain.toml` with rustfmt, clippy and `wasm32-unknown-unknown` | Current stable; pinned so every machine and CI builds the same way |
| Node | 24 (`.nvmrc`, `engines`)                                                                  | Current LTS; required by Vitest 5 and ESLint 10                    |
| pnpm | 12.11.2 (`packageManager`)                                                                | Required by CLAUDE.md; exact versions with `savePrefix: ""`        |

### Rust crates

| Crate                             | Where                           | Why                                                                                                        | Alternatives considered                                                                         |
| --------------------------------- | ------------------------------- | ---------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| `wasm-bindgen =0.2.129`           | `crates/wasm`                   | The standard Rust/JS bridge. Pinned exactly because the CLI must match                                     | Hand-written `extern "C"` exports: much more unsafe glue, no typed JS                           |
| `boon 0.6.1`                      | dev-dependency of `crates/core` | Validates the spec examples. Passes the JSON Schema draft 2020-12 test suite, with a small dependency tree | `jsonschema 0.58`: much larger tree (regex engines, optional HTTP). `valico`: older drafts only |
| `serde_json 1`                    | dev-dependency of `crates/core` | Parses examples for boon                                                                                   | None reasonable                                                                                 |
| `zip 9.0.3` (no default features) | dev-dependency of `xtask`       | An **independent** reader to check the fixture generator's output, including CRCs                          | Trusting our own writer: circular                                                               |

The fixture generator writes stored zips itself (about 100 lines in `xtask/src/zip_writer.rs`) instead of using `zip` to write. That gives byte-identical output on every OS with no compression backend involved. **The engine's own zip and XML crates are a Phase 1 decision**, with fidelity (untouched XML stays exactly as it was) as the main test.

### wasm-bindgen CLI rather than wasm-pack

wasm-pack 0.15.0 still exists. We need only the JS glue, not npm package generation, and the rustwasm organisation that hosted both tools was archived in July 2025; wasm-bindgen moved to its own organisation (guide at https://wasm-bindgen.github.io/wasm-bindgen/). `scripts/build-wasm.mjs` runs `cargo build` and then `wasm-bindgen --target web`. It first checks that the CLI version equals the one in `Cargo.lock`, the main thing wasm-pack would otherwise do for us.

`--target web` is used everywhere. The task pane loads it in the browser, and Node tests call `initSync` with the bytes.

### TypeScript packages (all dev-dependencies)

| Package                | Version                      | Why                                                                                                                                                   | Alternatives considered                                                                                                           |
| ---------------------- | ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| `typescript`           | **6.0.3**                    | Strict typing                                                                                                                                         | **7.0.2** (the native compiler) is out, but typescript-eslint 8.71 supports only TypeScript below 6.1. Upgrade when it supports 7 |
| `eslint`, `@eslint/js` | 10.12.0, 10.0.1              | Linting, flat config                                                                                                                                  | Biome: faster, but no type-aware rules comparable to `strictTypeChecked`                                                          |
| `typescript-eslint`    | 8.71.2                       | Type-aware rules (`strictTypeChecked`, `stylisticTypeChecked`)                                                                                        | None comparable                                                                                                                   |
| `prettier`             | 3.9.10                       | Formatting for TS, JSON, YAML and Markdown                                                                                                            | Biome formatter: weaker Markdown support                                                                                          |
| `vitest`, `vite`       | 5.0.4, 8.3.4                 | Required by CLAUDE.md. `vite` is listed because it is Vitest's peer                                                                                   | none                                                                                                                              |
| `@types/node`          | 24.19.2                      | Node types for tests and scripts                                                                                                                      | none                                                                                                                              |
| `ajv`                  | 8.20.0 (in `@lineage/shell`) | JSON Schema 2020-12 validation of the spec examples, mirroring boon. Phase 2 decides whether it also validates values at runtime at the wasm boundary | `@cfworker/json-schema`: smaller but less strict-mode checking                                                                    |

### CI

- GitHub Actions, read-only permissions, every action pinned to a full commit SHA.
- `taiki-e/install-action` installs the pinned `cargo-deny` and `wasm-bindgen` binaries, instead of adding `EmbarkStudios/cargo-deny-action`. One third-party action instead of two, and the same cargo-deny version as local runs.
- **DCO** is checked by a short script in the workflow rather than the DCO GitHub App, so nothing needs installing on the repository.

### Licence header

The SPDX short form, `// Copyright 2026 Hrishikesh Vedantam` and `// SPDX-License-Identifier: Apache-2.0`, enforced by `cargo xtask check-headers`. The full LICENSE is at the root and NOTICE names the copyright holder.

## Consequences

- Contributors install the wasm-bindgen CLI at the exact version; the build script tells them the command.
- TypeScript stays on 6.0 until typescript-eslint supports 7. Revisit at each phase start.
- `cargo-deny` allows only Apache-2.0, MIT and Unicode-3.0 today. A dependency with another licence needs a decision note before it can merge.
- **Wasm baseline**, release build with `opt-level = "s"` and LTO, before wasm-opt, measured 2026-10-15:
  - size: 21,378 bytes raw, 8,545 bytes gzipped, plus 6,070 bytes of JS glue,
  - Node `initSync`: about 2 ms warm (10 ms on first run).

  Task-pane budgets are set in Phase 2 from real measurements.
