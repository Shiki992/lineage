# 0018: How a deck's changes land, and what a sent deck leaves out

Date: 2026-10-17. Status: accepted.

## Context

In Word, a change from the master lands in the version that is open, as tracked changes by "Lineage" that the person accepts or rejects in Word (decision 0003). PowerPoint has no tracked changes, and Office.js can't replace an open presentation. It can only add slides from another file (`insertSlidesFromBase64`) and delete slides, so a deck needs another way in.

The founder journey (BLUEPRINT §5, screen 4) starts in the master deck. The founder changes Slide 6 · Traction, sees which tailored versions should get it, presses **Update 3 versions**, and later wants to see exactly what Pellow Ventures was sent.

The maintainer chose (2026-10-16):

1. **"Update N versions" writes the versions' files** from the master deck's panel, so the versions don't need to be open. Bringing changes into an open version through its own panel comes only if a spike shows slides can be replaced exactly.
2. **A sent deck leaves out hidden slides.** Speaker notes and the data behind charts stay.

Office.js was checked on 2026-10-16:

- [Insert slides](https://learn.microsoft.com/office/dev/add-ins/powerpoint/insert-slides-into-presentation) and [InsertSlideOptions](https://learn.microsoft.com/javascript/api/powerpoint/powerpoint.insertslideoptions): `sourceSlideIds` and `targetSlideId` take `"<id>#<creationId>"`;
- [PowerPoint.Slide](https://learn.microsoft.com/javascript/api/powerpoint/powerpoint.slide): `delete`;
- [PowerPoint add-ins](https://learn.microsoft.com/office/dev/add-ins/powerpoint/powerpoint-add-ins): `createPresentation(base64)`; no API opens an existing file;
- [Microsoft Graph errors](https://learn.microsoft.com/graph/errors): 423 Locked.

## Decision

### Update N versions (`lineage::decks::update_versions`)

From the master deck's panel, for the versions the person ticked:

1. **The master's edits become a checkpoint first**, named after what changed ("Changed Slide 6 · Traction"), as Save checkpoint would.
2. **Each version's file is read as stored**, with its version (OneDrive's eTag, or the dev folder's content hash).
3. **A version edited outside Lineage gets a checkpoint first**, "Your edits before the update", so the update never loses the person's edits and the history shows them.
4. **Changes are offered and brought in:**
   - every change that only the master made is brought in;
   - a change both sides made waits for the person's choice, **Keep mine** or **Use the master's**, on that version's line;
   - anything by hand (decision 0017) is listed, never written.
5. **The new file is written as direct edits** by the splice-only writer: shapes, notes and slides are replaced or added as parts, and the order and sections are rebuilt once. Before the file is used, it must reopen, and its parts and relationships must hold together.
6. **A checkpoint** "Brought in N changes from master v4" records the new file. The version's previous checkpoint is the way back.
7. **The shell replaces each file only if it is still the version it read.** The history is appended once, after every file. If a write or the append fails, every file already replaced is put back and nothing is recorded.

Each version gets one line in the report: "Brought in 2 changes", "Brought in 1 change · 1 to choose", "Already up to date", or "Not updated: …".

Further rules:

- **Unticked versions are left pending.** Nothing is recorded for them, and the panel keeps offering them the change ("Last sent 17 Oct, before this change"). Recording them as declined would be a one-line change if the maintainer prefers it.
- **A version that is open, where its file can't be changed, is left as it is.** PowerPoint for Windows locks a deck it has open; OneDrive answers 423 Locked. Storage says `FileLocked`, and the run goes again without that version. Its line says "Not updated: it's open, so Lineage couldn't change it. Close it, then update it again." Nothing is recorded for it. Found in the PowerPoint run.
- **Missing history is fetched for all versions at once.** A run that lacks blobs tries every version before asking for them, so the number of fetch rounds doesn't grow with the number of versions. The shell skips uploading blobs it has already seen in storage.

### The open deck (the spike)

The spike, run in PowerPoint for Windows with the Lineage pane on a throwaway copy of the invented pitch deck, replaced one slide with the version from another deck. For 10 cases it read the deck back through `getFileAsync` and compared it with the engine:

- a text and chart change;
- notes;
- a chart only;
- a hyperlink;
- a moved picture;
- a layout change;
- bold text;
- animations;
- a group;
- a new slide with a table.

Results:

- **The content lands exactly.** In every case, what changed in the deck is just the slide's change, and the deck's parts and relationships hold together. Each replacement took 75 to 140 ms (540 ms the first time).
- **Ids depend on the order.** If the new slide is inserted while the old one is still there, it gets a new id and creation id (for example `264#1780260602` in place of `261#1002066859`), and the next comparison has to match it by content. If the old slide is deleted first, both ids are kept. A new slide keeps its own ids when they're free.
- **PowerPoint re-saves what it reads back.** It rewrites the design parts and the chart markup, and adds row and column ids to tables. Decision 0017's keys ignore all of these, after the fix this spike prompted.

The **version panel's Bring in is not wired in this phase.** The spike shows it can work, but doing it safely needs more than replacing slides:

- the checkpoint after it must be of the deck PowerPoint actually holds, which needs a new engine command;
- there's no undo through the API, so a failure halfway needs putting back from the bytes before;
- a deck the version has rearranged needs moves and sections as well as slides.

Until then, a tailored deck's panel says to close it and choose Update versions in the master deck.

**Update (2026-10-18):** built, with all three, in [decision 0019](0019-bringing-changes-into-an-open-deck.md). A tailored deck open in PowerPoint (PowerPointApi 1.8) brings changes in from its own panel; without 1.8 the text above still shows.

### What a sent deck leaves out (`pptx::clean`)

A sent deck has:

- **no hidden slides** (removed as slide deletions, so relationships, notes and sections stay consistent);
- no comments, either kind (`ppt/comments`, `commentAuthors.xml`, `authors.xml`);
- no revision information, tags or `custDataLst`, and no add-in data;
- no thumbnail and no custom XML;
- empty core and app properties;
- only the two Lineage ids (or none, when the person turns them off).

Speaker notes and chart data workbooks stay. The panel says so: "Always a clean copy: no comments, hidden slides or document properties. Speaker notes stay."

### Open what they saw

The master panel shows the last copy sent, "Sent to Pellow Ventures on 17 Oct", with **Open what they saw**. It opens the stored copy, byte for byte, as a new presentation (`createPresentation`). The canvas's "Pellow replied" isn't shown: Lineage can't know about replies without access to email.

A file that came back is recognised by its ids, but reviewing a returned deck comes later. Phase 3's returned-file review is Word's.

## Alternatives

- **Write into the open version through slides** (the spike's path) as the main way: not chosen by the maintainer. It needs every version open, one at a time.
- **Replace the open deck's file in storage while it's open:** PowerPoint for Windows refuses (it holds a lock), and an open window would later save over the update anyway.
- **Copy the master's whole slide over the version's:** it would lose the version's own edits on that slide. Shapes are the unit instead (BLUEPRINT §4).
- **Keep hidden slides in sent copies:** they often hold appendix material that wasn't meant for that reader, and the maintainer chose to drop them.

## Consequences

- PowerPoint changes versions without tracked changes. The way back is the checkpoint before the update, through My documents' Restore as a new version.
- A version open somewhere when the master updates it is reported, and the person closes it and updates again. With OneDrive and co-authoring, whether PowerPoint holds a lock is part of the maintainer's web run (`docs/testing/powerpoint-founder.md`).
- If the open-deck path is built later, it should delete the old slide before inserting the new one, so the slide keeps its ids.
