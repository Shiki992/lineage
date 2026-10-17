# 0017: The deck model, and how slides and shapes are matched

Date: 2026-10-17. Status: accepted.

## Context

Phase 4 (`docs/PROMPTS.md`) brings PowerPoint decks to the engine: "a .pptx model (slides → shapes → text), matching by ID, then layout, then position and text similarity. Diff, three-way merge and write." BLUEPRINT §4 makes a slide shape the unit of change for decks. The fixtures must cover reordered and duplicated slides, edited charts (as whole objects), speaker notes and hidden slides.

The Word engine's rules carry over (decision 0005): untouched XML stays byte for byte, a returned file is untrusted input, and integers stay integers.

What PowerPoint does with its ids was checked on real saves, not assumed. `scripts/powerpoint-resave.ps1` drives PowerPoint 16 (Current Channel) through COM on the invented pitch deck and keeps nine results in `fixtures/pptx/powerpoint/`. They showed:

- a slide keeps its `p:sldId@id` and its `p14:creationId` through saves;
- a duplicated slide, and one brought in with "Reuse slides", gets a new id and a new creation id;
- PowerPoint can give a new slide the id of a deleted one;
- every save renews each chart series' `c16:uniqueId`;
- the first save of a table adds `a16:rowId` and `a16:colId` to every row and column (found in the Phase 4 slide-replacement spike);
- a comment adds a link in the slide's own `p:extLst`;
- the first save of a deck another program wrote rewrites the slide masters, layouts, theme and each chart's markup, though not its figures or text.

## Decision

### The model (`crates/core/src/pptx/`)

- **A deck** is its package (the same reader and writer as Word, so untouched parts are byte for byte), plus:
  - the slides in show order (`p:sldIdLst`);
  - sections (`p14:sectionLst`);
  - a key for the design: slide masters, layouts and theme;
  - the layouts by part, name and type.
- **A slide** has:
  - its id (`p:sldId@id`) and creation id (`p14:creationId`, when there is one);
  - hidden or not (`show="0"`);
  - its layout;
  - its shapes in drawing order;
  - its speaker notes (the notes slide's body placeholder);
  - keys for its settings (layout, background, transition) and its animations (`p:timing`).
- **A shape** has:
  - its id (`cNvPr@id`), name and kind: text, drawing, picture, table, chart, group, diagram (SmartArt), media, OLE object, connector or other;
  - its placeholder (type and index);
  - its geometry in EMU (`i64`);
  - its paragraphs and runs;
  - a content key and a format key.
- **Pictures, tables, charts, groups and SmartArt are whole objects.** Their content key covers the shape's XML and the parts it reaches, one level down. A picture's media is compared by its bytes; an XML part (a chart) by its canonical form.
- **A chart's content is its figures**: the cached values in the chart part. Its data workbook isn't compared, because PowerPoint re-saves it on its own. A change to how a chart looks, with the same figures, is formatting.

### Canonical keys

Keys are FNV hashes of canonical XML. They leave out what PowerPoint rewrites without anyone changing the slide:

- spell-check and language marks on runs (`lang`, `altLang`, `dirty`, `err`, `noProof`, `smtClean`, `smtId`, `bmk`);
- `a:endParaRPr`;
- shape ids (`cNvPr@id`);
- `c16:uniqueId`, `a16:rowId` and `a16:colId`, and extension lists that hold nothing else;
- the slide's own `p:extLst`.

Relationship ids are replaced by what they point at, so the same picture under another `rId` is the same picture.

### Matching slides, in order of trust

1. **Creation id**, when it is unique on both sides.
2. **Slide id**, but not when both slides have creation ids and they differ (PowerPoint reuses deleted slides' ids), and only when the slides look related: at least 30% similar text, the same title, or both empty.
3. **Content:** title, layout and text similarity (Dice over words, as for Word). Two slides pair when they are at least 50% similar (`PAIR_SIMILARITY`), or have the same title and are at least 20% similar. The best-scoring pairs win (similarity, plus 20 for the same title and 10 for the same layout), and ties go by order, so the result is the same every time.

A duplicated slide has a new id and creation id while its original keeps both, so a duplicate is always an added slide, never a match for the original. Moves are the slides off the longest common order of the matched ones.

### Matching shapes on a matched slide

1. **Shape id**, when unique on both sides and confirmed: the same kind, and the same placeholder, content, at least 30% similar text, at least half the same place, or both empty.
2. **Placeholder** type and index.
3. **The rest by kind:** the same content first, then text similarity, overlap of position and the same name.

### What changed

- **Units:** a shape, a slide's notes, its settings, its animations, a whole slide (added or removed), its place in the order (moved), and the design.
- **Kinds:** Word's (insert, delete, modify, move, formatting).
  - Text changes carry word-level marks for the panel.
  - A shape that only moved or changed size, or whose only change is formatting, is formatting.
- **Change ids** come from the change's content, so the same two decks always give the same ids.
- **Summaries** are grouped under their slide: "'$1.2M' changed to '$2.0M'" under "Slide 6 · Traction", "Added slide 8 · Pricing", "Moved slide 6 · Market", "Hid this slide", "The chart changed: '1200' changed to '2000'".
- **`spec/format`** gains `pptxLocation` (slide index and id, shape id), with examples. The engine's output is checked against the schema.

### Brought in by hand, never written

The engine refuses to write, and shows with a reason:

- SmartArt, media, OLE objects and other kinds it doesn't model;
- a change to the slide masters, layouts or theme;
- a slide whose layout the version doesn't have;
- animation changes, and deleting a shape that has animations.

## Alternatives

- **Reusing Word's alignment (`diff/align.rs`) through a shared trait.** That was the plan, but slides aren't a sequence of paragraphs. They have ids that mostly work, and they are reordered as a whole. A matching that trusts ids first and scores the rest was simpler, and it leaves Word's code untouched.
- **Slide ids only.** PowerPoint reuses ids, and two decks can each add a different slide 260, so ids alone would pair unrelated slides.
- **Text only.** Pitch decks repeat titles and bullets across versions; without ids, a moved slide could look like two edits.
- **Charts compared by their workbook.** PowerPoint rewrites the workbook on its own, so every save would show a chart change.

## Consequences

- Real PowerPoint saves show only what a person did: the nine PowerPoint fixtures are each found as made (`crates/core/tests/powerpoint_fixtures.rs`).
- The first save of a deck another app wrote can still show "The slide master, layouts or theme changed" and chart formatting once. These are formatting or design changes, never text.
- A future PowerPoint that renews other ids on save would show changes that aren't there. New fixtures from `scripts/powerpoint-resave.ps1` would catch it.
- `cargo-fuzz` covers the reader (`pptx_reader`); four property tests cover diff and merge (`crates/core/tests/deck_properties.rs`).
