# 0023: How the master's changes land in a workbook

Date: 2026-10-18. Status: accepted, after the stop 2 spike in Excel and the maintainer's choices on what it found.

## Context

The maintainer chose (2026-10-17) that offers are reviewed in the open tailored workbook, and that on Accept the engine produces an apply plan that the task pane carries out through Office.js (CLAUDE.md, "Excel engine", Applying accepted changes). Office.js can't replace an open workbook. It can insert and delete rows and set cell values and formulas ([Range](https://learn.microsoft.com/javascript/api/excel/excel.range)). Excel has no tracked changes, so nothing is written until the person has accepted it.

CLAUDE.md's merge rules: independent changes to different cells may merge; the same cell changed on both sides is a conflict; a formula changed on one side and its inputs on the other merges only when the engine can show they are independent; structural edits resolve against row and column identity, never coordinates; unsupported or ambiguous interactions become explicit conflicts; never drop an edit.

## Decision

### The merge (`crates/core/src/xlsx/merge.rs`)

Three versions: base (the master checkpoint the version last took), ours (the tailored version, as open in Excel) and theirs (the master). Both sides are compared with base (decision 0022), and each of the master's changes becomes an item:

- **Clean**: the cell's row and column are matched in ours, ours didn't change it, and no formula is tangled with an input (below).
- **Already there**: ours has the same content.
- **Overlap**, for the person, with **Keep mine** and **Use the master's**:
  - **both changed the cell**;
  - **a formula and its inputs**: the master changed a formula whose direct inputs (what it reads in base or in the master, mapped back through alignment) this version changed, or the master changed an input that a formula this version changed reads;
  - **rows added at the same place on both sides**, with **Keep mine**, and **Keep both** in either order.
- Overlaps with only **Keep mine**: the master changed a cell in a row this version **deleted**, or in a row Lineage **couldn't match** (decision 0021).
- **By hand** in v0: a cell's format (styles don't carry between workbooks yet), rows the master removed, moved or sorted, columns, sheets, names, tables, merged cells, conditional formats, data validation, filters, settings, refreshed data, parts kept but not tracked, and a formula that uses a defined name this version doesn't have (names come in by hand).

Knock-on changes are never items: Excel recalculates.

### Why the master's row deletions are by hand

When Excel deletes a row, every formula that pointed at it becomes `#REF!` for good. If an apply plan failed halfway after a deletion, the way back could put the row's cells back, but never the formulas elsewhere that it broke. So nothing that can't be undone exactly is written in v0.

### The apply plan (`apply.rs`)

From the person's decisions (each must be one of its item's choices):

1. **Rows to insert**, per sheet of ours: after the row of ours that the master's new rows follow (through base), and after this version's own new rows there for **Keep both, mine first**. They run **bottom-up**, so each insert's address is still right when it runs.
2. **Cells to set**, at their **final** addresses (after the inserts): a number by its lexeme; text exactly; a boolean; an error; or a formula in en-US. The master's formulas are written in this version's own rows and columns: each reference goes from the master through base to ours, or to the master's new rows where they now are. A formula that points at a row this version doesn't have, or that couldn't be placed, is **skipped** and reported with the reason, never written wrong.
3. **What each target cell holds now** (`expect`), for the pane to check before it starts.
4. **The way back** (`undo`): every set cell back to what it held (its formula moved as Excel moves it after the inserts), then the inserted rows deleted, bottom-up.

### Checking what landed (`landed`, `put_back`)

After the pane runs the plan, it reads the workbook back (as bytes), and the engine compares it with what the plan meant: every input by value and every formula by its text, in one case and without spaces, on every sheet. Ours' own cells are expected where the inserts moved them, with their formulas moved as Excel moves them. Calculated results are left out. Anything else, such as typing while the plan runs, is a mismatch: the pane runs the way back, and `put_back` checks that the workbook is as it was.

### The Rust writer (`write.rs`)

For closed files, the test harness's pilot runs and fidelity tests, the writer carries the same steps out on bytes, splice only:

- a set cell replaces its `<c>` element and keeps its format; text is written as an inline string, so it is never read as a number;
- a new cell or row goes where it belongs;
- an inserted row renumbers the rows below and moves every reference to them, as Excel does: formulas on every sheet, defined names, tables, merged cells, the filter, conditional formats and data validation;
- shared formulas the change reaches are written out cell by cell;
- the calculation chain is dropped (Excel rebuilds it) and `fullCalcOnLoad` asks Excel to recalculate on open, since cached results are left as they were.

It refuses to delete rows, and to insert rows on a sheet a chart, a pivot table or a what-if data table reads.

Checked: the writer's row inserted above Price per seat, with its two cells typed, gives exactly the inputs, formulas and names of Excel's own (`fixtures/xlsx/excel/model-row-inserted.xlsx`). Every writer output tested opens in real Excel without repair (`scripts/excel-smoke.ps1`). A property test merges random changes into an unchanged base and writes them: the result is the changed version, cell for cell.

## What Excel for Windows showed (stop 2, 2026-10-18)

Excel 16.0.20430 (Current Channel, ExcelApi up to 1.21), invented workbooks in a scratch folder, the pane driven through the DevTools port with the real shell, the dev-folder storage and `apps/office-addin/src/office/excel-host.ts`.

**Reading the workbook.** `getFileAsync(Compressed)` works in Excel for Windows, in one slice for these files, in 125 to 150 ms. It includes unsaved edits: a cell set through Office.js, with `isDirty` true, was in the bytes. So the pane never saves before planning.

**Writing values.** `range.formulas` and `range.values` read a string as if it were typed:

- "00123" became 123, "TRUE" a boolean, "=1+1" a formula, "#N/A" an error, and " 12" became 12;
- "1E-7", "1,234", "50%", "$5" and "12/03/2026" became numbers, and **also changed the cell's number format**;
- only 15 digits were kept: "0.30000000000000004" became 0.3.

So the host writes each kind differently:

- **numbers** as JavaScript numbers through `values`. That keeps every digit of the double (stored `0.30000000000000004`, `1.2345678901234568E+17`) and leaves the format alone. The plan still carries the lexeme; the host writes the double it stands for, which is what the engine compares;
- **text** through `valuesAsJson` as a string (ExcelApi 1.16). Every string above was stored exactly, as a shared string, with the format untouched. The exception is a leading apostrophe: Excel takes it as the quote prefix even there. So text starting with `'` is written with one more, and Excel gives the cell its quote-prefix style;
- **booleans** through `values`; **errors** and **en-US formulas** (with `=`) through `formulas`. Cross-sheet references such as `='P&L'!B2` were stored as written.

**Row steps.** `getRange("5:5").insert(down)` inside the budget's table grew the table (A1:D18) and kept its structured references. The summary's SUMIFS recalculated, and the landed check passed. `delete(up)` in the way back took the row out again. Excel gives inserted rows the format of the row above (its own "format same as above"). The plan doesn't set formats, and formats come in by hand in v0.

**The runs.**

1. The model's new price into Northwind: one `set`, landed, ids moved, one append. 0.62 s for the whole bring-in.
2. The budget's new row in the middle of the table, and February's rent below it: insert, five cells, landed. 0.61 s.
3. Two formulas changed in P&L: written as formulas, recalculated, landed. 0.76 s on a later run, with each file read about 140 ms and the plan itself 63 ms. One earlier run of the same kind took 5.6 s; it wasn't reproduced.
4. Two new rows with text that looks like a number and a boolean ("00123", "TRUE"): landed as text. 0.95 s.
5. Excel stopped after the insert (simulated with `PlanStopped(1)`): the way back was the one `deleteRows`, the workbook was back, and nothing was recorded.
6. The person typed into another cell while the plan ran: the landed check refused it, naming only the cell. The way back ran, but `put_back` then found the person's own typing, so the pane said to close without saving.

**Events.** A change Lineage wrote through Office.js has `triggerSource` `ThisLocalAddin`, so it isn't taken as the person's edit; "What changed" didn't call the new price "Edited in this session".

**Other findings.**

- Two text cells typed in a column that was empty before read as "Added column C", which comes in by hand. This is right by decision 0021, but a person may expect two inputs.
- After a bring-in, a tailored workbook on this computer is left unsaved; the history already has its checkpoint.

## Settled with the maintainer after the spike (2026-10-18)

1. **Typing while the plan runs is kept.** After the way back, the engine checks the workbook again with the cells Excel's change events say the person edited left out (`put_back_except`). When only those differ, the pane says "Lineage took its changes back out. What you typed meanwhile stays. Nothing was recorded." Anything else still says to close the workbook without saving.
2. **The workbook is saved after a bring-in** (`Workbook.save`, ExcelApi 1.11), as Word's document is, so a file on this computer matches the checkpoint just recorded. A failed save leaves it unsaved; the history is already written.
3. **ExcelApi 1.16 to bring changes in** (Office 2024 and Microsoft 365): text is written exactly only through `valuesAsJson`. Older Excel still shows what changed, and says to update Excel (decision 0020).
4. **New rows take Excel's formatting from the row above.** Every update card for new rows says "New rows take the formatting of the row above." Formats still come in by hand.

## Alternatives

- **Rewriting the open workbook from bytes.** Office.js can't. `insertWorksheetsFromBase64` adds whole sheets, which would break every reference into the sheet it replaced.
- **Applying row deletions and hoping.** Rejected above.
- **Merging every different cell.** CLAUDE.md: different cells are not automatically independent.

## Consequences

- A whole family of changes (formats, names, sorting, columns) is by hand in v0, and listed as such with the reason.
- The plan's addresses are for the workbook as it was read. The pane reads the workbook, plans and carries the plan out in one go, and the landed check catches anything that changed in between.
- Text that starts with an apostrophe gets Excel's quote-prefix style when it comes in.
