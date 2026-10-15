# lineage-core

The document engine. It reads .docx (later .pptx and .xlsx) packages, builds canonical models, works out what changed, runs three-way merges, writes results back as tracked changes, and writes plain-English summaries.

Rules:

- Pure: no file, network or clock access. Callers pass bytes in and get bytes back.
- `#![forbid(unsafe_code)]`.
- Tests first. Every bug gets a fixture in `fixtures/<format>/` and a test.

Phase 0 contains only version constants and the schema tests for `spec/format/v0` (`tests/spec_examples.rs`).

```bash
cargo test -p lineage-core
```
