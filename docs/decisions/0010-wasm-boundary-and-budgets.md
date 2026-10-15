# 0010: The wasm boundary, and the task pane's budgets

Date: 2026-10-15. Status: accepted.

## Context

Phase 2 puts the Rust engine in a Word task pane. CLAUDE.md asks for schema validation on both sides of the boundary, and BLUEPRINT open decision 4 asks for size and start-up budgets set from real measurements. Phase 1 had measured 1.30 MB raw and 358 KB gzipped for the whole engine, before any optimisation. The maintainer agreed the budgets below on 2026-10-15.

## Decision

### The boundary

- **One class per side.**
  - `crates/wasm` exposes `Engine`: one method per panel button and view, each wrapping a plain Rust function in `api.rs` that is tested natively.
  - `packages/shell` wraps it as a typed `Engine`. `Lineage` sits on top, with one method per button, mirroring `harness::Panel`.
- **Replies.** Every call returns a JSON envelope, `{ok: true, value}` or `{ok: false, error: {kind, userMessage, detail}}`, with bytes beside it: the file, a new file, the blobs to store. The error kinds are:
  - `engine`
  - `ledger`
  - `missingBlobs` (with `hashes`)
  - `wrongFile`
  - `notAllowed`
  - `badInput`

  `userMessage` is the engine's own plain-language text; `detail` is structural and never quotes the document.

- **Validated on both sides.**
  - Rust inputs refuse unknown fields, and check every id, hash and time (`nowMs` must be a whole, safe number).
  - TypeScript checks every output with valibot schemas (`packages/shell/src/schemas.ts`), and its inputs before they cross.
  - `crates/wasm/tests/golden.rs` writes a golden reply of every kind to `packages/shell/test/golden/`, and fails when they drift. The TypeScript tests parse every golden reply with the schemas and draw the panels from them, so a field changed on one side fails on the other.
- **Blobs.** Storage is asynchronous and engine calls are not. The engine keeps fetched blobs (capped at 64 MB, checked against their hashes), and a call that lacks one reports `missingBlobs`. The host fetches those and calls again, at most six rounds. A missing part brings every other missing part of its snapshot, so each snapshot costs one round. Manifests and change lists are fetched up front when a file opens.
- **No clocks or randomness in the engine.**
  - New ids come from SHA-256 of a 32-byte seed from `crypto.getRandomValues` and a counter (`SeededIds`).
  - Time is passed in.
- **Crashes.** A Rust panic traps the module. The shell marks the engine crashed, every later call fails with "Close and reopen the panel", and nothing is written.
- **No runtime ajv.** It compiles validators with `new Function`, which the pane's Content Security Policy blocks; valibot needs no eval. ajv stays as a test dependency for the public change-list schema.

### Build

The wasm builds with its own Cargo profile (`wasm`: release with `opt-level = "z"`), and `wasm-bindgen --remove-name-section --remove-producers-section`. Function names were a third of the file. `wasm-opt -Oz` (binaryen 132) was measured and rejected: it cut the raw size 9.4% but made the gzipped transfer 7% larger (347,712 bytes against 324,081).

### Budgets and measurements

| Budget                                             | Limit                    | Measured 2026-10-15                                                                                                                                             |
| -------------------------------------------------- | ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| wasm, raw (CI)                                     | 1.5 MB                   | 907,598 bytes                                                                                                                                                   |
| wasm, gzipped (CI)                                 | 420 KB                   | 324,081 bytes (brotli 260,578)                                                                                                                                  |
| JavaScript, gzipped (CI)                           | 250 KB                   | 153,884 bytes (React, MSAL, valibot and the app)                                                                                                                |
| Engine ready in Word (fetch, compile, instantiate) | 500 ms cold, 200 ms warm | 15 loads: 30 to 90 ms warm, 192 to 251 ms on the first load after Word started                                                                                  |
| Panel ready, local storage                         | 800 ms                   | 15 loads: median 496 ms; 13 under 800 ms, the 2 over (816 and 867 ms) on Word's first seconds after starting                                                    |
| One engine command on a resume                     | 300 ms                   | Synthetic resume: 1 to 52 ms per command. The maintainer's 44 documents under 100 KB: start tracking at most 249 ms, what changed at most 144 ms (median 15 ms) |

`scripts/check-budgets.mjs` fails CI over the first three. The others were measured in Word 16.0.20430 on this computer, through the dev server's log, and in Node for engine-only times. They are recorded here and in STATUS, not enforced.

Word's own work is extra:

- reading the document takes about 0.2 to 0.6 s;
- writing a brought-in change and reading it back takes about a second for a resume (decision 0008).

## Alternatives

- **A serde derive on the core and lineage view types**: less code, but it ties core's types to a JavaScript format. The DTOs in `crates/wasm/src/dto.rs` are the format and can be versioned on their own.
- **Passing every blob into every call**: simple, but copies megabytes across the boundary on every click.
- **ajv standalone, precompiled validators**: no eval, but a build step and generated code to review; valibot is smaller.
- **`opt-level = "s"`**: 1.04 MB raw and 378 KB gzipped without names, still inside the budgets but with less headroom.

## Consequences

- Adding a field to the boundary means changing `dto.rs`, `schemas.ts` and the golden replies together; CI catches a missed one.
- A panel is ready in under half a second warm, so there is room for the OneDrive round trips. That run is the maintainer's (`docs/testing/word-personal.md`).
- If the engine grows, the gzip budget leaves about 96 KB of headroom.
