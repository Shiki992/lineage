# Status

## Phase 5B: macros, pivot tables and Power Query (2026-10-20). Built, and checked twice in Excel for Windows: invented workbooks re-saved by Excel (stop 1), and the pane on a scratch dev folder (stop 2). The master's own macro edit wasn't checked in Excel (see Not done).

Branch `claude/modest-perlman-35f244`, from the Phase 5 branch `claude/amazing-lamport-081931` at `45fdf1b`. Not pushed.

Decisions:

- [0027 Macros, queries and pivot tables: security, and how their changes land](decisions/0027-phase-5b-security-and-landing.md): nothing runs, parser limits, by-hand landing, the returned-macro warning, the spec change, no new crates, the budgets;
- [0028 Reading VBA, and why Lineage only compares it](decisions/0028-reading-vba.md), with the spike's result;
- [0029 Pivot tables, slicers and refreshed pivot data](decisions/0029-pivot-tables-and-slicers.md);
- [0030 Power Query](decisions/0030-power-query.md);
- [0026](decisions/0026-sent-workbooks-and-untracked-parts.md) updated with what is now tracked; [0010](decisions/0010-wasm-boundary-and-budgets.md) with the sizes and pane timings.

Manual test script: [docs/testing/excel-macros.md](testing/excel-macros.md).

The maintainer chose (2026-10-19 and 20):

- changes to macros, queries and pivots are compared by the engine and come in by hand, with no new write path;
- a Windows-only VBA spike, with VBA shipped compare-only (it was: decision 0028);
- at stop 1, the Excel re-save run, the compile check done through computer use, and the budgets: at most 100 ms natively and 300 ms in the pane on top of a workbook's own;
- at stop 2, the whole run: the pane's dev server on port 3000 with a scratch folder, a scratch Excel-only sideload (removed afterwards), and an Excel instance of its own with macros left disabled.

**Done**

- **Reading** (`crates/core/src/vba/`, `xlsx/pivot.rs`, `xlsx/query.rs`), each bounded and fuzzed:
  - **VBA:** compound files, MS-OVBA decompression, the dir and PROJECT streams, module kinds, references, "locked for viewing", and who signed;
  - **pivots:** caches, pivot tables and pivot slicers;
  - **Power Query:** the DataMashup item, its inner package, an M lexer, and each query's id, load target and last load.

  A kind leaves the "Preserved; semantic tracking not yet supported" list only when its parts read.

- **What changed:** Macros, Queries and Pivot tables groups after Formulas.
  - Code cards show changed lines with three lines of context, line numbers and a − / + glyph with the words Removed and Added. Pivot cards show the fields before and after. The Macros group says who signed.
  - A pivot's output cells are refreshed data, never inputs, and so is a refresh. Excel's own formatting of pivot cells is left out of structure.
  - The open change format gained `macro`, `query`, `pivot` and `slicer` targets with `object` (additive, v0).
- **Merge:** each module, reference, query, pivot or slicer change is one item that comes in by hand ("already there" once it matches; says when the version changed it too).
  - **Copy code** copies the whole module or query.
  - **Open the master's version** opens the master as a new workbook without its macros.
  - Refresh-only changes are never items and aren't stored.
- **Returned files:** new or changed macro code shows the warning first. Each module is its own card with Skip only. Copying its code asks first. What I've sent says "… including new macro code", and no Open is offered.
- **Clean copies** count readable macros, and blank the pivot caches' `refreshedBy`.
- **Family logic:**
  - a checkpoint records whether the file has macros;
  - a returned file records whether it came back with new macro code;
  - My documents rows and pages say **Contains macros**;
  - the library index is at version 2.
- **Boundary and screens:** DTOs, schemas and golden replies for code lines, field lists, object groups, the warning and badges.
  - The Excel pane: stacked code view, field table, Copy code with its confirm, the warning, Open the master's version.
  - My documents: code side by side at full width, and the field table.
- **Fixtures** (invented, `crates/testkit`): a compound-file writer, the VBA compressor (reproduces the spec's three examples), projects in the spec's on-write form, pivots and slicers with their rendered cells, DataMashup items.
  - 16 workbooks in `fixtures/xlsx/synthetic/`: the founder's model with macros, two pivots on one cache, a slicer and two queries; a variant for each kind of change; a signed project; a copy that came back with a new module.
  - 7 saved and edited by real Excel in `fixtures/xlsx/excel/` (`scripts/excel-resave-5b.ps1`).
- **Fuzz targets** `cfb_reader`, `vba_decompress`, `vba_project` and `mashup`, 30 s each in CI. **`cargo xtask bench-objects`.**

**Checked in Excel for Windows** (16.0.20430), macros disabled throughout, invented workbooks only:

- **Stop 1** (`scripts/excel-resave-5b.ps1`, Excel's own instance, macros force-disabled):
  - Excel opened and saved the macro, pivot and query workbooks without a repair, keeping every module's code exactly.
  - It made three pivot edits of its own. They read as a pivot change, refreshed data (the typed sale the only input) and a slicer change.
  - A plain save shows no changes.
  - **The spike:** Debug › Compile on the spec-form project stopped with "Invalid procedure call or argument" (nothing ran), so VBA is compare-only.
- **Stop 2, through the real pane** (DevTools port, scratch dev folder, scratch sideload). On "Startup model.xlsm":
  1. **Start tracking**; Northwind's tailored version; **Send a copy** to Harbor Lane said "Macros go with this copy".
  2. A sale typed (Sales!E13 396 → 420), SeatsByPlan's rows changed from Plan to Month, and RevenueByRegion refreshed (through COM, saved).
  3. **Save checkpoint:** "1 input, 2 pivot table changes, 2 refreshed values since v1". Sales!E13 was the only input; "Rows: Month (was Plan)" with the field table; "RevenueByRegion was refreshed: 2 values changed. From a data refresh. Nobody typed these values." No structure group (after the first fix below).
  4. **Offer**, then Northwind's **Review them**: the sale with Bring in; the pivot card by hand with Decline; the refreshed values said to refresh. **Open the master's version** opened Book1: an `.xlsx`, no VBA project, both pivots, connections disabled, no trust prompt (after the third fix below). Closed unsaved. **Bring in 1 change:** Northwind's E13 became 420, it was saved, its macros were intact and its pivot was unchanged (v2).
  5. **A file came back** with the copy that came back (no ids): "This came back from Harbor Lane Capital … 98% of the text matches". **Review:** the warning first, then:
     - 2 inputs;
     - Macros: "Macro RefreshForecast: 1 line changed" (line 9 added) and "Added module Mailer (4 lines)", each with Copy code and Skip only;
     - Queries: "Query Rates: 3 lines changed", with the lines marked;
     - 2 refreshed values.

     **Copy code** on Mailer asked "This code came from outside…", and only **Copy their code** copied: the clipboard held the whole module. Forecast's copy was all 14 lines. What I've sent: "Came back with 7 changes, including new macro code", with Review only.

  6. **My documents** (built-in browser): the row and page say **Contains macros**. v1 against v2: the input, both pivot cards with their field tables, refreshed data.
  7. **The audit** (`dev_folder.rs`): 1 family, 4 checkpoints restore, 5 blobs check, the tailored version and the sent copy on disk match the record, 1 file came back.
  8. Afterwards: my Excel was closed without saving, the scratch sideload was removed (the maintainer's own entry kept), and the dev servers were stopped.

**Bugs found in the Excel runs, and fixed** (each with a test)

- **Excel's re-formatting of a refreshed pivot read as 13 structure changes** (cell formats, column widths). Pivot areas are now left out of formatting, and a pivot's cell count is its refreshed cells only.
- **A refresh-only pivot change was stored with the checkpoint.** It had no card, so Northwind's panel said "5 updates" over 4 cards, and the version could never catch up (it still says "has v1" in the scratch ledger, written before the fix). Refresh-only pivot and query changes are now summarised and not stored, like calculated results (decision 0022).
- **Open the master's version made Excel ask "Trust this add-in? … wants to create a file with macros".** "Don't allow" then showed only "Something went wrong". The master's version now opens without its VBA project (`xlsx::clean::without_macros`, a round-trip test on the full and signed models). If Excel refuses any copy, the pane says "Excel didn't open the workbook. Nothing was changed."
- **Send a copy said "Formulas may read them, so they stay"** after "Macros go with this copy". Each hidden-content line now gives its own reason; the closing line is "Check before you send it."
- From stop 1: the pivot key changed on a plain Excel save (attributes in other namespaces, `multipleFieldNames`).

**Found by the `vba_project` fuzz target in CI, and fixed:** the testkit compressor wrote an empty buffer as a chunk whose size underflowed ("attempt to subtract with overflow"). An empty buffer is now the signature byte alone (MS-OVBA 2.4.1.3.6). It was the test kit, not the reader, but the fuzz target builds its input with it. A regression test, and property tests that run the `vba_project` and `mashup` targets' paths natively with overflow checks on (5,000 cases each, locally).

**Measurements**

- **wasm** 2,132,549 bytes raw and 712,227 gzipped (was 1,982,567 and 657,409), under 4 MB. JavaScript gzipped: task pane 181,396, My documents 160,923.
- **Native** (`cargo xtask bench-objects`, release; the computer of decision 0025), all inside the 100 ms budget:

  | Case                        | Size        | Read    | Compare |
  | --------------------------- | ----------- | ------- | ------- |
  | A 20,000-line module        | 149 KB      | 11.2 ms | 10.4 ms |
  | 200 queries                 | 258 KB item | 3.8 ms  | 0.5 ms  |
  | A 20,000-record pivot cache | 66 KB file  | 3.2 ms  | 0.5 ms  |
  | The full model, both opened | 23 KB file  | 2.7 ms  | 0.4 ms  |

- **In the Excel pane** (its log): what changed 209 to 537 ms, save checkpoint 751 ms, updates 532 to 580 ms, bring in 823 ms, recognise 488 ms, returned review 201 ms, Open the master's version 1.4 s (16.8 s while Excel's trust prompt waited).

**Tests**

- **Rust:** 575 pass, 4 ignored (was 495).
  - Every parser, with malformed input.
  - Seven new property tests: decompression undoes compression and never panics; the code view rebuilds both sides; the M lexer never panics; reordering queries is no change; any bytes through the `vba_project` and `mashup` fuzz targets' paths never panic.
  - Diff, view and merge for each kind of change; the Excel-saved fixtures; family logic; golden replies.
- **TypeScript:** 307 pass (was 295): shell 176, add-in 47, My documents 31, design 15, OneDrive 29, storage-local 9. Playwright: 19 pass on Microsoft Edge, including a macro family kept as `.xlsm`.
- **All checks pass:** fmt, clippy, cargo-deny, licence headers, fixtures, lint, typecheck, Prettier, budgets.

**Not done**

- **The master's own macro edit in Excel** (script steps 3, 8, 10, and the macro parts of 6 and 15). Excel keeps the invented project's code, but after an edit in the VBA editor it couldn't save the workbook ("Errors were detected while saving"; cancelled). That is the spike's compile failure again. These steps need a project Excel made itself; the script says how. The same path is covered by engine, golden and component tests.
- **Pasting copied code into Northwind and seeing "already there"** (step 10): not clicked in Excel.
- **Excel for Mac**, and Excel on the web (decision 0020).
- **The fuzz targets run only in CI:** libFuzzer doesn't build on this Windows machine. Two of them are mirrored by native property tests.
- **Out of scope** (decision 0027): writing VBA, M or pivot definitions; `.xlsb`; Office Scripts; Excel 4.0 macro sheets, timelines and table slicers (still labelled untracked); `.docm` and `.pptm`.

**Known issues**

- **OLAP and data-model pivots** are read in part, and the panel doesn't say so yet.
- **`GETPIVOTDATA`** names a pivot by one cell, so its result can move while that cell doesn't; it isn't handled specially.
- **Open as sent** for a sent `.xlsm` opens its exact bytes, so Excel asks to trust the add-in. "Don't allow" now gets a clear message, but the person still meets the prompt.
- **Ledgers written by this branch before the refresh fix** keep a refresh-only pivot change that can't be decided (only scratch ledgers; nothing released has one).
- **Pivot cells formatted by hand** are left out with Excel's own formatting of them.
- **Driving Excel by computer use:** ribbon clicks and Alt+F11 didn't reach Excel. The pane in Northwind's window was opened by the maintainer, and the VBA editor through COM (`ExecuteMso("VisualBasic")`).

## Phase 5: Excel (2026-10-18). Built, and the three pilots clicked through in the Excel pane in Excel for Windows with a scratch dev folder. Excel on the web is not supported yet (decision 0020).

Branch `claude/amazing-lamport-081931`, from `main` at `bd8dd04`. Not pushed.

Decisions:

- [0020 Where workbook bytes come from, and which Excel Lineage needs](decisions/0020-workbook-bytes-and-excel-requirements.md);
- [0021 Row and column alignment](decisions/0021-row-and-column-alignment.md);
- [0022 Change categories and provenance](decisions/0022-workbook-change-categories-and-provenance.md);
- [0023 How the master's changes land in a workbook](decisions/0023-how-workbook-changes-land.md), with what Excel showed at stop 2;
- [0024 Storing big workbooks](decisions/0024-storing-big-workbooks.md);
- [0025 Phase 5 dependencies and budgets](decisions/0025-phase-5-dependencies-and-budgets.md): `quick-xml`, and the budgets agreed at stop 1;
- [0026 What a sent workbook leaves out, and parts Lineage keeps untracked](decisions/0026-sent-workbooks-and-untracked-parts.md);
- [0010](decisions/0010-wasm-boundary-and-budgets.md) updated with the Excel measurements and the engine Web Worker.

Manual test script: [docs/testing/excel-pilots.md](testing/excel-pilots.md).

The maintainer chose (2026-10-17 and 18):

- one phase with two stops;
- offers are reviewed in the open tailored workbook, and an apply plan is carried out through Office.js;
- a sent copy keeps hidden sheets, rows and columns, and says so;
- no command line: past 1.5 million filled cells, the panel says the workbook is too large;
- at stop 1, the budgets as measured, and the engine in a Web Worker in the Excel pane;
- at stop 2, the person's typing stays when Lineage takes its changes back out, the workbook is saved after a bring-in, ExcelApi 1.16 to bring changes in, and new rows take Excel's formatting from the row above, which the card says.

**Done**

- **Fixtures** (`crates/testkit/src/xlsx.rs`, `workbooks.rs`): invented workbooks built in Rust: the founder's financial model and investor versions, a household budget, a job tracker, 50,000 and 200,000-row sheets, 60 linked sheets, a data connection with a query table, a workbook with macros, and awkward cells. Twelve were re-saved and edited by real Excel (`scripts/excel-resave.ps1`, its own Excel instance, author and label data scrubbed) into `fixtures/xlsx/excel/`.
- **Reading** (`crates/core/src/xlsx/`): worksheets and shared strings streamed with `quick-xml`, each row keeping where it is in the part; numbers kept as their lexemes; shared formulas, tables, names, filters, hidden rows, styles, connections; parts Lineage keeps but doesn't track (macros, pivots, Power Query, charts, links to other workbooks, slicers) listed as "Preserved; semantic tracking not yet supported." Every fixture writes back byte for byte. A fuzz target runs in CI.
- **Formulas:** a tokenizer for references (cells, ranges, whole rows and columns, sheets, 3D, names, structured references, external links), canonical by row and column identity, so an inserted row doesn't change a formula; a dependency index; volatile functions flagged.
- **Alignment** (decision 0021): sheets by name, then id; columns by header or content; rows by a key column, a pair of columns, exact content, then similarity, with unsure rows marked and never guessed; moves found; a sort found and said once.
- **What changed** (decision 0022): five categories (inputs, formulas, knock-on changes, refreshed data, structure and formatting) with a provenance for each. Knock-on changes are summarised, not listed. Excel's change events, from the person and not from Lineage, mark direct edits. The open change format has workbook locations, categories and provenance, with examples and schema tests.
- **Merge and apply plan** (decision 0023): cell by cell over identities; overlaps for both sides changing a cell, a formula and its inputs, rows added at one place, and rows deleted or unmatched; formats, sorted or removed rows, columns, sheets and names come in by hand. Accepted changes become steps for Excel (rows inserted bottom-up, cells set at their final addresses), with what each target holds first, the way back for any number of steps carried out, and a check of what landed. A Rust writer carries the same steps out on bytes, for closed files and tests; its inserted row matches Excel's own.
- **Clean copies** (decision 0026): notes, comments, people and metadata left out; hidden sheets, rows, columns and macros kept, and named before saving.
- **Family logic** (`crates/lineage/src/workbooks.rs`): workbook families, offers brought into the open workbook in two calls (plan, then check and record), returned workbooks kept the same way, several master updates in order, the cell side by side, and the size limit.
- **Storage** (decision 0024): XML parts of 1 MB or more stored as deflated, content-cut chunks, so a changed cell stores one chunk. Manifests without large parts are unchanged.
- **Boundary and shell:** wasm calls for workbook what-changed (with hints), the side by side, prepare and finish for offers and kept changes, the way back, the put-back check and what goes with a copy, with schemas and golden replies. The shell's `CellHost`, `Lineage.bringInWorkbook` and `keepIntoWorkbook` (plan, carry out, check, append once, retry once, otherwise take the changes back out and say so), and `FakeExcel`. `Lineage` drives the engine through promises, so in Excel it runs in a Web Worker.
- **Excel pane:** the manifest's Workbook host; `ExcelHost` (whole file with unsaved edits, ids in custom properties, edit hints, cell plans written as the spike showed, Go to cell, save). Screens: what changed in five categories with Go to cell and the side by side; a tailored workbook's updates by category with Bring in, Decline, Keep mine and Use the master's; a returned workbook's changes with Keep and Skip; Send a copy saying what goes with the copy; Excel too old to bring changes in still shows them.
- **My documents:** workbook families compared in five categories, sent workbooks opened exactly as sent, restored as `.xlsx` or `.xlsm`.
- **Harness:** the three pilots in memory and on disk, a workbook test account, and the dev-folder audit reading workbooks.

**Checked in Excel for Windows** (16.0.20430, Current Channel, ExcelApi 1.21), the pane driven through the DevTools port, history in a scratch folder holding only the invented workbooks (not `.dev-storage`):

- **Stop 2 spike** (decision 0023): `getFileAsync` includes unsaved edits; `formulas` and `values` read strings as if typed, so numbers go in as numbers and text through `valuesAsJson`; a row inserted inside a table grows it; a plan stopped after its first step and a mismatch were both taken back out exactly.
- **The three pilots, through the real pane** (pane timings from its own log):
  1. the model: start tracking (0.5 s), two versions (0.1 s each), a copy sent to Harbor Lane Capital (0.45 s); 12 typed in Northwind's B3: "'15' changed to '12'", "Edited in this session", knock-on changes summarised, **Go to cell** selected B3 and **Side by side** showed 15 and 12; the master's typed price offered and brought into the open Northwind: B4 $55, B3 still 12, the workbook saved, checkpoint v3 (0.97 s); the copy came back with churn 3% (made by changing the sent copy's XML), recognised by its Lineage ID, and kept in Harbor Lane's open version (1.8 s);
  2. the budget: the broker's version sorted by Excel ("Sorted the rows on Transactions by Amount, largest first", one line); the master's row inserted mid-table and February's rent changed; both brought into the sorted version (0.84 s): the rent on the version's own row, the new row after the row it follows in the master, the table grown, the summary recalculated;
  3. the tracker: the coach's version filtered (two hidden rows and the filter listed); the master's status change and two new applications brought in (1.65 s): rows matched by ID, the Rejected rows still hidden, the filter extended by Excel.
- **The audit** of that folder: 3 families, 17 checkpoints restore, 20 blobs check, 4 versions on disk are their nodes' checkpoints, the sent copy matches its record, the returned file is kept exactly.
- Every workbook was saved after its checkpoints and bring-ins.

**Bugs found in the Excel runs, and fixed** (each with a test)

- **"A file came back" took only .docx**, so a returned workbook was refused in Excel.
- **A sent workbook was named "… .xlsx (recipient).docx"** (found by the harness pilots).
- **The not-landed message named PowerPoint** for every app.

**Tests**

- **Rust:** 495 pass, 4 ignored (was 363): the reader and fidelity on every fixture, formulas, alignment, the diff and its categories, merge, the apply plan and the way back, the writer, clean copies, chunked storage, the family logic, 7 property tests, the harness pilots and workbook account, golden replies. cargo-deny, licence headers and fixtures pass.
- **TypeScript:** 295 pass (was 257): shell 170 (the open-workbook path with a fake Excel, the engine behind a port), add-in 44, My documents 28, design 15, OneDrive 29, storage-local 9. Plus 19 Playwright runs (5 new, on the workbook account).
- **All checks pass:** fmt, clippy, lint, typecheck, Prettier, budgets (wasm 1,982,567 bytes raw and 657,409 gzipped, under 4 MB; task pane JavaScript 179,648 gzipped).

**Not done**

- **Excel on the web:** an add-in gets only a PDF of the open workbook there. Reading the saved file needs `Files.Read`, wider than the app folder. The maintainer's choice; until then the pane says to use Excel for Windows or Mac.
- **Typing while a plan runs, in real Excel:** the kept-typing path is tested with `FakeExcel`, and the strict path was seen in the spike, but a person typing during the second the plan runs wasn't clicked in Excel.
- **By hand in v0:** formats, rows the master removed or sorted, columns (including a column that was empty before and now has values), sheets, names, tables, merged cells, conditional formats and data validation.
- **From the prompt, left for later:** VBA, pivots and Power Query semantics (Phase 5B); the optional "Lineage review" sheet; user-marked key columns.
- **The workbook budgets** (open and compare times, decision 0025) are measured with `cargo xtask bench-xlsx`, not checked in CI. The panel's size limit wasn't exercised in Excel.

**Known issues**

- **A new row in a sorted version** arrives after the row it follows in the master, not at its place in the version's sort.
- **Two slow runs right after Office.js writes:** one checkpoint save took 6.8 s and one bring-in 5.6 s, against 0.5 to 1.8 s for every other. Neither repeated.
- **The panel refreshes when it comes back into view.** In the automated run, Excel's panes reported themselves hidden (the screen-capture tool hides WebView2 windows), so they were reloaded instead. Not seen by hand.
- **Phase 4's PowerPoint fixtures on `main`** carry the maintainer's name and a sensitivity-label tenant id that PowerPoint stamped. The Excel fixtures are scrubbed (`scripts/excel-resave.ps1`); the PowerPoint ones should be too.
- **The uncommitted CLAUDE.md draft** in the main checkout brings back `crates/cli`, which decision 0001 rules out. This phase followed the committed decisions.

## Phase 4 follow-up: bringing changes into an open deck (2026-10-18). Built, and checked in PowerPoint for Windows with a scratch dev folder.

Branch `claude/laughing-payne-b62016`, from `phase-4-deck-variants` at `06a8d55` (not pushed, so it may need a rebase when that is). Not pushed.

Decision: [0019 Bringing the master's changes into an open deck](decisions/0019-bringing-changes-into-an-open-deck.md); [0018](decisions/0018-how-deck-changes-land.md) points to it. No new dependencies.

**Done**

- **Engine** (`crates/core/src/pptx/slides.rs`, `check.rs`): slide steps from one deck to another (delete, insert after a slide, move), naming slides as Office.js does (`"<sldId>#<creationId>"`), changed slides deleted before they go back so they keep their ids; `check::landed`, whether what PowerPoint holds is the deck that was meant; `Error::NotLanded`.
- **Family logic** (`crates/lineage/src/decks.rs`): `prepare_open` runs "Update N versions" for the one open version and returns its line, the merged deck and the steps, recording nothing; `finish_open` runs it again, checks what PowerPoint holds, and records a checkpoint of that deck (or only the choices, for Keep mine on everything). A deck's version sees every pending master change, since a master deck has no offer step.
- **Boundary and shell:** wasm `prepareOpenDeck`, `finishOpenDeck`, `deckSlideOps`, with schemas and golden replies; `SlideHost` (`applySlideOps`, `canChangeSlides`); `Lineage.bringInOpenDeck`: carries out the steps, checks in the engine, appends once, retries once on a conflicting append, and otherwise puts the original slides and ids back and says so; `FakePowerPoint` for tests.
- **PowerPoint pane:** `PowerPointHost.applySlideOps` (one sync per step; PowerPointApi 1.8 for `moveTo`); a tailored deck's panel shows **Review them**; the review lists what comes in, what both changed with **Keep mine** / **Use the master's**, and what stays by hand, with **Bring in N changes**. Without 1.8, the old "close it and update from the master" text stays.
- Manual test script: steps 10a to 10c in [docs/testing/powerpoint-founder.md](testing/powerpoint-founder.md).

**Checked in PowerPoint for Windows** (Current Channel, PowerPointApi 1.8), the pane driven through the DevTools port, history in a scratch folder holding only the invented pitch deck (not `.dev-storage`):

1. master "$1.2M" → "$2.0M", checkpoint; Seed fund open: **Review them**, **Bring in 1 change**: slide 6 changed, kept `261#1002066859`, the deck's ids moved to the new checkpoint, one append. 1.06 s;
2. Northgate open with its own unsaved "$1.4M": the overlap shown side by side, **Use the master's**: "Your edits before the update", then the master's number. 1.2 s;
3. master added a Pricing slide and moved Market after Traction: Seed fund took both in one press, in the master's order. 3.0 s in a pane just reloaded; Northgate later 0.95 s;
4. a step forced to fail halfway (the new slide in, the move refused) and a forced mismatch (the new slide at the start): both put back exactly (same slides, ids and checkpoint), nothing recorded, and the pane said so;
5. **Keep mine** on Seed fund's own "$2.2M" against the master's "$2.4M": no slide changed, the choice recorded, nothing left waiting;
6. **The audit** (`dev_folder.rs`) of that folder after PowerPoint saved the decks: 12 checkpoints restore and hold together, 15 blobs check, and both versions on disk are their nodes' checkpoints.

**Bug found in the PowerPoint run, and fixed** (with a test): an open deck version showed no updates, because a deck version saw only _offered_ changes and a master deck never offers on its own.

**Tests**

- **Rust:** 363 pass, 3 ignored (was 342): slide steps and the landed check (with PowerPoint's own re-saved decks), a property test that the steps reach the merged deck and back, the open-deck family logic, golden replies.
- **TypeScript:** 257 pass (was 240): shell 138 (the open-deck path with a fake PowerPoint), add-in 38.
- **All checks pass:** fmt, clippy, cargo-deny, headers, fixtures, lint, typecheck, Prettier, budgets (wasm 1,429,062 bytes raw, under 4 MB; task pane JavaScript 164,461 gzipped).

**Not done**

- **PowerPoint on the web, and AutoSave on OneDrive**, where the open deck saves itself while slides change. The maintainer's run.
- **Sections:** Office.js has no section API. The merge never changes sections, but if PowerPoint puts a new or moved slide in a different section than the engine expects, the check fails and the deck is put back. Not met in the run (the pitch deck has none).

**Known issues**

- Typing in the deck during the second or so Bring in takes can make the check fail; the deck is then put back, and that typing on the changed slides with it.
- After **Keep mine** on everything, the pane says "Nothing brought in". True, but it could say the choice was kept.

## Phase 4: PowerPoint deck variants (2026-10-17). Built, and the founder journey clicked through in PowerPoint for Windows with the dev folder. PowerPoint on the web and OneDrive are the maintainer's run.

Branch `phase-4-deck-variants`, from `phase-3b-my-documents` at `c75e934`. Not pushed.

Decisions:

- [0017 The deck model and matching](decisions/0017-deck-model-and-matching.md);
- [0018 How a deck's changes land, and what a sent deck leaves out](decisions/0018-how-deck-changes-land.md);
- [0010](decisions/0010-wasm-boundary-and-budgets.md) updated: wasm budgets raised to 4 MB, blob rounds for "Update N versions", PowerPoint timings.

No new dependencies.

Manual test script: [docs/testing/powerpoint-founder.md](testing/powerpoint-founder.md).

The maintainer chose (2026-10-16):

- "Update N versions" writes the closed versions' files from the master's panel;
- a sent deck drops hidden slides only;
- stay inside Word's wasm budget, or raise it to 4 MB;
- keep the `.gitignore` as committed.

**Done**

- **Engine** (`crates/core/src/pptx/`):
  - the deck reader: slides, shapes (text, drawings, pictures, tables, charts, groups, SmartArt, media, OLE), placeholders, geometry, notes, hidden slides, layouts, sections, animations;
  - canonical keys that ignore what PowerPoint renews on save (decision 0017);
  - matching by creation id, then slide id, then title, layout and text; shapes by id, placeholder, then content and place;
  - what changed, grouped by slide, with word marks and plain summaries;
  - three-way merge per shape, notes, settings, animations, slide and order, reusing Word's overlap kinds;
  - the splice-only writer: shapes, notes and slides as parts, charts with their workbooks, media reused by bytes, order and sections rebuilt once;
  - the structure check, and clean copies;
  - `doc::Doc`, one face for Word and decks;
  - `spec/format`: `pptxLocation`, with examples.
- **Family logic** (`crates/lineage`):
  - commands work by the family's format;
  - `decks::update_versions` ("Update N versions", decision 0018) and `deck_panel`: what you just changed, which versions get it, the last copy sent;
  - a returned deck is recognised, but reviewing it says "comes later".
- **Boundary and shell:**
  - wasm `deckPanel`, `stageVersionFile`, `markVersionOpen`, `updateVersions`, with DTOs, schemas and golden replies;
  - `Storage` gains `readFileVersioned` and `replaceFile` (OneDrive: eTag and If-Match; dev folder: content hash) in every adapter and the conformance suite, and `FileLocked`;
  - `Lineage.deckPanel` and `updateVersions`: writes every file version-checked, appends once, puts files back on failure, and goes again without a locked version.
- **PowerPoint pane** (`apps/office-addin`):
  - one manifest for Word and PowerPoint; requirements checked at run time (WordApi 1.5, PowerPointApi 1.7);
  - `PowerPointHost`: the deck through `getFileAsync`, the ids as custom properties, "Open what they saw" through `createPresentation`;
  - the master deck panel (screen 4): the call-out with figures marked, a tick per version with its context, **Update N versions**, the per-version report with **Keep mine** / **Use the master's**, **Sent to … · Open what they saw**, and **Check for changes**;
  - a tailored deck's panel says to close it and update from the master;
  - Send a copy says hidden slides are left out and notes stay.
- **My documents:** decks compare slide by slide, open as sent (as .pptx), and restore as a new tailored version; the page names PowerPoint where it named Word.
- **Fixtures and tests:**
  - `crates/testkit` builds decks: 25 invented fixtures in `fixtures/pptx/synthetic`;
  - nine saved and edited by real PowerPoint through COM in `fixtures/pptx/powerpoint` (`scripts/powerpoint-resave.ps1`);
  - `scripts/powerpoint-smoke.ps1` opens decks in PowerPoint and compares their text with the engine's;
  - the `pptx_reader` fuzz target in CI;
  - the harness's founder journey, in memory and on disk;
  - the test accounts' deck is a real history.

**Checked**

- **PowerPoint for Windows** (16.0, Current Channel), with the pane driven through the DevTools port and history in a scratch dev folder holding only the invented deck (not `.dev-storage`). The founder journey:
  1. start tracking the eight-slide pitch deck;
  2. three tailored versions;
  3. a copy sent to Pellow Ventures from its version;
  4. Northgate's own "$1.4M" on slide 6, saved in PowerPoint;
  5. the master's "$2.0M": "You just changed Slide 6 · Traction";
  6. **Update 2 versions** with Pellow unticked. Seed fund brought it in. Northgate's edits were saved as a checkpoint first, and it asked to choose; **Use the master's** brought it in. Pellow kept "$1.2M";
  7. **Open what they saw** opened eight slides with "$1.2M ARR";
  8. two more master changes updated all three versions, Pellow taking both at once;
  9. with Seed fund open in PowerPoint, the others were updated and Seed fund's line said to close it; after closing it, it was updated.
- **The audit** (`dev_folder.rs`, now deck-aware) of that folder: 28 checkpoints restore and hold together, 34 blobs match, the 3 versions on disk are their nodes' checkpoints, and the sent copy matches its record.
- **Real PowerPoint opens** the three versions and the sent copy (COM, no repairs). Earlier in the phase: all 25 synthetic decks and 29 engine outputs, with their text as the engine wrote it.
- **The slide-replacement spike** (decision 0018): 10 cases, each landed exactly, 75 to 140 ms. Deleting the old slide first keeps its ids. The version panel's Bring in is not wired (see Not done).
- **My documents in the built-in browser** on the PowerPoint run's folder: the deck's seven checkpoints, and v1 compared with v7 ("2 changes", grouped under Slide 6 · Traction and Slide 8 · The ask).
- **Playwright, 14 runs on Microsoft Edge:** the deck replaces the old "nothing to open" run with compare, open as sent (SHA-256 equal to the copy that left) and restore.
- **LibreOffice:** the 25 synthetic decks and 22 merge outputs open headless (earlier in the phase). It was not run on the PowerPoint run's files, at the maintainer's request.

**Tests**

- **Rust:** 342 pass, 3 ignored (was 272): deck reading, matching, diff, merge, write, check and clean; 4 property tests; the PowerPoint fixtures; the family logic; the boundary.
- **TypeScript:** 240 pass (was 218): shell 124, OneDrive 29, storage-local 9, design 15, add-in 35, My documents 28. Plus 14 Playwright runs.
- **All checks pass:** fmt, clippy, cargo-deny, licence headers, fixtures, lint, typecheck, Prettier (on this phase's files; CLAUDE.md and BLUEPRINT have someone else's unformatted edits) and budgets.

**Bugs found in the PowerPoint run, and fixed** (each with a test)

- **One open version stopped "Update N versions"** with "The development storage couldn't do that." PowerPoint for Windows locks a deck it has open. Now `FileLocked`; the others are updated, and the line says to close it. The dev folder also left a temporary file behind.
- **A pane that had fetched nothing needed more rounds for each version** (8 runs for 4 versions, past the limit of 6). Every version is now tried before blobs are fetched.
- **One update stored 110 blobs, one at a time** (6.4 s for three versions). Blobs already seen in storage are skipped: 12 uploads, 2.2 s.
- **Files staged for a failed run** carried over to the next one.
- **"Check for changes" was hidden** while a change was shown; the ticks didn't reset for a different change with the same number of cards; a "Made …" notice stayed on later screens.
- **A table showed as changed** after PowerPoint first saved it (its new row and column ids).
- **"last sent 17 oct":** a version's whole status was lower-cased (Word too).
- **The golden replies were stale** after 251007d (a chart's change id).

**Budgets**

- wasm 1,409,100 bytes raw, 467,525 gzipped (was 1,159,791 and 388,922). It is over Word's 420 KB, so the limits are now 4 MB (the maintainer's choice; decision 0010 has the measurements and the per-host split that would get back under).
- JavaScript gzipped: task pane 162,848, My documents 158,133.
- In PowerPoint: engine ready median 56 ms; panel ready median 268 ms; the deck panel 166 to 272 ms early, 550 to 830 ms with seven checkpoints; Update 3 versions 1.9 to 2.2 s.

**Not done**

- **PowerPoint on the web, and OneDrive.** They need the maintainer's Microsoft sign-in, and OneDrive the Entra registration (`docs/testing/powerpoint-founder.md`). In particular: whether OneDrive locks a version open in PowerPoint (423) or lets co-authoring windows share it.
- **Bringing changes into an open tailored deck** from its own panel. The spike shows slides can be replaced exactly. Building it needs a checkpoint of what PowerPoint holds afterwards, a way back halfway through, and moves and sections (decision 0018).
- **Reviewing a deck that came back.** It is recognised, and the panel says it comes later.
- **Word's start-up** was not measured again: Word was open with the maintainer's own document.

**Known issues**

- **The deck panel and "Update N versions" are slower than a Word command** (see Budgets). The deck panel grows with the history.
- **The first save of a deck another app wrote** shows "The slide master, layouts or theme changed" and chart formatting once. These are design and formatting, never text.
- **The canvas's "Pellow replied"** isn't shown: the card is "Sent to Pellow Ventures on 17 Oct · Open what they saw".
- **SmartArt** isn't among the real-PowerPoint fixtures: through COM, PowerPoint's SmartArt layouts come back empty without a window. The synthetic decks have it, and it is brought in by hand.
- **The PowerPoint ribbon button** appeared only after the add-in was first started from **Home › Add-ins** in the dev run.
- **A sensitivity label** that this computer's Office applies on open made PowerPoint ask to save decks Lineage had written. Choosing Don't Save kept them as written.
- **BLUEPRINT §5 and CLAUDE.md** have someone else's uncommitted edits (the Excel engine and the CLI). They weren't touched, so BLUEPRINT doesn't yet say that tailored decks are updated from the master's panel.

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
