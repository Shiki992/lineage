# lineage

Family logic: tracked files, their checkpoints and tailored versions, offers, change states, sent copies and identity. Pure: storage, ids and the clock belong to the host and are passed in. `#![forbid(unsafe_code)]`. Depends only on `lineage-core`.

| Module     | What it does                                                                                                                                                              |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `event`    | The ledger: versioned JSON Lines records (`Lineage/<familyId>/ledger.jsonl`). Unknown versions or fields are errors.                                                      |
| `state`    | `FamilyState`, folded from the ledger, refusing gaps, repeats and unknown references.                                                                                     |
| `blob`     | SHA-256 blobs; checkpoint snapshots as a manifest of part blobs (pictures stored once); sent copies stored as exact bytes.                                                |
| `changes`  | Change states (decision 0002): merged, declined or pending; the merge base; quiet changes; detection of text typed in by hand and of Lineage changes rejected in Word.    |
| `identity` | A fingerprint (bottom-128 sketch of five-word shingles) stored with each checkpoint, and a word-by-word content match, for files whose ids were stripped.                 |
| `returned` | Files that come back: recognising (ids, then content), keeping the file, reviewing only the outside changes, and writing the kept ones (decisions 0012 and 0013).         |
| `commands` | One pure function per panel button: start tracking, save checkpoint, make a tailored version, what changed, offer, updates, decide, send a copy, open what they received. |
| `views`    | The master panel, a tailored version's panel (with files that came back), and "What I've sent" (with their chips).                                                        |
| `ids`      | Validated ids (they name folders) and timestamps with civil-date formatting, without a clock.                                                                             |

```bash
cargo test -p lineage
```

`tests/recognition.rs` reports precision and recall of recognising returned files over the synthetic set (`--nocapture` prints the table).

End-to-end scenarios that press these commands like a person live in `crates/harness`.
