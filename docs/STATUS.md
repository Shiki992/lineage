# Status

## Phase 3B: My documents (2026-10-16). Built and checked in a browser on invented test accounts. The Microsoft sign-in and Word runs are the maintainer's.

Branch `phase-3b-my-documents`, from `phase-3-returned-files` at `7e52eb5` (PR #5 merged). Not pushed.

Decisions:

- [0015 My documents](decisions/0015-my-documents.md): accounts, the library index, damaged histories, restore, milestones and relinking;
- [0016 dependencies](decisions/0016-phase-3b-dependencies.md).

Manual test script: [docs/testing/my-documents.md](testing/my-documents.md).

Before the work, the uncommitted edits were sorted with the maintainer:

- the Phase 3B prompt and BLUEPRINT screen 5 were kept, and the reverted content was restored (`97f4d98`);
- the 100 MB size check was kept as edited (`71c9869`; see Known issues).

**Done**

- **Engine** (`crates/lineage`):
  - `read::read_lenient`: as much of a damaged ledger as can be trusted, and what was lost;
  - `library::build_library`: name, format, master file, tailored versions, checkpoints, sent copies, files that came back and those still to review, last activity;
  - `index`: the rebuildable library index, `stale` and `refresh` (the ledgers win), `update`;
  - `graph::family_graph`: lanes, nodes and edges, including what each version brought in;
  - `views::document_page`;
  - commands `compare`, `restore_as_new_version`, `mark_milestone`, `relink`;
  - new records `MilestoneMarked` and `NodeRelinked`.
- **Boundary and shell:**
  - wasm methods with DTOs, valibot schemas and golden replies;
  - `Storage` gains `readIndex`, `writeIndex` and `ledgerVersions` in every adapter, in the harness's trait and in the conformance suite. OneDrive uses If-Match upload sessions, and `$batch` of up to 20 for eTags;
  - `History` (shared blob and index work) and `Library`;
  - the task pane updates the index after every append.
- **Harness:**
  - a button for each My documents action;
  - `accounts` (three families, damaged, large) and `tests/my_documents.rs`, the journey in memory and on disk;
  - `cargo xtask accounts <folder>`.
- **`apps/family-app`** (React, the design system):
  - sign-in (MSAL standard client, redirect bridge, app folder permission, the add-in's client ID), and sign-out that clears Lineage's cache only;
  - My documents with search, format filter, sorting and an empty state;
  - a document page with ARIA tabs:
    - Versions: compare any two, restore as a new version, milestones, "File not found" and relinking;
    - Sent copies: open exactly as sent;
    - Family tree: one tab stop, arrow keys along and between lanes, details and actions per point, archived behind a toggle.
  - Damaged histories and decks are shown, with nothing offered to change.
  - Development builds read the Word pane's dev folder, or `LINEAGE_DEV_STORAGE`.
- **Word pane:**
  - **My documents** in the footer (`openBrowserWindow`);
  - a master opened from a new address of the same kind is relinked.
- **Packages:** `@lineage/storage-local` (the dev folder, moved out of the add-in, now with a conformance run against a real server) and `@lineage/design` (the tokens and their contrast test).
- **CI:** a Playwright job on the runner's Chrome; My documents' JavaScript has its own budget.

**Checked**

- **Playwright, 12 runs on Microsoft Edge** (`pnpm --filter @lineage/family-app e2e`, invented accounts from `cargo xtask accounts`). Each account is served by its own dev server.
  1. **The library:** three families, newest first, after an out-of-date index (the deck missing, the letter at an old version); the index is rewritten. Search, format filter and sort.
  2. **Resume:** v4 to v1 with the milestone; archived behind the toggle.
  3. **Compare** v1 with v3: removed and added text marked.
  4. **Open as sent:** the download's SHA-256 equals the copy that left.
  5. **The family tree by keyboard:** one tab stop, Home, arrows between lanes, Enter for details.
  6. **The deck:** "Comes with PowerPoint support", nothing to compare, Open disabled.
  7. **"File not found":** relinked to the renamed file, and only that.
  8. **Restore:** a new version "Restored from Resume v1"; every other file's bytes unchanged.
  9. **Milestone** marked.
  10. **Damaged ledger:** the readable part, the note, no actions.
  11. **300 checkpoints and 50 copies:** the versions list in about 1.5 to 2 s from the click (reading the ledger, checking 5 files, drawing 300 rows); the 350-point tree in about 0.4 s.
  12. **Keyboard only:** list, tabs and tree, focus outlined.
- **The built-in browser pane** on a fresh three-family account: the list, Versions, compare, the tree and its details as screenshots, and phone width (no sideways scroll).
- **Audit** (`dev_folder.rs` with `LINEAGE_DEV_STORAGE`) of the account the Playwright run wrote to, after a restore, a relink and a milestone: 3 families (the deck skipped: its snapshots are invented and not stored), 12 checkpoints restore, 16 blobs match, 3 tailored versions on disk are their nodes' checkpoints.
- **LibreOffice 26.8.1 headless** opens the restored version and the relinked one.
- **The Word pane's own dev folder** lists its 5 families in My documents (counts only: some look like the maintainer's own documents).
- **Production builds:** My documents leaves out the dev-folder storage; both pages carry the CSP.

**Tests:**

- **Rust:** 272 pass, 3 ignored (was 244). New tests:
  - read, index (with a proptest: any damage to the index refreshes to a rebuild), graph, library and the page;
  - compare, restore, milestones, relink;
  - the boundary, golden replies;
  - the harness journey, damaged and large.
- **TypeScript:** 218 pass (was 160):
  - shell 112, OneDrive 26, storage-local 7, design 15;
  - add-in 32;
  - My documents 26 (every screen state: loading, empty, normal, missing file, damaged, error);
  - plus 12 Playwright runs.
- **All checks pass:** fmt, clippy, cargo-deny, licence headers, fixtures, lint, typecheck, Prettier and budgets.

**Bug found and fixed**

- **Restoring with a fresh engine ran out of fetch rounds** on a master with several checkpoints (found by Playwright). `Library` now fetches manifests and change lists first, as the task pane does, and allows a round per snapshot (with a test).

**Budgets:**

- wasm 1,159,791 bytes raw and 388,922 gzipped (was 1,015,022 and 354,113): 31 KB under the 420 KB gzip budget.
- JavaScript: task pane 159,861 gzipped, My documents 157,172.

**Not done**

- **Signing in with a Microsoft account, and OneDrive.** These need the maintainer's Entra registration, plus the redirect URI `http://localhost:3001/redirect.html` (`docs/setup/microsoft-app-registration.md`, step 3b). That is Part B of the test script. Not yet checked:
  - MSAL's redirect bridge against the real login page;
  - `$batch` eTags matching single requests;
  - personal OneDrive's download host names (the CSP allows `*.1drv.com` and `*.microsoftpersonalcontent.com`).
- **In Word:** the footer link and relinking a renamed master.
  - Word was open with the maintainer's own document, and Word must be started afresh, with remote debugging, to drive the pane.
  - Both are covered by jsdom and shell tests.
- **A real deck family.** Decks can be made only from Phase 4. The deck in the test account is an invented history.
- **Production hosting** of My documents, like the add-in's.

**Known issues**

- **The open-document size check is 100 MB** (`71c9869`, the maintainer's choice), but Microsoft's `Word.Document` reference says `insertFileFromBase64` takes at most 4 MB. Between 4 and 100 MB, Word itself may refuse the write. That hasn't been tried. Decision 0008 and the Phase 2 known issue still say 4 MB.
- **Only a master's same-kind move is followed.** Word on the web and Word for Windows report one file with different addresses, so moving the master and then opening it only on the web isn't recorded. The page still shows the old address.
- **"Open as sent" downloads the copy;** it doesn't open in Word on the web.
- **The tree is one column per event,** so a family with hundreds of events scrolls sideways (350 points: about 20,000 px).
- **CLAUDE.md's architecture list** still calls `apps/family-app` enterprise-only, and doesn't name `packages/storage-local` or `packages/design`. Left for the maintainer.

## Phase 3 follow-up: keep only the words for files saved by other apps (2026-10-15, clicked through in Word 2026-10-16). Built, checked with Word's accept/reject comparison and LibreOffice, and clicked through in the Word pane with real LibreOffice re-saves. Real Google Docs downloads are fixtures.

Branch `returned-words-only`, from `phase-3-returned-files` at `69194fc`. Not pushed. Decision: [0014 Keeping only the words of a file another app saved](decisions/0014-words-only.md). Decision 0012's consequences updated.

**Done**

- **Detection** (`crates/core/src/origin.rs`). A returned file's formatting isn't trusted when:
  - LibreOffice (or another app Lineage knows) saved it, by the `Application` in `docProps/app.xml`;
  - it has no `app.xml` though the copy that left had one (a Google Docs download);
  - or at least half the paragraphs whose text came back unchanged were reformatted.

  Saved by Word with a few reformatted paragraphs: trusted.
  - All six real LibreOffice re-saves are flagged by name.
  - All six real Google Docs downloads (`fixtures/docx/returned/google/`) and the imitation are flagged as exports. In the real downloads, every paragraph whose text came back unchanged is reformatted too (8 of 8 to 14 of 14).
  - The Word restyle (testkit) is flagged as formatting changed almost everywhere.
  - Word edits are not flagged.

- **Words-only writing** (`write::apply_with(.., Bring::WordsOnly)`):
  - an edited paragraph keeps its formatting, and added words take a neighbouring run's;
  - a new paragraph takes the formatting the nearest paragraph of theirs formatted alike has in the master: style, list, `numId`, and run formatting mapped run by run;
  - a moved paragraph keeps its own;
  - formatting-only changes are still written whole;
  - `apply` and every existing output are unchanged.
- **Review** (`returned::review`): `formatting` says whether only the words come in, and why.
  - Example: "This file was saved in LibreOffice, which changes fonts and spacing. Lineage brings in their words in your formatting."
  - `decide` takes the person's choice. No ledger change.
- **Boundary and shell:**
  - wasm `decideReturned(.., wordsOnly, nowMs)` and `ReturnReview.formatting`;
  - golden replies, including a new words-only review;
  - `Lineage.decideReturned(returned, cards, { wordsOnly })`;
  - the harness `keep_and_skip` uses the review's default.
- **Word pane:**
  - the review shows the note at the top;
  - it draws their reformatting plainly on the cards;
  - the formatting step reads "The formatting changed in N places", with the app's sentence;
  - **Bring in their formatting too** (a checkbox, unticked) sits above Apply.

**Checked**

- **Clicked through in the Word pane** (step 20 of `docs/testing/word-personal.md`). Word 16.0.20430, dev-folder storage, the pane driven through the DevTools protocol, no typing.
  - A fresh master, "Words-only resume", sent three copies with Lineage IDs.
  - Each came back edited and re-saved by LibreOffice 26.8.1.1, and was brought back with **A file came back**. Each was recognised by its Lineage ID, and its review opened with "This file was saved in LibreOffice, which changes fonts and spacing. Lineage brings in their words in your formatting."
  1. **Northgate Analytics, box unticked.** Kept 3 changes (two word changes and a new bullet).
     - Word lists only Added and Deleted revisions, all by Lineage, and no formatting revisions.
     - Track Changes is back off.
     - The new bullet is List Paragraph in Calibri 11, like its neighbours.
     - v2 "Kept 3 changes from Northgate Analytics".
  2. **Harbour Bank, box ticked.**
     - The formatting step read "The formatting changed in 2 places. LibreOffice changes formatting when it saves a file, so these are probably not changes Harbour Bank made."
     - Once the box was ticked, the summary read "Their formatting will come with their words, as you chose." Apply kept 3 (v3).
     - LibreOffice's formatting on those paragraphs was only an explicit Normal style and empty run properties, so the result looks the same as words only.
  3. **Eastwood Credit, box ticked, with a new bullet they set in italics.** The bullet came in italic (their formatting), in the master's list style and font, added by Lineage (v5).
     - Their word change sat in a paragraph that still held Northgate's tracked change.
     - Its card offered only Skip: "This part of your version has tracked changes. Accept or reject them in Word first." That is as designed.
  - **Audit** (`dev_folder.rs`): 1 family, 5 checkpoints restore, 9 blobs match, the 3 sent copies on disk match the record, 3 files came back, 3 reviews.
  - LibreOffice headless opens the final master.

- **Real Google Docs downloads** (`fixtures/docx/returned/google/`): the six LibreOffice inputs, uploaded to Google Drive as Google Docs and downloaded as Word.
  - The four edited ones, kept words only, look exactly like a Word save of the same words, with no `w:rtl` or `w14` from Google.
  - A real download is reviewed words only, end to end (`returned/tests.rs`).
  - They join the recognition report: 135 returned files, precision 100% (50 right, 0 wrong), automatic recall 80.6% (50 of 62), found 100% (99 of 99).
  - Google keeps custom properties, so Lineage IDs survive a download.
- **`scripts/word-smoke.ps1 -Compare`** (Word 16.0.20430) on all 38 write-test outputs, 18 of them words-only: every file opens, and Word's accept-all and reject-all match the engine.
- **Word, asked directly** after accept-all on every words-only output:
  - every word is in the master's font (Calibri, never the export's Arial);
  - every revision is by Lineage;
  - the only formatting revisions are the deliberate bold and the last paragraph mark at the end of the document.
- **LibreOffice 26.8.1.1 headless** opens all 38 outputs.
- **Round trips** (accept gives ours plus their words, reject gives ours):
  - the Google imitation (light, heavy, moved, three end-of-document cases);
  - all four edited LibreOffice re-saves, and the four edited real Google downloads;
  - overlaps (both edited, edited vs deleted, both added);
  - a table cell, the neighbour rules, the fallback, and a kept formatting change.

  The Google and LibreOffice outputs match, character by character, a Word save of the same words.

- **New proptest** (64 cases in CI, 4,096 run locally): keeping the words of another app's file gives all their words, rejecting gives the base, and no paragraph that was already there gets any of the app's formatting.

**Tests:**

- **Rust:** 244 pass, 3 ignored (as before).
- **TypeScript:** 160 pass (shell 93, OneDrive 21, add-in 46).
- Lint, typecheck, formatting, cargo-deny, licence headers and fixture checks pass.

**Budgets:**

- wasm 1,015,022 bytes raw and 354,113 gzipped (was 984,711 and 345,574);
- JavaScript 158,562 gzipped.

**Not done**

- **Word on the web and OneDrive.** Step 20 ran in Word for Windows with the dev folder. The rest is the maintainer's run, as for Phase 3.
- **A Google Docs download brought back in the pane.** Real downloads are covered by the engine and review tests; the pane run used LibreOffice files.

**Known issues**

- **An explicit Normal style reads as a formatting change.** Found in the Word run.
  - Word leaves Normal paragraphs with no style. LibreOffice writes `Normal` out, and `read_para_props` compares style ids as written.
  - So a card read "'SQL, Python, …': Normal changed to Normal".
  - It is probably also behind the "Both changed … one the words, one the formatting" overlaps on a master made in Word.
  - Follow-up: treat the default paragraph style as no style when comparing.

- **Words they formatted on purpose lose it in words-only mode** (an italic title added to a re-saved file). "Bring in their formatting too" keeps it.
- **Their own formatting is kept for:**
  - links they added;
  - inserted rows and whole tables;
  - a new paragraph with nothing formatted alike in theirs (with its own list id, as before).
- **Restyled in Word is flagged too:** "formatting changed almost everywhere" also fires when a person restyled the whole file in Word. The review says so; the box brings theirs.
- **Formatting noise against your own edits is still an overlap.** Where their side of an overlap is formatting only (LibreOffice's noise against the master's own edit), it is still offered as an overlap. Follow-up.

## Phase 3: files that come back (2026-10-15). Built and checked in Word for Windows with the dev folder; OneDrive and Word on the web are the maintainer's run.

Branch `phase-3-returned-files`, from `phase-2-word-addin` at `5118fd2`. Not pushed. Decisions:

- [0012 Files that come back](decisions/0012-returned-files.md)
- [0013 When a file without ids is recognised](decisions/0013-fingerprint-threshold.md) (BLUEPRINT open decision 3)

Manual test script: [docs/testing/word-personal.md](testing/word-personal.md), steps 12 to 19.

**Done**

- **Engine** (`crates/lineage/src/returned.rs`):
  - **Recognition across every family in storage.**
    - By ids first.
    - Otherwise by content: stored sketches shortlist the checkpoints, then each candidate is compared word by word with what actually left (the sent copy, or the snapshot).
    - Automatic only when one recorded copy clearly fits (decision 0013); otherwise "Which copy is this?", never a silent guess.
  - **Recording:** the file is kept exactly as it came back, deduplicated by hash.
  - **Review:** only the outside person's changes (base = what left, judged against the open file now), so a master that moved on is fine. Where both changed a paragraph, it is an overlap with both sides.
  - **Keep and skip:**
    - kept changes are written as tracked changes by Lineage, with the same read-back check as `decide`;
    - a checkpoint "Kept N changes from Mentor" is saved;
    - each sitting is recorded, and a review can be finished later.
  - **What I've sent chips:** Came back with N changes, formatting changes only, unchanged, Partly reviewed, Reviewed. The master and version panels list files that came back.
- **Sent copies carry their own id** in `LineageCheckpointId` (decision 0012). A returned copy names exactly whose copy it is, and can never be saved as a checkpoint of its master.
- **Core:**
  - `view::from_plan_for` words cards for a returned file: Keep / Skip / Keep mine / Use theirs, "You and Mentor both changed …";
  - `identity::content_match` (changed words and a percentage).
- **Harness:** five new buttons on `Panel`, and `tests/feedback.rs`, the BLUEPRINT "Asking for feedback" journey, in memory and on disk:
  - send to a mentor, and the master moves on;
  - the copy comes back renamed and edited, and is recognised by ids, "Came back with 4 changes";
  - keep two, skip one, use their side of an overlap; tracked changes by Lineage, checkpoint, "Reviewed".
  - Also: a Google export without ids (certain by content), two copies of one checkpoint (a choice), and a stranger's file (not recognised).
- **wasm and shell:**
  - `recognise` (every ledger as text), `recordReturned`, `returnedReview`, `decideReturned`, `openReturned`;
  - `openFile` resolves sent-copy ids; missing-blob errors name their family;
  - golden replies for each, valibot schemas;
  - `Lineage` methods mirroring the harness. `open()` returns "cameBack" for a returned or older copy.
- **Word pane:**
  - "A file came back" (file picker and drop zone) on the panels and in What I've sent;
  - "Which copy is this?";
  - the review one change at a time ("Change 2 of 5", Keep / Skip, Decide later), then one step for formatting ("Skip all formatting"), and Apply;
  - returned call-outs, chips and Review buttons;
  - "This file came back" when the returned copy itself is opened;
  - "This looks like a copy that came back" above Start tracking for an untracked copy.
- **Fixtures:**
  - testkit builders for light and heavy edits, an **imitation** Google Docs export, heavy reformatting, a cover letter, meeting notes and a stranger's resume;
  - six **real LibreOffice 26.8.1** re-saves in `fixtures/docx/returned/libreoffice/` (`scripts/libreoffice-resave.ps1`). LibreOffice keeps custom properties, so ids survive it.
- **Recognition report** (`crates/lineage/tests/recognition.rs`, 126 returned files, runs in CI):
  - **precision of automatic matches 100%** (44 right, 0 wrong, none on the 36 files never sent);
  - **automatic recall 80%** where no other copy has nearly the same text: every unedited or lightly edited copy is automatic, every heavy rewrite is a choice;
  - **found (automatic or among the choices) 100%** (90 of 90).
  - Table in decision 0013.

**Checked in Word** (Word 16.0.20430, Windows 11, dev-folder storage, Word started with remote debugging for that launch and the panes driven through the DevTools protocol; Office.js for every edit, no typing):

1. Started tracking a fresh synthetic master. Its untracked panel first said "This looks like a copy you sent that came back". That is right: the Phase 2 family in the same folder had sent copies of the same synthetic text.
2. Sent a copy to "Mentor" (id on) and one to "Northgate Analytics" (id off); changed the master (v2).
3. **The mentor's copy, opened in Word under a new name:** "This came back from Mentor", recognised by its Lineage ID, not the master's panel.
   - Edited it with Track Changes on, plus a comment, and saved; "Review their changes" kept it; "Open 'Feedback resume'" opened the master (after Word's own confirmation).
4. **In the master:**
   - "Mentor sent back your copy from 15 Oct · 4 changes" › Review: one change at a time. Kept two, skipped one, and used the mentor's side of the summary both had changed.
   - Word's tracked-changes API lists only **Lineage** revisions; Track Changes is back off; the master's own edit stayed; the skipped change wasn't written. v3 "Kept 3 changes from Mentor".
5. **The Northgate copy (no id):**
   - opened in Word, its panel said it looks like a copy that came back, and "Which copy is this?" listed the two copies sent from v1 and two in the Phase 2 family;
   - edited, saved and re-saved by LibreOffice;
   - in the master, **A file came back** with the file set on the file input took it as a choice, Northgate was picked, and the two real edits were kept. LibreOffice's re-save added five noise items (two formatting-only, two formatting-vs-words overlaps offering "Keep mine", one page-layout change), all skipped. v4 "Kept 2 changes from Northgate Analytics".
6. What I've sent: both copies "Reviewed".
7. **Audit** (`dev_folder.rs`): 3 families, 11 checkpoints restore, every blob matches, 4 sent copies on disk match, 2 files came back, 2 reviews.
   - `word-smoke.ps1 -Compare` on both review checkpoints: Word's accept-all and reject-all match the engine.
8. **LibreOffice headless** (now installed, CLAUDE.md's smoke test): all 38 synthetic fixtures, all 58 engine outputs and both review checkpoints open; Word's comparison over the 58 found no differences.

**Timings in Word** (dev log):

- recognising a chosen file 0.4 s with two families in storage;
- the background check on an untracked file 0.13 to 0.5 s;
- recording 0.1 to 0.3 s; review 0.2 to 0.3 s;
- keep and apply 1.1 to 1.3 s (Word's write, read-back and save included, like bringing changes in).

**Budgets** (decision 0010): all inside.

- wasm 984,711 bytes raw and 345,574 gzipped (was 907,598 and 324,081);
- JavaScript 158,261 gzipped.

**Bugs found and fixed** (each with a test):

- **A Google Docs export's paragraphs carry `w14:paraId`.** A version that doesn't declare `w14` had every change marked "copy by hand", though the writer never copies theirs' paragraph tag (core).
- **Content match by whole percentages couldn't separate copies a word apart.** Decisions now use changed words (found by the recognition report).
- **"Review it as a file that came back" did nothing on an untracked file** (screen order), found in Word.
- **A file recorded from another window showed on the master's panel only after reopening the panel.** It now refreshes when it regains focus. Found in Word; the focus refresh itself is untested in Word.

**Tests:**

- **Rust:** 224 pass, 3 ignored (local fidelity, dev-folder audit, Word read-back).
- **TypeScript:** 156 pass (shell 91, OneDrive 21, add-in 44).
- Lint, typecheck, formatting, cargo-deny, licence headers and fixture checks pass.

**Not done**

- **The maintainer's run** on OneDrive and Word on the web (steps 12 to 19 added to the manual script). It is still waiting on the Entra app registration from Phase 2.
- **Google Docs exports were imitations.** Real downloads are now in `fixtures/docx/returned/google/` and in the report (follow-up above).
- **Not checked in Word:**
  - dropping a file on the pane: the jsdom test covers the handler, but in Word only the file picker was used (through `DOM.setFileInputFiles`);
  - "Open what came back";
  - the focus refresh.

**Known issues**

- **Comments a mentor leaves are not shown in the review.** "Open what came back" opens the file exactly as it came.
- **Kept changes bring their paragraph's formatting.** From a Google Docs export that means Arial and Google's spacing, as tracked formatting changes the person can reject in Word. Addressed in the follow-up above (decision 0014): files saved by other apps bring only their words.
- **LibreOffice and Google re-saves add formatting-only items,** and formatting-vs-words overlaps where the master also changed a paragraph. They are skippable (one "Skip all formatting" step), but noisy.
- **"Use theirs" on an overlap takes their whole paragraph,** including undoing the master's own edit there. That is the paragraph rule in BLUEPRINT §4, and the card shows both sides.
- **The master itself, attached by hand,** that comes back while the master is still at that checkpoint opens as the master (decision 0012).
- **The background check flags any untracked file** whose text matches something already sent, including a fresh copy of a tracked file. Start tracking stays available.
- **Word opens email attachments in Protected View,** where add-ins don't run; Enable Editing first.

## Phase 2: Word add-in for personal users (2026-10-15). Built and checked in Word for Windows; the OneDrive and Word-on-the-web run is the maintainer's.

Branch `phase-2-word-addin`, from `phase-1-word-core` at `125f18d`. Not pushed. Decisions:

- [0008 Word add-in platform](decisions/0008-word-addin-platform.md)
- [0009 OneDrive layout](decisions/0009-onedrive-layout.md)
- [0010 wasm boundary and budgets](decisions/0010-wasm-boundary-and-budgets.md)
- [0011 dependencies](decisions/0011-phase-2-dependencies.md)

Manual test script: [docs/testing/word-personal.md](testing/word-personal.md).

**Done**

- **Engine boundary** (`crates/wasm`):
  - `Engine` exposes every operation the personal screens need, each a plain Rust function in `api.rs` with native tests;
  - inputs refuse unknown fields and check ids, hashes and times;
  - a fetched-blob cache reports missing blobs a snapshot at a time;
  - golden replies (`packages/shell/test/golden/`) keep Rust and TypeScript in step.
- **Engine additions:**
  - `check::written` (did a write land in Word as written);
  - `SeededIds`;
  - `decide_cards` (the harness uses it too);
  - `blob::referenced` and `manifest_parts`.
- **`packages/shell`:**
  - valibot schemas for every value crossing the boundary;
  - the typed `Engine`;
  - `Storage` mirroring the harness trait, with `MemoryStorage` and a conformance suite;
  - `DocumentHost`;
  - `Lineage`, the TypeScript twin of `harness::Panel`. It writes the document before appending to the ledger, reads every write back and puts the original back if it didn't land, and on a conflicting append undoes and runs again.
- **`packages/storage-onedrive`:**
  - Graph with token refresh and `Retry-After`;
  - ledger appends through upload sessions with `If-Match`;
  - downloads through `downloadUrl`;
  - both layouts of BLUEPRINT open decision 1, tested against a fake Graph.
- **`apps/office-addin`:** an add-in-only manifest with a Home-tab button, and every screen from the canvas "Personal users" page:
  - master panel, tailored version panel;
  - updates with − and + marks, bring in or decline, overlaps with both sides, by-hand reasons;
  - save checkpoint, make a tailored version;
  - send a copy (Word only, "PDF is coming later", the Lineage ID switch, the clean-copy note);
  - what I've sent.

  Also: sign-in through nested app authentication, the Word host through Office.js, and dev-only local-folder storage, timings and a diagnostics screen.

- **Development scripts:** `dev-cert.ps1` (run by the maintainer) and `sideload-word.ps1` replace Microsoft's two CLI tools; `make-icons.mjs` draws the icons; `check-budgets.mjs` runs in CI.

**Checked in Word** (Word 16.0.20430, Current Channel, Windows 11; history in the dev folder):

- **Write spike** (decision 0008): all 57 of the engine's Word smoke-test outputs went into an open document through `insertFileFromBase64` and were read back. 57 of 57 landed exactly, with every Lineage revision still by "Lineage". The first run found the check too strict about empty note parts (fixed).
- **The Phase 2 scenario, clicked through in the pane** on the synthetic resume:
  1. Start tracking; make "Resume – data roles" and "Resume – platform roles".
  2. Add a job to the master in Word; what changed shows both paragraphs marked +; save v2; offer to both versions.
  3. Open data roles from the panel and bring both changes in. Word's own tracked-changes API lists one insertion by **Lineage**, and Track Changes is back off.
  4. Open platform roles and decline both; nothing is written.
  5. Send a copy to Northgate Analytics (with the id) and one to a mentor (without it).
  6. Open what Northgate received: a new, unsaved document with no revisions or comments.
- **Ids:** written through Office.js custom properties, they are read by the engine from Word's bytes.
- **Audit with the Rust engine alone** (`crates/harness/tests/dev_folder.rs`): 2 families, 7 checkpoints restore, every blob matches, both versions on disk are checkpoints of their nodes, and both sent copies match the record.
- **`scripts/word-smoke.ps1 -Compare` on both versions:** accept all and reject all match the engine.

**Budgets** (decision 0010; the first three fail CI):

|                                | Budget                   | Measured                                                                                                                                      |
| ------------------------------ | ------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------- |
| wasm, raw                      | 1.5 MB                   | 907,598 bytes                                                                                                                                 |
| wasm, gzipped                  | 420 KB                   | 324,081 bytes                                                                                                                                 |
| JavaScript, gzipped            | 250 KB                   | 154,272 bytes                                                                                                                                 |
| Engine ready in Word           | 500 ms cold, 200 ms warm | 15 loads: 30 to 90 ms warm, up to 251 ms on the first load after Word starts                                                                  |
| Panel ready, local storage     | 800 ms                   | median 496 ms; 2 of 15 over (816 and 867 ms), both in Word's first seconds after starting                                                     |
| One engine command on a resume | 300 ms                   | 1 to 52 ms on the synthetic resume. On the maintainer's 44 documents under 100 KB: start tracking at most 249 ms, what changed at most 144 ms |

- **Word's own work comes on top:** reading the document takes about 0.2 to 0.6 s, and bringing changes in about 1.1 s (write, read back, check, save).
- **`wasm-opt` was measured and dropped:** 9.4% smaller raw but 7% larger gzipped.

**Bugs found and fixed** (each with a regression test):

- **Start tracking failed on 40 of the maintainer's 45 documents.** Their custom properties part is an empty `<Properties/>`, and adding two properties expanded the tag twice. The local fidelity run now writes and rereads the ids on every document: 45 of 45.
- **Any file that had been through Word looked changed.** Word re-saves files in its own way, so sending a copy straight after bringing changes in made an empty checkpoint. "Changed" now means the engine finds a change in a story, in either view.
- **The write check was too strict.** Word keeps footnote and endnote parts with only separators once a document has had notes, and the check counted those empty stories.
- **A new section break was summarised as "Added a blank line".**
- **By-hand cards had no buttons**, so such a change could never stop being offered. They now offer Decline.

**Tests:**

- **Rust:** 200 pass, 3 ignored (local fidelity, dev-folder audit, Word read-back).
- **TypeScript:** 127 pass (shell 75, OneDrive 21, add-in 31).
- Lint, typecheck, formatting, licence headers and fixture checks pass.

**Not done**

- **Sign-in and OneDrive in a real account, and Word on the web.** They need the maintainer's app registration (`docs/setup/microsoft-app-registration.md`); the steps are in `docs/testing/word-personal.md`. Until then, OneDrive is tested against the fake Graph only. Still open:
  - whether OneDrive re-checks `If-Match` when an upload session finishes (the diagnostics screen has a button for it);
  - the real-account comparison of the two layouts.
- **PDF copies:** skipped in v0 (decision 0008).
- **No sign-in fallback** for Office without nested app authentication: the panel says to update Office.
- **No blob cache across sessions:** start-up didn't need one.
- **Store listing and production hosting:** out of scope.

**Known issues**

- Files over 4 MB can't be changed while open (an `insertFileFromBase64` limit); the panel says so.
- Word asks the person to confirm whenever the add-in opens a file ("Open" on a version).
- After Word restarts, the Lineage button sometimes appears only after **Home › Add-ins › Developer Add-ins › Lineage** has been used once. Seen with the sideloaded development manifest.
- In the Word run, the bullet under a new job was placed "Under 'Experience'" rather than under the new job's heading. The wording is minor, but it differs from the synthetic case.
- A version's storage ref is its path. Renaming or moving it in OneDrive loses the link until Phase 3 can find it by its ids.
- Some screen captures show WebView2 task panes as blank. For automated checks, start Word with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` for that one launch and drive the pane through the DevTools protocol, then close that Word.
- With the organisation's sensitivity labels, Word may mark a document changed on opening ("Confidential*"). That is not Lineage, but it makes Word ask to save.

## Phase 1: Word engine and family logic (2026-10-15). Done.

Branch `phase-1-word-core`, from `phase-0-foundations`. Not pushed. Decisions: [0005 package and XML](decisions/0005-docx-package-and-xml.md), [0006 dependencies](decisions/0006-phase-1-dependencies.md), [0007 merging and clean copies](decisions/0007-merge-and-clean-copies.md).

**Done**

- **Package layer** (`crates/core/src/package`, `xml`):
  - rawzip reader with size, name, CRC and compression checks;
  - miniz_oxide inflate capped at the declared size;
  - our own writer, which copies unchanged entries as their original compressed bytes.
  - XML is parsed with roxmltree and changed only by byte-range splices, so untouched XML stays byte-identical.
  - The writer re-parses every part it changed and refuses to return malformed XML.
- **Canonical model** (`docx/`):
  - stories (body, headers and footers by section, footnotes, endnotes) → blocks (paragraphs, tables with rows and cells, section breaks, opaque) → text, atoms and markers, with byte spans;
  - styles and numbering referenced, not flattened;
  - accepted and rejected views of existing tracked changes, matching Word (checked: whitespace without `xml:space`, `w:cr`, text boxes, field results, paragraphs before tables).
- **What changed** (`diff/`):
  - patience LCS on content-and-formatting keys, with ties treated as one changed line so repeated lines pair with their counterparts;
  - formatting-only pairs, moves, similarity pairing (whole-percentage Dice), table rows and cells;
  - a word-level diff inside paragraphs;
  - the v0 change list, with a new optional `cell` location (schema, examples, Rust and TypeScript tests updated).
- **Three-way merge** (`merge/`): clean, already there, overlap (both edited, deleted vs edited, edited vs deleted, both added here, formatting vs text) or by hand, per paragraph, cell, row, section break or opaque block.
- **Tracked-change writer** (`write/`):
  - `w:ins`, `w:del`, `w:rPrChange` and `w:pPrChange`, row and table insertions and deletions, and new external hyperlinks, all by "Lineage";
  - moves shown as a deletion plus an insertion;
  - `Check` says in advance what can be written.
  - **No empty paragraph is ever left** at the end of the document or a cell, or before a table: the last paragraph mark changes hands Word's way (`write/ends.rs`), so accepting gives the master's version and rejecting gives the original, exactly.
- **Custom properties and clean copies** (`docx/props.rs`, `clean.rs`), **summaries** (`summary.rs`) and the **what-changed view model** (`view.rs`).
- **Family logic** (`crates/lineage`):
  - JSON Lines ledger with validated ids and hashes, and a `FamilyState` fold;
  - content-addressed snapshots;
  - change states and the merge base (decision 0002), with detection of text typed in by hand and of Lineage changes rejected in Word;
  - fingerprints, pure commands for every button, panel views.
- **Harness** (`crates/harness`): memory and local-folder storage, and `Panel` with one method per button. **The Phase 1 "done when" scenario passes end to end** in memory and on disk (`tests/personal_resume.rs`), plus 8 more scenarios.
- **Fixtures:** 38 synthetic .docx files (`crates/testkit`, moved out of xtask; the Phase 0 ten are byte-identical). All open in Word.
- **Tests:** 177 Rust tests pass, plus 18 TypeScript tests.
  - The 7 property tests pass at 10,000 cases each. They found three bugs, now fixed with regression tests: crossing pairs, repeated-line pairing, and the end of the document.
- **Word check** (`scripts/word-smoke.ps1 -Compare`):
  - All 57 engine outputs on fixtures (writer outputs and clean copies) open in Word.
  - The 54 that can be compared match Word's accept-all and reject-all text exactly. Files with DATE fields, maths or symbols are opened but not compared, because Word shows those in its own way.
  - Paragraph styles after accept and reject were checked for the end-of-document cases.
- **Your documents** (`tests/local_fidelity.rs`, local only, counts only), 45 files:
  - every file reads, writes back with every part and entry identical, opens in both views and cleans to the same text;
  - for all 44 neighbouring pairs, bringing every change in reproduces the other file and rejecting gives the original back;
  - all 89 outputs open in Word, and the 44 merges match Word's accept-all and reject-all text.
  - This run found two real bugs (self-closing empty paragraphs, paragraphs before tables), now fixed with regression tests. 60 changes between neighbours were left to bring in by hand (mostly section breaks and table layout).
- **Fuzzing:** `fuzz/` with `docx_reader` and `zip_reader`, and a CI job on a pinned nightly.
  - Not run locally: on Windows, the AddressSanitizer runtime in Visual Studio does not match nightly Rust's, and WSL has no Rust installed.
  - `the_reader_never_panics_on_damaged_files` covers damaged input on stable in every test run.

**Wasm size** (release, `opt-level = "s"`, LTO, before wasm-opt; measured with a temporary export that reaches the whole engine, then removed):

- 1,296,730 bytes raw, 357,725 bytes gzipped, plus about 6.8 KB of JS glue;
- compiles in about 1 ms in Node (lazy tiering).

The committed wasm crate still exports only the Phase 0 bridge (21,855 bytes). Task-pane budgets are set in Phase 2.

**Known issues**

- Brought in by hand for now (each shown with its reason; see 0007):
  - footnote and endnote changes;
  - section breaks;
  - table columns, merged cells and table formatting;
  - new pictures or other new parts;
  - content in tables of contents and block content controls;
  - paragraphs that already have unaccepted tracked changes.
- UTF-16 parts are read-only.
- Fuzzing has only run in CI, not locally.
- A sent copy drops the document title with the other metadata. Make it an option if people want titles kept.

## Phase 0: repository foundations (2026-10-15). Done locally; CI result below.

Branch `phase-0-foundations`. Decisions: [0004-stack](decisions/0004-stack.md).

**Done**

- **Cargo workspace:** `crates/core`, `crates/lineage`, `crates/wasm`, `crates/harness` and `xtask`.
  - Rust 1.99.0 pinned; clippy warnings are errors in CI.
  - `forbid(unsafe_code)` in core, lineage, harness and xtask.
- **pnpm workspace:** TypeScript 6.0.3 (strict), ESLint 10 with `strictTypeChecked`, Prettier and Vitest 5. `@lineage/shell` wraps the engine.
- **Bridge proven:** `engineVersion()` and `echoBytes()` go Rust → wasm → TypeScript.
  - Byte buffers up to 1 MB round-trip unchanged (`packages/shell/src/bridge.test.ts`).
  - `scripts/build-wasm.mjs` checks the wasm-bindgen CLI matches `Cargo.lock`.
- **`spec/format/v0`:** change-list JSON Schema (draft 2020-12), with 4 valid and 6 invalid examples. Tested in Rust (boon) and TypeScript (ajv).
- **Fixtures:** `cargo xtask fixtures` generates 10 deterministic .docx files with invented content into `fixtures/docx/synthetic/`.
  - All ten open in Microsoft Word with the expected comments, footnotes, revisions, sections, image, fields and table (`scripts/word-smoke.ps1`).
  - `cargo xtask check-fixtures` catches drift, and `cargo xtask check-headers` catches missing licence headers. Both were confirmed to fail on bad input.
- **Community docs:** README, CONTRIBUTING (DCO, Conventional Commits, fixtures), CODE_OF_CONDUCT (Contributor Covenant 2.1), SECURITY (GitHub private reporting), GOVERNANCE, NOTICE. Placeholder READMEs for later packages and apps.
- **CI** (`.github/workflows/ci.yml`):
  - Rust on Linux and Windows: fmt, clippy, tests, header and fixture checks.
  - cargo-deny.
  - wasm build and TypeScript lint, typecheck, tests and format.
  - DCO check on pull requests.
- **Local results:** cargo fmt, clippy (`-D warnings`), `cargo test --workspace` (20 tests), `cargo deny check`, both xtask checks, and `pnpm lint`, `typecheck`, `test` (16 tests) and `format:check` all pass.
- **Clean clone** of the pushed branch (Windows): `cargo test --workspace --locked`, both xtask checks, and `pnpm install --frozen-lockfile`, `lint`, `typecheck`, `test` and `format:check` all pass.

**Wasm baseline** (release, `opt-level = "s"`, LTO, before wasm-opt):

- size: 21,378 bytes raw, 8,545 bytes gzipped, plus 6,070 bytes of JS glue,
- Node `initSync`: about 2 ms warm.

Task-pane budgets are set in Phase 2.

**CI:** pending the first run on the pull request.

**Not done / known issues**

- TypeScript 7 is out, but typescript-eslint supports only TypeScript below 6.1, so we're on 6.0.3. Revisit at each phase start.
- LibreOffice is not installed, so the headless smoke test is replaced locally by `scripts/word-smoke.ps1` (Windows with Word). CI has neither.
- GitHub private vulnerability reporting must be turned on in the repository settings (Security → Private vulnerability reporting) for SECURITY.md to work.
- The fixture set is 10 files; Phase 1 grows it to 30 or more.
- **Windows long paths.** pnpm 12's recursive runner writes a state file under `node_modules/.pnpm-task-run-state-v1/` with a long hashed name. In a deeply nested clone (folder path over about 120 characters), `pnpm test` fails with "The system cannot find the path specified". Fix: clone to a shorter path, or turn on Windows long-path support. The README says so.

## Session 0: orientation (2026-10-14 to 15). Done.

- Read CLAUDE.md, BLUEPRINT.md, PROMPTS.md and every board of the blueprint canvas.
- Repo connected to https://github.com/Shiki992/lineage.
- Installed Rust 1.99.0 (with clippy, rustfmt, wasm32 target) and pnpm 12.11.2 through npm (corepack 0.33 can't run pnpm 12's native binary); `%APPDATA%\npm` added to the user PATH. Visual Studio 2026 C++ tools and Node 24.3.0 were already present.
- Copyright holder and DCO sign-off: Hrishikesh Vedantam. Repo-local git `user.name` set.
- Decisions: 0001 no command line for users, 0002 change states, 0003 accepted changes land as tracked changes, then a checkpoint. PROMPTS.md moved into docs/.
- `test/` holds the maintainer's own Word documents (42 resumes, 3 business documents). They contain real personal data, so `/test/` is git-ignored. Use them for local fidelity runs only.
  - By structure only: 4 resumes saved by Word (including the master), 38 written by a script; the business documents mix both. No tracked changes, fields or footnotes.

## Open questions (carried forward)

- BLUEPRINT open decision 5 (start from files made by hand): which phase. Phase 3's recognition (sketch shortlist, word-by-word match) is the matching it needs.
- Files that come back: whether kept changes should keep only the words for files saved by another app, and whether Lineage's tracked changes should name the recipient as author (decision 0012).
- Whether a change left pending (never clicked) should also go quiet after the master moves on by a set number of checkpoints, like the undetected case in decision 0002.
- Canvas mismatches still to fix on the canvas or in BLUEPRINT:
  - solid dark boxes used for both "offered change" and milestone,
  - Send a copy defaults to PDF and has no "leave out the ID" switch,
  - "slide notes" as a review channel,
  - Drive properties vs the ledger,
  - "Restore as the master" and "Share family",
  - EMEA "caught up at v2" vs "took v5".
- Re-verify Docs API suggest-mode writes before Phase 5.

## Next

1. The maintainer registers the app and runs `docs/testing/word-personal.md` on Word for Windows and Word on the web, with OneDrive, now including steps 12 to 20.
2. Then Part B of `docs/testing/my-documents.md`: My documents with the same registration (one more redirect URI), and the Word footer link.
3. Fix whatever those runs find.
4. Phase 4 (PowerPoint deck variants), when the maintainer starts it.
