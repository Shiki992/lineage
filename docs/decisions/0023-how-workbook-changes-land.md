# 0023: How the master's changes land in a workbook

Date: 2026-10-18. Status: the merge, the apply plan and the writer accepted; how Excel carries the plan out is checked at stop 2 (the Excel spike), and this note is finished then.

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

## Alternatives

- **Rewriting the open workbook from bytes.** Office.js can't. `insertWorksheetsFromBase64` adds whole sheets, which would break every reference into the sheet it replaced.
- **Applying row deletions and hoping.** Rejected above.
- **Merging every different cell.** CLAUDE.md: different cells are not automatically independent.

## Consequences

- A whole family of changes (formats, names, sorting, columns) is by hand in v0, and listed as such with the reason.
- The plan's addresses are for the workbook as it was read; the pane must re-read and re-plan if it changed (stop 2 decides how).
