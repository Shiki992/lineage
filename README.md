# Lineage

Version control for Word, PowerPoint and Excel files, for people who never want to see git.

You keep working the way you already do: one master resume and a tailored version for each kind of role, or one pitch deck and a version for each investor. Every version stays a real file. Lineage adds the part that's missing:

- **Versions that follow a master.** Change the master once, and each tailored version is offered the change. You pick what to bring in, and your own edits stay.
- **What I've sent.** Every copy that leaves is recorded and kept exactly as sent, so you can open what someone actually received.
- **Files that come back.** A returned file is recognised, and you see only the other person's changes.
- **Your storage, no servers.** History lives in your own OneDrive or Google Drive. The project runs no servers.

## Status

Early development. Phase 0 (repository foundations) is in place; the Word engine is next. See [docs/STATUS.md](docs/STATUS.md), the product design in [docs/BLUEPRINT.md](docs/BLUEPRINT.md), and the build phases in [docs/PROMPTS.md](docs/PROMPTS.md).

## How it's built

A Rust engine compiled to WebAssembly does everything that touches document bytes: reading, comparing, merging and writing. Thin TypeScript shells draw the UI inside Word, PowerPoint and Excel (and later Google Docs and Slides) and call platform APIs.

| Path             | What it is                                                                  |
| ---------------- | --------------------------------------------------------------------------- |
| `spec/format/`   | The open change format (JSON Schema), versioned public API                  |
| `crates/core`    | Document engine. Pure Rust, no I/O                                          |
| `crates/lineage` | Family logic: checkpoints, versions, offers, sent copies. Pure Rust, no I/O |
| `crates/wasm`    | WebAssembly bindings                                                        |
| `crates/harness` | Test-only end-to-end harness                                                |
| `packages/shell` | Typed TypeScript wrapper around the engine                                  |
| `apps/`          | Office add-in and later apps                                                |
| `fixtures/`      | Test documents, all invented                                                |
| `xtask/`         | Repository tasks (`cargo xtask`)                                            |

## Developer setup

You need:

- Rust via [rustup](https://rustup.rs). The pinned toolchain in `rust-toolchain.toml` installs itself.
- Node 24 and pnpm 12.
- The wasm-bindgen CLI at the exact version in `Cargo.lock`:

```bash
cargo install --locked wasm-bindgen-cli --version 0.2.129
```

Then:

```bash
pnpm install
```

```bash
cargo test --workspace
```

```bash
pnpm test
```

`pnpm test`, `pnpm lint` and `pnpm typecheck` build the wasm module first. More checks are listed in [CONTRIBUTING.md](CONTRIBUTING.md).

On Windows, keep the clone in a short path (such as `C:\src\lineage`) or turn on long-path support. pnpm's task runner writes files with long names under `node_modules`, and deep folders hit the 260-character path limit.

## Licence

[Apache-2.0](LICENSE). Contributions are accepted under the [Developer Certificate of Origin](CONTRIBUTING.md#sign-off-dco).
