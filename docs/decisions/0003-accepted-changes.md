# 0003: How accepted changes land in the file

Date: 2026-10-15. Status: accepted.

## Context

Changes offered from a master, or from a file that came back, are reviewed in the panel's "what changed" view (decision 0001). Once the person has decided, the engine has to write them into the real file. It could write plain edits, or Word tracked changes (`w:ins` / `w:del`).

## Decision

1. The owner approves changes in the panel. Each approved change is tagged merged (decision 0002).
2. The engine writes the approved changes into the file as real tracked changes, attributed to "Lineage", so the edit is visible in Word's own review tools.
3. Lineage then saves a checkpoint in the version history, with a note such as "Brought in 2 changes from master v3". Its parents are the variant's previous checkpoint and the master checkpoint the changes came from.

Overlaps are never written until the owner picks a side in the panel.

## Alternatives

- Plain edits. Rejected: the owner wants to see in Word exactly what Lineage changed.
- Tracked changes with no automatic checkpoint. Rejected: the history would not show when a change was brought in.

## Consequences

- If the person later rejects a Lineage tracked change in Word, the next checkpoint shows it removed. Lineage treats that as a local edit and tags the change declined (`how: detected`), so it is not offered again.
- Sent copies must still be clean: the send path accepts or strips all tracked changes before writing the copy (CLAUDE.md non-negotiable).
- The tracked-change writer is on the Phase 1 critical path and needs round-trip tests that open in Word and LibreOffice.
