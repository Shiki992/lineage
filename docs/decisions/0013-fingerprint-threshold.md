# 0013: When a file without ids is recognised, and when Lineage asks

Date: 2026-10-15. Status: accepted.

## Context

BLUEPRINT open decision 3: the match threshold for returned files, and how uncertain matches are shown. CLAUDE.md: never merge silently when unsure. The cost of a wrong automatic match is real:

- the review is computed against the wrong base, so the person sees changes the outside person never made;
- the "Came back" chip lands on the wrong recipient.

The cost of asking is one click on a list of copies. So precision comes first.

The checkpoint fingerprint from Phase 1 (a bottom-128 sketch of five-word shingles) needs no blobs. But it can't separate checkpoints that differ by a few words, and those are common: a master v2 and v3 a word apart, a tailored version that barely differs from its master.

## Decision

Recognition (`crates/lineage/src/returned.rs`):

1. **Ids first.**
   - A sent-copy id is certain.
   - A checkpoint id is certain when no copy with an id was recorded from it; otherwise the person chooses among those copies and "not recorded".
   - Ids that name nothing known fall through to content.
2. **Shortlist by sketch**, across every family in storage, no blobs: checkpoints with sketch similarity ≥ 20, at most 6 that copies were sent from and 2 that nothing was sent from.
3. **Compare closely** with what actually left (the sent copy's bytes, or the snapshot), word by word, through the engine's "what changed". This gives:
   - `changed`: words removed plus words added;
   - `percent`: `100 × 2 × common / (words in both)`.
     Moved paragraphs cost nothing; formatting is ignored.
4. **Verdict.**
   - **Certain** when the closest recorded copy:
     - matches ≥ **80%**;
     - is the only copy sent from its checkpoint;
     - needs at least **5 fewer changed words** than any copy sent from another checkpoint (the evidence that it is this copy and not that one);
     - and no unrecorded checkpoint needs fewer.
   - Otherwise a **choice**: every candidate ≥ **50%** and within **20 points** of the best, at most 6. An unrecorded checkpoint is listed only when it fits better than every recorded copy.
   - Below 50%: **not recognised**.

How it is shown (`describe`):

- **Certain:** "This came back from Northgate Analytics", the copy (version, date) and "Recognised by its Lineage ID" or "92% of the text matches". One button, **Review their changes**.
- **Choice:** "Which copy is this?", with a line saying why Lineage asks. Radio buttons for each copy (recipient, version, date, match), and **None of these**. Nothing is recorded until the person picks one; the record says `matched_by: person`.
- **Unknown:** "Lineage doesn't recognise this file", and **Choose another file**.
- On an untracked document, the same check runs in the background. A likely match puts "This looks like a copy that came back" above Start tracking, without blocking it.

## Measured

`crates/lineage/tests/recognition.rs` runs in CI over 126 returned files:

- **Families:**
  - Avery's resume: three master checkpoints, two tailored versions, and six sent copies, two of them from one checkpoint;
  - a cover letter with two copies;
  - meeting notes, never sent.
- **Every copy comes back:**
  - saved by Word with ids, saved by Word without ids, as an imitation Google Docs export, and heavily reformatted (with and without ids);
  - each with no edits, light edits or a heavy rewrite;
  - plus six real LibreOffice re-saves, with and without ids.
- **Negatives:** the synthetic fixtures and a stranger's resume on the same template.

| Kind                  | Edits                | Files | Automatic, right | Automatic, wrong | Choice with the right copy | Choice without | Unknown |
| --------------------- | -------------------- | ----- | ---------------- | ---------------- | -------------------------- | -------------- | ------- |
| Word, ids kept        | none / light / heavy | 9     | 9                | 0                | 0                          | 0              | 0       |
| Reformatted, ids kept | all                  | 9     | 9                | 0                | 0                          | 0              | 0       |
| LibreOffice, ids kept | none / light         | 3     | 3                | 0                | 0                          | 0              | 0       |
| Word, no ids          | none                 | 8     | 4                | 0                | 4                          | 0              | 0       |
| Word, no ids          | light                | 8     | 4                | 0                | 4                          | 0              | 0       |
| Word, no ids          | heavy                | 8     | 0                | 0                | 8                          | 0              | 0       |
| Google export         | none / light         | 16    | 8                | 0                | 8                          | 0              | 0       |
| Google export         | heavy                | 8     | 0                | 0                | 8                          | 0              | 0       |
| Reformatted, no ids   | all                  | 15    | 4                | 0                | 11                         | 0              | 0       |
| LibreOffice, no ids   | all                  | 6     | 3                | 0                | 3                          | 0              | 0       |
| Never seen            | n/a                  | 36    | 0                | 0                | 0                          | 3              | 33      |

- **Precision of automatic matches: 100%** (44 right, 0 wrong, none on a file that was never sent).
- **Recall, automatic, where no other copy has (nearly) the same text: 80%** (44 of 55).
  - Every unedited or lightly edited copy is automatic.
  - Every heavy rewrite (about 50 to 60% match) is a choice.
- **Recall, automatic or among the choices: 100%** (90 of 90).
- **Copies that are always a choice without ids:** 35 returned files came from copies whose text is the same as, or within 5 words of, another copy's: two copies from one checkpoint, or checkpoints a word apart. These are always a choice, and the test checks that none is ever automatic.
- **Files never sent:** 33 not recognised. The 3 shown as a choice are synthetic fixtures that contain fragments of Avery's resume.

The test fails CI if any automatic match is wrong, if a file never sent is matched automatically, if twin copies are ever automatic, if a lightly edited copy is ever missed, or if recall falls below these floors (80% automatic, 100% found).

## In Word

Checked on this computer with the dev-folder storage (STATUS, Phase 3):

- a mentor's copy returned with its ids, edited with Track Changes on, was recognised by its id;
- a copy sent without an id, edited and then re-saved by LibreOffice, was a choice between the two copies sent from the same checkpoint and two copies in another family. This is right: without ids they can't be told apart.

## Alternatives

- **Sketch similarity alone, with a percentage threshold.** No blobs needed, but checkpoints a word apart tie, and integer percentages can't separate them.
- **A margin in percentage points between the best and second-best.** On a 100-word resume, one changed word is under 1%. A margin in changed words is the evidence itself.
- **Store per-paragraph hashes in each checkpoint's fingerprint.** Precise without blobs, but it changes the ledger format; comparing against the bytes that left is exact and costs one blob per candidate.
- **Treat unrecorded checkpoints like sent copies.** Most returned files are copies that were sent, and identical unsent checkpoints (a tailored version just made from its master) would turn every certain match into a choice.

## Consequences

- Each candidate costs a diff against its base (a few milliseconds on a resume) and one blob fetch. A fresh pane may need two fetch rounds per family: manifests, then parts. Recognising a file in Word took about 0.4 s with two families in storage.
- The fixture set is synthetic and small. Real Google Docs exports, and resumes far longer than these, should be added to the set as they arrive (`fixtures/docx/returned/google/`), and the floors raised as recall improves.
