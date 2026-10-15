# lineage-testkit

Test-only. Never shipped.

Builds synthetic Word documents from hand-written WordprocessingML:

- `cargo xtask fixtures` writes the files listed in `src/fixtures.rs` to `fixtures/docx/synthetic/`.
- Engine tests use the same builder to make documents in memory, including random ones for property tests.

All content is invented. Never put real documents or real people's data here.

```bash
cargo test -p lineage-testkit
```
