# lineage-harness

Test-only. Never shipped (decision 0001: no user-facing command line).

From Phase 1 it holds:
- local-folder and in-memory storage behind the same interface the TypeScript storage adapters implement,
- one function per button on the personal screens (start tracking, save checkpoint, make a tailored version, offer, bring in or decline, send a copy, and so on),
- scenario tests that click through those functions the way a person would.

```bash
cargo test -p lineage-harness
```
