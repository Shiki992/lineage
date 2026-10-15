# lineage-harness

Test-only. Never shipped (decision 0001: no user-facing command line).

- `src/storage.rs`: the storage interface the TypeScript adapters mirror, with `MemoryStorage` and `LocalFolderStorage`. Layout per family: `Lineage/<familyId>/ledger.jsonl` and `Lineage/<familyId>/blobs/<sha256>`, with the person's files in the folder itself. Ledger appends are optimistic (the version read must still be current), and blobs are checked against their names.
- `src/lib.rs`: `Panel`, one method per button on the personal screens: start tracking, save checkpoint, make a tailored version, show what changed, offer to versions, updates, bring in, decline, decide (for overlaps), send a copy, what I've sent, open what they received. Each returns what the panel would draw.
- `tests/personal_resume.rs`: the Phase 1 "done when" scenario, in memory and on a real folder.
- `tests/dev_folder.rs`: ignored; audits what the Word task pane wrote to its development folder with the Rust engine alone, and with `LINEAGE_SMOKE_DIR` set writes each tailored version for `scripts/word-smoke.ps1 -Compare`.
- `tests/scenarios.rs`: text typed in by hand, v3 without v2, overlaps, sending without the id, unsaved changes before sending, append conflicts, folder safety.

"The person edits the file in Word" is simulated by replacing the file's `word/document.xml` with one made by `lineage-testkit`, never by the engine's own writer.

```bash
cargo test -p lineage-harness
```
