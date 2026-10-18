# 0019: Bringing the master's changes into an open deck

Date: 2026-10-17. Status: accepted.

## Context

Decision 0018 updates tailored decks from the master deck's panel by rewriting their stored files. PowerPoint for Windows locks a deck it has open, so a version that is open can't be updated that way: its line says to close it and update again. The maintainer asked for the open-deck path if a spike showed slides can be replaced exactly.

The Phase 4 spike did show that (decision 0018): `insertSlidesFromBase64` with `UseDestinationTheme` and `sourceSlideIds: ["<sldId>#<creationId>"]`, after deleting the old slide, lands the slide exactly and keeps its ids. Inserted while the old slide is still there, it gets new ids. Each replacement took 75 to 140 ms.

Doing it safely needs more than replacing slides:

- PowerPoint has no tracked changes and no undo through the API. If a step fails halfway, or what lands isn't what was meant, the deck must be put back from what it was.
- The checkpoint must be of the deck PowerPoint holds afterwards, not of the engine's file: the history must describe what the person has.
- A version may have moved, added or deleted slides, so moves and inserts are needed as well as replacements.
- TypeScript never parses a deck (CLAUDE.md), so the engine must say exactly what PowerPoint is to do.

Office.js was checked on 2026-10-17:

- [PowerPoint.Slide](https://learn.microsoft.com/javascript/api/powerpoint/powerpoint.slide): `delete` (PowerPointApi 1.2), `moveTo(index)` (1.8);
- [InsertSlideOptions](https://learn.microsoft.com/javascript/api/powerpoint/powerpoint.insertslideoptions): without `targetSlideId`, slides go in at the start;
- there is no API for sections.

## Decision

### Two engine calls around one host step

1. **Prepare** (`lineage::decks::prepare_open`, wasm `prepareOpenDeck`). Runs "Update N versions" for this one version on the open deck's bytes, with the same rules: "Your edits before the update" first when there are unsaved edits; everything only the master changed is brought in; what both changed waits for **Keep mine** or **Use the master's**; anything by hand is listed, never written. It returns the version's line, the merged deck, and **slide steps** from the open deck to it. Nothing is recorded.
2. **The host carries out the steps** (`PowerPointHost.applySlideOps`), one `sync` each so a failure stops where it is, then reads the deck back with `getFileAsync`.
3. **Finish** (`finish_open`, `finishOpenDeck`). Runs the same update again on the bytes prepare had, checks that what PowerPoint holds is the merged deck (`pptx::check::landed`), and records a **checkpoint of what PowerPoint holds**, with the ids to set in it. The shell appends the history once.

The engine keeps nothing between the calls: finish works the merge out again from the same bytes and choices, and the engine is deterministic. The shell holds the bytes in between.

### Slide steps (`pptx::slides::slide_ops`)

Slides are named as Office.js names them, `"<sldId>#<creationId>"`. Both decks come from each other through edits that keep slide ids, so slides are paired by those two ids only.

1. Delete every slide the target lacks, and every slide whose new version differs (the slide's `full` key: shapes, notes, settings, animations).
2. Insert, in the target's order, each slide that isn't there, after the slide before it in the target (or first).
3. Move the slides that are out of order: those off the longest run already in order, each after the slide before it in the target.

Deletions come first, so a changed slide keeps its ids when it goes back in. The engine checks that the steps reach the target's order before returning them. A deck with a slide without a creation id can't be named to Office.js; its line says to close it and update it from the master deck.

### What counts as landed (`pptx::check::landed`)

The same slides, with the same ids, in the same order; the same sections; and no change between the two decks by the engine's keys (decision 0017), which leave out what PowerPoint renews on save. PowerPoint's own fixtures in `fixtures/pptx/powerpoint/` pass: a text edit, notes, a moved slide and a new slide made in PowerPoint count as having landed when the engine merges the same change.

### When it goes wrong

If a step fails, the deck isn't what was meant, or the history can't be written, the shell asks the engine for the steps from what PowerPoint now holds back to the deck as it was read, carries them out from that deck, sets the old ids back, and checks it. The panel then says "PowerPoint didn't take the changes exactly as Lineage made them, so this deck was put back as it was. Nothing was changed." (or "Lineage couldn't record the change in this deck's history, …"). If putting it back fails too: "… and putting this deck back didn't work either. Close it without saving to keep it as it was." The file on disk is only changed when the person saves.

### What PowerPoint for Windows showed (2026-10-18)

Checked on the invented pitch deck, with the pane driven through the DevTools port:

- Office.js names slides exactly as the engine does (`261#1002066859`).
- `moveTo(index)` leaves the slide at `index` in the final order, as the steps assume.
- A changed slide, deleted then inserted, keeps both ids; a new slide inserted from another deck keeps its own when they're free.
- One change took 1.06 s from the press to the panel's answer; an overlap with the person's unsaved edit 1.2 s; a new slide and a move 0.95 s (3.0 s in a pane just reloaded, which fetches the history first).
- A step forced to fail halfway (the new slide in, the move refused) and a forced mismatch (the new slide at the start) were both put back exactly, with the same ids and nothing recorded.

### A deck's updates need no offer

Word's master offers a change to its versions ("Offer it to …"), and a version sees only what was offered. A master deck has no such step: "Update N versions" offers and brings in with one press (decision 0018). So a tailored deck's panel and its updates list every pending master change, offered or not, or an open version would never see one. Found in the PowerPoint run.

### Keep mine on everything

When every choice keeps the version's side, no slide changes. The choices are still recorded (finish with the deck as it is), so the overlap isn't offered again.

### The panel

A tailored deck's panel shows **Review them** when PowerPoint has PowerPointApi 1.8 (for `moveTo`). The review lists what only the master changed (it all comes in), what both changed with **Keep mine** / **Use the master's**, and what stays to copy in by hand: design changes, a slide this deck has no layout for, SmartArt, media and animations (decision 0017). The button says what it does: "Bring in 2 changes". Without 1.8, the panel keeps 0018's text: close the deck and update it from the master deck.

## Alternatives

- **Record the engine's merged file**, as "Update N versions" does. Not chosen: the open deck is what the person keeps and saves, and PowerPoint writes it its own way. The history would hold a file nobody has.
- **Keep the merge in the engine between the two calls.** Fewer bytes across the boundary, but the engine would hold family state between calls, which it never does (decision 0010).
- **All steps in one `sync`.** Faster, but a failure would leave no way to tell how far it got. Putting back reads the deck again anyway, so one `sync` per step costs little (75 to 140 ms each in the spike).
- **Replace every slide.** Simpler steps, but each replacement is a chance for PowerPoint to change something, and it loses the person's place in the deck.
- **Precomputed undo steps.** Steps back from a state halfway can't be known in advance; reading the deck again and asking the engine covers any state.

## Consequences

- A tailored deck open in PowerPoint for Windows takes the master's changes from its own panel. The master deck's panel still updates closed versions, and still says "Close it, then update it again" for an open one.
- Typing in the deck during the second or two Bring in takes can make the check fail; the deck is then put back, and the typing on the changed slides with it.
- Sections can't be changed through Office.js. The merge never changes them, but a new or moved slide joins a section by where it lands; if PowerPoint places it differently from the engine, the check fails and the deck is put back.
- PowerPoint on the web and AutoSave (where the open deck saves itself as it changes) are the maintainer's run.
