# 0010: The wasm boundary, and the task pane's budgets

Date: 2026-10-15. Status: accepted. Updated 2026-10-17 (Phase 4): the wasm budgets are raised to 4 MB, and the blob rules for "Update N versions". Updated 2026-10-18 (Phase 5) and 2026-10-20 (Phase 5B) with the workbook measurements.

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
  - **Phase 4:** "Update N versions" reads several snapshots for each version. It tries every version before reporting what's missing, fetches the manifests again first, and allows six rounds plus two per version.
  - The shell doesn't store a blob it has already seen in storage in this session (fetched, or stored without an error; blobs are never deleted). A deck's checkpoint names about 40 parts, nearly all unchanged, and one update had stored 110 blobs one at a time.
- **No clocks or randomness in the engine.**
  - New ids come from SHA-256 of a 32-byte seed from `crypto.getRandomValues` and a counter (`SeededIds`).
  - Time is passed in.
- **Crashes.** A Rust panic traps the module. The shell marks the engine crashed, every later call fails with "Close and reopen the panel", and nothing is written.
- **No runtime ajv.** It compiles validators with `new Function`, which the pane's Content Security Policy blocks; valibot needs no eval. ajv stays as a test dependency for the public change-list schema.

### Build

The wasm builds with its own Cargo profile (`wasm`: release with `opt-level = "z"`), and `wasm-bindgen --remove-name-section --remove-producers-section`. Function names were a third of the file. `wasm-opt -Oz` (binaryen 132) was measured and rejected: it cut the raw size 9.4% but made the gzipped transfer 7% larger (347,712 bytes against 324,081).

### Budgets and measurements

| Budget                                             | Limit                     | Measured 2026-10-15                                                                                                                                             |
| -------------------------------------------------- | ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| wasm, raw (CI)                                     | 1.5 MB, 4 MB from Phase 4 | 907,598 bytes                                                                                                                                                   |
| wasm, gzipped (CI)                                 | 420 KB, 4 MB from Phase 4 | 324,081 bytes (brotli 260,578)                                                                                                                                  |
| JavaScript, gzipped (CI)                           | 250 KB                    | 153,884 bytes (React, MSAL, valibot and the app)                                                                                                                |
| Engine ready in Word (fetch, compile, instantiate) | 500 ms cold, 200 ms warm  | 15 loads: 30 to 90 ms warm, 192 to 251 ms on the first load after Word started                                                                                  |
| Panel ready, local storage                         | 800 ms                    | 15 loads: median 496 ms; 13 under 800 ms, the 2 over (816 and 867 ms) on Word's first seconds after starting                                                    |
| One engine command on a resume                     | 300 ms                    | Synthetic resume: 1 to 52 ms per command. The maintainer's 44 documents under 100 KB: start tracking at most 249 ms, what changed at most 144 ms (median 15 ms) |

`scripts/check-budgets.mjs` fails CI over the first three. The others were measured in Word 16.0.20430 on this computer, through the dev server's log, and in Node for engine-only times. They are recorded here and in STATUS, not enforced.

### Phase 4: decks (2026-10-17)

The deck engine took the wasm to 1,406,281 bytes raw and 466,627 gzipped, 47 KB over the 420 KB budget. Phase 3B had ended at 388,922 gzipped.

The maintainer's instruction was to stay inside Word's budget if possible, and otherwise raise the wasm limits to 4 MB. The build already used the smallest settings:

- `opt-level = "z"`;
- no name or producers sections;
- `wasm-opt` had made the gzipped file larger.

So the limits were raised to 4 MB (`scripts/check-budgets.mjs`), leaving the JavaScript budget as it was. Shrinking was not attempted beyond that. The way back under 420 KB is two engines, one per host (Word without the deck code, PowerPoint without the Word writer), with My documents loading both. That doubles the builds and the golden replies, and was left for the maintainer to choose.

Measured at the end of Phase 4: 1,409,100 bytes raw, 467,525 gzipped (brotli 365,291). JavaScript gzipped: task pane 162,848, My documents 158,133.

In PowerPoint for Windows (16.0, Current Channel), with the dev folder, from the dev server's log:

| What                                        | Measured                                                                                                                                          |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| Engine ready (fetch, compile, instantiate)  | 77 loads: median 56 ms, 24 to 90 ms on most; the slowest were while the dev server was rebuilding                                                 |
| Panel ready                                 | 73 loads: median 268 ms; the slow ones were the first load after a dev-server restart or a reload during a rebuild, not separated cleanly         |
| The deck panel (`deckPanel`)                | 166 to 272 ms with one or two checkpoints; 550 to 830 ms once the master had seven                                                                |
| Start tracking, make a version, send a copy | 626 ms, 249 to 266 ms, 654 ms                                                                                                                     |
| Update N versions                           | 3 versions: 6.4 s before the blob and round fixes, 1.9 to 2.2 s after (one checkpoint of the master, one or two per version, three files written) |

Word's start-up was not measured again: Word was open with the maintainer's own document.

The deck panel and "Update N versions" are over the 300 ms command budget, which was set for one command on a resume. The deck panel compares the open deck with each version's history, so it grows with the history. This is recorded as a known issue rather than a new budget.

Word's own work is extra:

- reading the document takes about 0.2 to 0.6 s;
- writing a brought-in change and reading it back takes about a second for a resume (decision 0008).

### Phase 5: workbooks (2026-10-18)

The workbook engine took the wasm to 1,982,567 bytes raw and 657,409 gzipped, inside the 4 MB limits. Task pane JavaScript: 179,648 gzipped. The workbook budgets themselves (open, compare and memory by size, and the 1.5-million-cell limit) are in decision 0025.

In the Excel pane the engine runs in a Web Worker (decision 0025), so a comparison that takes seconds never blocks Office. `Lineage` drives it through promises; Word and PowerPoint keep it in the pane's thread through the same shape. The worker loads in 113 to 254 ms (15 loads; 1.8 and 3.1 s while the dev server was rebuilding).

In Excel for Windows (16.0.20430), with a scratch dev folder, from the pane's log during the three pilots:

| What                              | Measured                                   |
| --------------------------------- | ------------------------------------------ |
| Panel ready                       | 308 to 475 ms, outside dev-server rebuilds |
| Start tracking                    | 496 to 525 ms                              |
| What changed                      | 257 to 580 ms                              |
| Cell side by side                 | 179 ms                                     |
| Save checkpoint (and save)        | 536 ms to 1.0 s; one of 6.8 s              |
| Updates panel                     | 401 to 609 ms                              |
| Bring in (plan, Excel, check)     | 842 ms to 1.65 s                           |
| Keep a returned workbook's change | 1.78 s                                     |

Reading the open workbook (`getFileAsync`) takes 125 to 150 ms for these files, and each bring-in reads it twice. The one 6.8 s checkpoint save came right after a row insert and cell writes through Office.js, as did one 5.6 s bring-in at stop 2; neither repeated.

### Phase 5B: macros, pivot tables and queries (2026-10-20)

Reading and comparing them took the wasm to 2,132,549 bytes raw and 712,227 gzipped (was 1,982,567 and 657,409), inside the 4 MB limits. Task pane JavaScript: 181,396 gzipped; My documents 160,923. Their own budget (at most 100 ms natively and 300 ms in the pane on top of a workbook's) is in decision 0027. In Excel for Windows with the founder's model (macros, two pivot tables, a slicer, two queries), from the pane's log: what changed 209 to 537 ms, save checkpoint 751 ms, the updates panel 532 to 580 ms, a returned file recognised in 488 ms and reviewed in 201 ms.

## Alternatives

- **A serde derive on the core and lineage view types**: less code, but it ties core's types to a JavaScript format. The DTOs in `crates/wasm/src/dto.rs` are the format and can be versioned on their own.
- **Passing every blob into every call**: simple, but copies megabytes across the boundary on every click.
- **ajv standalone, precompiled validators**: no eval, but a build step and generated code to review; valibot is smaller.
- **`opt-level = "s"`**: 1.04 MB raw and 378 KB gzipped without names, still inside the budgets but with less headroom.

## Consequences

- Adding a field to the boundary means changing `dto.rs`, `schemas.ts` and the golden replies together; CI catches a missed one.
- A panel is ready in under half a second warm, so there is room for the OneDrive round trips. That run is the maintainer's (`docs/testing/word-personal.md`).
- If the engine grows, the gzip budget leaves about 96 KB of headroom. (Phase 4: the budget is now 4 MB; see above.)
