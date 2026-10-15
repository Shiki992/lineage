# 0007: How master changes are brought in, and what a sent copy leaves out

Date: 2026-10-15. Status: accepted.

## Context

BLUEPRINT §4 asks for a conservative merge: a change is applied automatically only when no other change touches the same unit, and everything else is shown to the person. Decision 0003 says brought-in changes are written as tracked changes by "Lineage". CLAUDE.md says copies that leave are clean. Phase 1 had to pin down what a unit is, what counts as an overlap, which changes this version cannot write safely, how the end of the document is handled, and exactly what a clean copy removes. The maintainer chose (2026-10-15) that sent copies also drop hidden text and the page thumbnail, and confirmed that moves shown as a deletion plus an insertion are how people want to see them.

## Decision

### Units and statuses

The unit is a **paragraph**, a **table cell** (everything inside a cell is one unit, nested tables included), a **table row** (for adding or removing rows), a **section break** or an **opaque block**. A change to a whole table (columns, merged cells, table formatting) touches every unit in it.

Each master change gets one status against the tailored version:

| Status        | When                                                                                     | Panel                                                        |
| ------------- | ---------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| Clean         | The version left the unit alone                                                          | Bring in / Decline                                           |
| Already there | The version made the same change, or the text was typed in anywhere in the story (≥ 90%) | Listed, not offered; tagged merged (detected, decision 0002) |
| Overlap       | Both changed the unit                                                                    | Both sides shown; Keep mine / Use the master's               |
| By hand       | This version cannot write it safely (below)                                              | Shown with the reason; never written                         |

Overlap kinds: both edited; the master deleted what this version edited; the master edited what this version deleted; both added text in the same place (with "Keep both, mine first" and "Keep both, master's first" as well); one changed formatting and the other the words. Bringing in an overlap without choosing a side is refused. A match below 90% is shown as an overlap, never tagged merged.

### Brought in by hand (for now)

| What                                                  | Why it isn't written yet                                                                                                                                                                                                                                                                      |
| ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Footnote and endnote changes                          | Notes live in their own part, tied to a reference run by an id that Word renumbers. Bringing one in means matching notes by their references, allocating ids, and writing the reference and the note together as tracked changes.                                                             |
| Section breaks and page layout                        | A section's properties sit in the mark of its last paragraph. Adding or removing a break moves that mark, merges or splits sections, and every header and footer reference must point at a part that exists in this version.                                                                  |
| Table columns, merged cells, table formatting         | Columns and merges change every row's cells. Word's tracked cell insertions and deletions (`w:cellIns`, `w:cellDel`) are not reliably supported, so the only safe write would be "delete the whole table, insert the new one", which would also undo this version's own edits in other cells. |
| New pictures and other new parts                      | A picture is a separate part reached through a relationship. Bringing it in means copying the part, adding a relationship and content type, rewriting `r:embed`, and making drawing ids unique, or Word repairs the file.                                                                     |
| Inside a table of contents or a block content control | The text there is a field result or bound content; editing it by hand breaks the field or the binding.                                                                                                                                                                                        |
| Paragraphs with unaccepted tracked changes            | Writing new revisions over existing ones risks a result neither accept nor reject explains. The person accepts or rejects them in Word first.                                                                                                                                                 |
| Markup the two files write differently                | Content copied from the master must use the same namespace prefixes as the version; if not, it is not copied.                                                                                                                                                                                 |

Each is a reason shown in the panel (`view::by_hand_reason`), not a silent skip.

### How it is written

- Only the paragraphs, rows and tables a change touches are rewritten; everything else is copied byte for byte (0005).
- Edits inside a paragraph are word-level `w:del`/`w:ins`; formatting changes are `w:rPrChange` and `w:pPrChange`.
- **Moves** are a deletion where the text was and an insertion where it now is. This is how people want to see them, and it avoids Word's `w:moveFrom`/`w:moveTo` quirks.
- **The end of the body or a cell.** Word cannot delete the last paragraph mark of a sequence. So the final mark always stays with the original last paragraph, using Word's own technique (`write/ends.rs`), and accepting or rejecting never leaves an empty paragraph:
  - when paragraphs are added after the last one, the old last mark is marked inserted and the final added paragraph takes over the old final mark, with the old formatting in `w:pPrChange`;
  - when the last paragraph is replaced, it is rewritten into its replacement;
  - when it is deleted, the paragraph before it hands over its mark.
- A deleted paragraph right before a table is deleted outright, and an inserted one there disappears when rejected. Both were checked in Word.
- The writer re-parses every part it changed and refuses to return one that is not well-formed XML.

### What a sent copy leaves out

`clean::clean_copy` accepts every tracked change in every story, then removes:

- comments: the comment parts, `people.xml`, comment ranges and reference runs;
- revision ids (rsids), document variables and the attached template;
- core and app properties (author, last editor, company, template, editing time and title: left as empty elements);
- custom properties other than `LineageFamilyId` and `LineageCheckpointId`, or all of them when the person turns the id off;
- the page thumbnail, custom XML data, add-in task panes (`webextensions`) and building blocks (glossary);
- runs with direct hidden formatting (`w:vanish`), counted so the panel can say so.

The copy is checked to reopen, and its text equals the accepted view of what was sent.

## Alternatives

- **Merging inside a paragraph** when both sides edited different words. Rejected for now: BLUEPRINT §4 makes the paragraph the unit, and people expect "both changed this" to be their call.
- **Writing structural table changes as whole-table replacements.** Rejected: it would quietly undo the version's own cell edits.
- **Leaving an empty paragraph at the end** (the first version). Rejected after review: it adds blank lines and sometimes a page.
- **Keeping hidden text in sent copies.** Rejected by the maintainer: a tailoring note in hidden text must never leave.

## Consequences

- Every write path has tests that reopen the output, check the accepted view is the master's result and the rejected view is the version's original, and check element order. `scripts/word-smoke.ps1 -Compare` checks the same in Word.
- The by-hand list is the backlog for making more changes writable; each item is a self-contained piece of work.
- Sent copies lose their title property as well as the author. If people want titles kept, that becomes an option in "Send a copy".
