# 0006: Dependencies added in Phase 1

Date: 2026-10-15. Status: accepted.

## Context

CLAUDE.md asks for a small dependency tree and a decision note for every new crate. Decision 0005 covers the package and XML crates (rawzip, miniz_oxide, roxmltree). This note covers everything else Phase 1 added. Versions were checked on crates.io on 2026-10-15. Every licence is one `deny.toml` already allows (an OR-expression counts when one alternative is allowed). `cargo deny check` passes.

## Decision

### Shipped (in `crates/core` and `crates/lineage`, and so in the wasm bundle)

| Crate        | Version, features                  | Licence           | Why                                                                                                                                                                                                                                                                                                                |
| ------------ | ---------------------------------- | ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `sha2`       | 0.11.1, `default-features = false` | MIT OR Apache-2.0 | SHA-256 names every stored blob and derives change ids. Content addressing is also an integrity check on the person's history, so we use the audited RustCrypto implementation. It brings `digest`, `block-buffer`, `crypto-common`, `hybrid-array`, `typenum`, `cpufeatures` and `cfg-if`, all MIT OR Apache-2.0. |
| `serde`      | 1, `derive`                        | MIT OR Apache-2.0 | The change list (`spec/format`) and the ledger are public JSON formats. Hand-written JSON would be a second parser to fuzz.                                                                                                                                                                                        |
| `serde_json` | 1                                  | MIT OR Apache-2.0 | As above. Brings `itoa`, `memchr`, `ryu` (Apache-2.0 OR BSL-1.0) and `serde_core`.                                                                                                                                                                                                                                 |

The derive macros bring `proc-macro2`, `quote`, `syn` and `unicode-ident` ((MIT OR Apache-2.0) AND Unicode-3.0) at build time only.

### Test-only

| Crate                  | Where                        | Licence                      | Why                                                                                                                                                                                                                                       |
| ---------------------- | ---------------------------- | ---------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `proptest` 1.12.0      | dev-dependency of core       | MIT OR Apache-2.0            | The property tests CLAUDE.md asks for. `default-features = false, features = ["std"]` drops `fork`, `timeout` and `tempfile`, which would pull a second `getrandom` and fail the duplicate check.                                         |
| `zip`, `flate2`        | dev-dependencies             | MIT; MIT OR Apache-2.0       | An independent zip reader for package tests (see 0005). `flate2` uses the Rust backend (`miniz_oxide`), so no C library and no Zlib-only licence.                                                                                         |
| `lineage-testkit`      | workspace crate, test-only   | Apache-2.0 (ours)            | The synthetic .docx builder, moved out of `xtask` so engine tests can build documents in memory, including random ones for property tests. `xtask` still writes the committed fixtures from it; the ten Phase 0 files are byte-identical. |
| `libfuzzer-sys` 0.4.13 | `fuzz/` only (own workspace) | (MIT OR Apache-2.0) AND NCSA | cargo-fuzz's standard harness. NCSA is LLVM libFuzzer's licence. The `fuzz/` workspace is never shipped and is outside the workspace `cargo deny` checks, so NCSA is not added to `deny.toml`.                                            |

## Alternatives

- **`similar` for sequence diffs.** Apache-2.0 and dependency-free, with Myers and patience. Rejected: alignment is the heart of the engine, with rules of our own (formatting-only pairs, moves, similarity pairing, table rows), and its deadline support calls `std::time::Instant`, which panics on `wasm32-unknown-unknown`. Our LCS (`diff/lcs.rs`) is about 150 lines with patience anchors, a capped DP fallback and property tests.
- **A hand-written SHA-256.** About 120 lines and testable against FIPS vectors, but any slip silently breaks content addressing for every family. Not worth saving six small crates.
- **`uuid` and `chrono`.** Not needed: the host supplies ids (`IdSource`) and time (`Timestamp`), and `ids.rs` formats dates with a 20-line civil-date conversion. The engine stays free of clocks and randomness, so every command is reproducible in tests.
- **`proptest` with default features.** Rejected for the duplicate `getrandom` it brings.

## Consequences

- The wasm bundle grows by `sha2`, `serde` and `serde_json`, measured in STATUS.
- Change ids and blob names are SHA-256 based and stable across platforms; in-memory matching keys use FNV-1a 64 (`core/src/hash.rs`), which is fully specified and never stored.
- Running the fuzz targets needs a nightly toolchain and cargo-fuzz, installed separately (`fuzz/README.md`). CI pins both.
