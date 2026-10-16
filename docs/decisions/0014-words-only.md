# 0014: Keeping only the words of a file another app saved

Date: 2026-10-15. Status: accepted.

## Context

The Phase 3 Word run found a problem with files saved again in another app. Keeping a change from a file LibreOffice saved, or one downloaded from Google Docs, also brought that app's formatting into the master as tracked formatting changes:

- Arial, explicit sizes and spacing on every run and paragraph;
- LibreOffice's explicit "Normal" style, `widowControl`, `bidi` and `jc`.

`modified_paragraph` copied theirs' paragraph and run properties along with the text. `inserted_paragraph` copied theirs' paragraph whole, including a `numId` that may name a different list in the master. Decision 0012 listed "keep the words only" as a follow-up.

Comparing what left with what came back (body paragraphs whose text came back unchanged, and how many were reformatted):

| Returned file                    | Reformatted / same text | `Application` in `docProps/app.xml`              |
| -------------------------------- | ----------------------- | ------------------------------------------------ |
| LibreOffice resume, light edits  | 3 / 12                  | `LibreOffice/26.8.1.1…`                          |
| LibreOffice resume, heavy edits  | 1 / 9                   | `LibreOffice/…`                                  |
| LibreOffice cover letter         | 7 / 8                   | `LibreOffice/…`                                  |
| Google Docs imitation (any edit) | 12 / 12                 | no `app.xml` at all (the copy that left had one) |
| Restyled in Word (testkit)       | 12 / 12                 | (the generator's name)                           |
| Edited in Word (testkit)         | 0 / 12                  | (the generator's name)                           |

CLAUDE.md applies here:

- never merge silently when unsure, so the choice is shown and can be changed;
- never corrupt a file, so every new write path has a round-trip test.

## Decision

### When kept changes bring only their words

`lineage_core::origin::formatting_trust(base, theirs, theirs_diff)` says a returned file's formatting can't be trusted, and why:

1. **Another app saved it.** The `Application` in its extended properties names an app Lineage knows: LibreOffice, OpenOffice, NeoOffice, ONLYOFFICE, WPS Office, Google Docs or AbiWord.
2. **It was exported.** It has no extended properties, though the copy that left had them.
   - Word always writes them.
   - Sent copies keep an emptied part (`clean::empty_properties`).
   - A Google Docs download has none.
3. **Its formatting changed almost everywhere.** Of the body paragraphs whose text came back unchanged, at least half were reformatted, and at least 3.

A file saved by Word with a few reformatted paragraphs is trusted.

**The review says it.** `ReturnReview.formatting` gives:

- `words_only`, the default;
- a note for the top of the review: "This file was saved in LibreOffice, which changes fonts and spacing. Lineage brings in their words in your formatting.";
- a sentence for the formatting step: "LibreOffice changes formatting when it saves a file, so these are probably not changes Northgate Analytics made."

The person can tick **Bring in their formatting too** above Apply. `returned::decide` takes the choice as `write::Bring` (`Everything` or `WordsOnly`).

### What "words only" writes

`write::apply_with(plan, decisions, rev, Bring::WordsOnly)`. The formatting of words from theirs comes from a `Look`:

- **A paragraph they edited** keeps its own paragraph properties and runs.
  - Words both have stay as they are here, even where theirs reformatted them.
  - Added words take a neighbouring run's formatting: the nearest unchanged word, before then after, if theirs formats it alike.
  - Otherwise the nearest word of ours before them, so a replaced word takes the formatting of the word it replaces.
  - Otherwise the nearest after, then the paragraph mark's.
- **A new paragraph** takes the formatting of one of ours' paragraphs. The engine looks through theirs, nearest first, for a paragraph with text that is formatted alike:
  - the same paragraph properties;
  - else the same style, and a list item or not, as it is;
  - it must have come from the base (`merge::locate::source_of`, the reverse of `map_path`) and still be in ours.

  It gets that paragraph's properties here: style, list and its `numId`. Each run formatting of theirs maps to the run it has here, by the words the two paragraphs share, weighted by length. So a bold label stays bold and the rest stays plain.

- **A moved paragraph** keeps its own formatting the same way.
- **The end of the document** is handled as before (`ends.rs`):
  - a paragraph added after the last one, or one replacing it, takes its look;
  - the last paragraph mark passes between two of this file's own paragraph formats, which is a tracked paragraph-formatting change, as in whole changes.
- **Overlaps:** "Use theirs" brings their words only where their side changed the words.
- **Fallbacks.** These keep theirs' own formatting, as a whole change does:
  - text inside a link they added (its look goes with the link);
  - a new paragraph with nothing alike in theirs;
  - inserted rows and whole tables.
- **Formatting-only changes** are still written whole. Keeping one asks for that formatting.

`apply` is unchanged (`Bring::Everything`). Every whole-change output is byte-for-byte what it was. `Check` is unchanged: words only copies less of theirs than a whole change, so anything writable stays writable.

### Not in the ledger

The choice is not recorded:

- each sitting starts from the engine's default;
- `ReturnReviewed` and the checkpoint note are as before.

## Alternatives

- **Restyle theirs into ours' formatting, then merge as usual.** It means splitting theirs' runs wherever ours' formatting changes inside one of theirs' runs. The plan would then describe a document Lineage built, not the file that came back. Rejected.
- **Give a new paragraph the formatting of the paragraph it follows here.** A heading added after a bullet would become a bullet. "Formatted alike in theirs" keeps the kind of paragraph they meant. Rejected.
- **One run formatting for a whole new paragraph** (the longest run of the alike paragraph). This loses bold labels. The run map keeps them.
- **Words only for every returned file.** A mentor who makes a word bold in Word means it, and Word's formatting can be trusted. Rejected.
- **One signal only:**
  - the app name misses exports, which have no `app.xml`;
  - the reformatted share misses most LibreOffice resumes (1 to 3 of 9 to 12 paragraphs).
- **Drop a flagged file's formatting-only changes.** That is a silent decision. They stay one skippable step ("Skip all formatting").
- **Record the choice in `ReturnReviewed`.** Nothing needs it yet, and it changes the ledger. It is possible later.

## Consequences

- **Words-only outputs look as if the same words had been typed into the master.** The tests check paragraph and run formatting, character by character, for:
  - the Google imitation's light, heavy, moved and end-of-document edits;
  - all four edited LibreOffice re-saves.

  In Word, after accept-all, every words-only output is in the master's font, and every revision is by Lineage.

- **A new proptest:** another app's file, kept words only, brings all their words, rejecting gives the base, and no paragraph that was already there gets any of its formatting.
- **Limits:**
  - words they deliberately formatted differently from their neighbours lose it (an italic title added to a re-saved file). "Bring in their formatting too" keeps it;
  - "changed almost everywhere" also fires when a person restyled the whole file in Word. The review says so, and the box brings theirs;
  - overlaps where their side is formatting only are still shown as overlaps. They are LibreOffice's noise against the master's own edits;
  - the fallbacks above.
- **The wasm boundary changes:**
  - `decideReturned(.., wordsOnly, nowMs)`;
  - `ReturnReview.formatting`;
  - wasm grows to 1,015,022 bytes raw and 354,113 gzipped, inside the budgets of decision 0010.
