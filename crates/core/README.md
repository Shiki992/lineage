# lineage-core

The document engine. Pure: no file, network or clock access. Callers pass bytes in and get bytes back. `#![forbid(unsafe_code)]`.

| Module    | What it does                                                                                                                                                                             |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `package` | Reads and writes the zip container. Unchanged entries keep their original compressed bytes (decision 0005).                                                                              |
| `xml`     | roxmltree parsing with limits, and byte-range `Edits`: XML we did not change is never re-serialised.                                                                                     |
| `docx`    | The canonical model: stories → blocks (paragraphs, tables, section breaks, opaque) → text, atoms and markers, with byte spans; accepted and rejected views; custom properties (`props`). |
| `diff`    | What changed: block alignment stable under moves, table rows and cells, word-level paragraph diffs, and the v0 change list (`spec`).                                                     |
| `merge`   | Three-way merge: clean, already there, overlap or by hand, per paragraph, cell, row, section break or opaque block (decision 0007).                                                      |
| `write`   | Writes chosen changes as tracked changes by "Lineage" (decision 0003); `Check` says in advance what can be written; `ends` keeps each sequence's last paragraph mark in place.           |
| `clean`   | Clean copies for sending: revisions accepted; comments, rsids, hidden text, metadata and extra properties removed.                                                                       |
| `summary` | Plain-English summaries ("'Data engineer' changed to 'Lead data engineer'").                                                                                                             |
| `view`    | The "what changed" view model the panel draws: content, formatting, overlaps with both sides, by-hand reasons.                                                                           |
| `hash`    | FNV-1a for in-memory keys, SHA-256 for anything stored.                                                                                                                                  |

## Tests

```bash
cargo test -p lineage-core
```

- Unit tests next to each module, using fixtures built in memory by `lineage-testkit`.
- `tests/properties.rs`: property tests (merge and write invariants, splices, zip, damaged input). `PROPTEST_CASES=3000` for a long run.
- `tests/spec_examples.rs`: the v0 schema against its examples.
- `tests/local_fidelity.rs`: ignored; runs over the maintainer's own documents with `LINEAGE_LOCAL_DOCS=test` and prints counts only.
- Word: `LINEAGE_SMOKE_DIR=target/smoke cargo test -p lineage-core`, then `powershell -NoProfile -File scripts/word-smoke.ps1 target/smoke -Compare` checks every output in Microsoft Word (accept all and reject all must match the engine).
- Fuzzing: `fuzz/` (see its README).
