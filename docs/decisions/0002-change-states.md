# 0002: Change states for offered master changes

Date: 2026-10-15. Status: accepted.

## Context

The canvas lets a person bring in some master changes and skip others: slide 6 goes to three of four decks, and a tailored resume made from v1 takes the v3 job. BLUEPRINT tracked a single `lastSyncedMasterCheckpointId` per variant. That cannot record what was skipped, so skipped changes would be offered again forever, or lost.

## Decision

Each master change offered to a variant gets a tag for that variant:

- **merged** when the person brings it in,
- **declined** when the person says no,
- **pending** otherwise.

A pending change is checked against the variant's content:

- If the variant already contains it (copied in by hand, or the file was made outside Lineage), the change is tagged merged (`how: detected`), and the variant is linked as a descendant of that master checkpoint. This repairs the tree.
- If it does not, the variant is treated as independent of that checkpoint. The skipped checkpoint stays in history, where the person can still open it and bring its changes in by hand, but Lineage does not offer it or notify about it. Once later checkpoints have been brought in, the person has moved on.

Declined changes are not offered again unless the person asks. The merge base for a variant is the newest master checkpoint whose changes are all merged or declined for it. Changes after it are judged one by one, so a variant can take v3 without v2.

Stored as `ChangeState` ledger records (BLUEPRINT §4).

## Resolved point

The maintainer confirmed (2026-10-15) that an undetected skipped change goes quiet rather than being offered again.

## Alternatives

- One sync pointer per variant (the old model). Rejected: it can't hold skips.
- Record only explicit clicks, with no content detection. Rejected: files edited by hand or made outside Lineage would show changes they already have.

## Consequences

- The engine needs "does this variant contain this change?" with a confidence score. Low confidence stays pending and is shown to the person, never silently tagged.
- The same detection powers open decision 5 (start from files made by hand).
