# lineage

Family logic: the append-only ledger, checkpoints, tailored versions, offers, change states (decision 0002), sent copies, and identity (custom document properties plus a content fingerprint).

Rules:

- Pure: takes events and blobs in, returns new events and blobs. The host stores them.
- `#![forbid(unsafe_code)]`.
- Depends only on `lineage-core`.

Phase 0 contains only a re-export of the core version constants.

```bash
cargo test -p lineage
```
