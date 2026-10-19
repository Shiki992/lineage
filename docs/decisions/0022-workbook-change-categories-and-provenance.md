# 0022: The five kinds of workbook change, and how sure Lineage is of each

Date: 2026-10-18. Status: accepted.

## Context

People lose track of which numbers in a workbook changed, who changed them, and why. A price typed in one cell can move hundreds of calculated cells; a data refresh can move numbers nobody typed. CLAUDE.md ("Excel engine") asks for every change to be sorted into one of five categories, reported separately, each with a provenance, and never to infer cause from two snapshots alone. The comparison must work from the files; change events from the open workbook can only raise confidence.

## Decision

`crates/core/src/xlsx/diff.rs` compares two versions after alignment (decision 0021); `view.rs` words it.

### The categories

1. **Input**: a typed value changed, appeared or went (by the double a number stands for, decision 0020).
2. **Formula**: the formula changed, or a value became a formula or the other way. It stays a formula change when the result is the same. Formulas are compared by what they mean: each reference is turned into the identity of the row and column it points at (from the alignment), names and functions into one case. So when Excel rewrites `=B5` as `=B6` after a row is inserted above, that is not a change. Shared formulas are compared cell by cell, as they read in their own cells.
3. **Knock-on**: a calculated result changed while its formula didn't.
4. **Refreshed data**: a value inside a query table's range (data loaded by a connection), or a calculated result whose formula is volatile (`TODAY`, `NOW`, `RAND`, `OFFSET`, `INDIRECT`, …, or `ca="1"`), reads another workbook, or is downstream only of those.
5. **Structure and formatting**: sheets added, removed, renamed, moved, hidden; rows and columns added, removed, moved, sorted, hidden, reformatted; merged cells, conditional formats, data validation, protection, links, filter, page setup, sheet settings; defined names (compared by meaning, so a name that moved with its row is unchanged); tables; data connections; workbook and calculation settings; a cell whose only change is its format; and parts kept but not tracked (decision 0026), labelled "Preserved; semantic tracking not yet supported."

Rows added or removed are reported once each run, not as one input change per cell.

### Provenance

- **Confirmed direct edit** (`DirectEdit`): only when the open workbook reported an edit to that cell in this session (`Hint`, from `onChanged`, with Lineage's own writes left out by `triggerSource`). Without one, an input or formula change is `Unknown`: the file can't say who typed it, or whether it was pasted.
- **Confirmed downstream effect** (`Downstream`): the dependency index (`deps.rs`) links the cell to an input, formula or row change in the same comparison, and nothing that changes by itself (a refresh, a volatile or external formula) or reads at run time (`OFFSET`, `INDIRECT`) is upstream of it.
- **Detected refresh** (`Refresh`): the evidence in category 4.
- **Likely downstream effect** (`LikelyDownstream`): linked as above, but a refresh or a run-time reference is also upstream.
- **Cause unknown** (`Unknown`): anything else. A calculated result that moved with nothing upstream changed stays a **knock-on change with cause unknown**: it is never shown as an edit, and the panel says "Cause unknown: nothing it depends on changed".

### Comparing calculated results

Excel stores the last calculated result of every formula. Excel's own recalculation can land one unit in the last place away from another program's arithmetic: the real-Excel fixtures (`fixtures/xlsx/excel/`) showed 14 such cells in the model after a plain open and save. Calculated results are therefore compared at the **15 significant digits Excel shows**. Typed inputs are compared exactly, by their double.

### What a re-save by Excel shows

Excel's first save of a file another program wrote rewrites parts that carry no meaning. Found on the real-Excel fixtures and left out of the keys:

- the default row height, which follows Excel's font metrics (15 in one program, 14.5 in Excel), unless it was set by hand;
- revision ids (`xr:uid`, `xr3:uid`) and `mc:Ignorable`;
- `filterMode` and a table's remembered `sortState`, which the filter and sort changes already report.

With these out, each of Excel's own edits in the fixtures shows exactly what was done: a sort is one "Sorted the rows on Transactions by Amount, largest first"; a filter is the hidden rows plus "Changed the filter"; an added table row is "Added row 18"; a plain re-save shows nothing.

### The view

- **Inputs** and **Formulas** are listed. A formula filled down a column is one card: "The formula changed in 24 cells (Revenue!D2:D25), filled down".
- **Knock-on changes** are summarised: "This changed 83 calculated cells, including Summary!B2 and Summary!B3", with five examples. The examples are the model's outputs first: cells nothing else reads, on sheets named like a summary or dashboard.
- **Refreshed data**: "12 values came from a data refresh, another workbook or a function like TODAY(). Nobody typed them."
- **Rows that could not be matched** are their own section, asking the person to check them.
- Each card has a place ("Assumptions!B4 · Price per seat, Value": the row's label and the column's header) and, where the evidence says something, a reason in plain words.

### The open change format

`spec/format/v0` gains `xlsx`: an `xlsxLocation` (part, sheet, sheet index, A1 address in the version the change is in, the address before, a target), and `category` and `provenance`, required for workbooks and refused for other formats. The engine's change lists are checked against the schema in Rust; the examples in both Rust and TypeScript.

## Alternatives

- **Inferring "who changed it" from which cells changed.** Rejected by CLAUDE.md: a changed cached result proves nothing about who edited what.
- **Putting unexplained calculated changes in "refreshed".** Refreshed needs evidence; without it the honest answer is cause unknown.
- **Comparing calculated results exactly.** Every plain re-save by Excel would show changes nobody made.
- **Comparing formula text.** Every insert above a formula would show as a formula change.

## Consequences

- Event hints exist only while the panel is open; provenance is `Unknown` for every edit made with it closed, which is most edits. The panel says nothing rather than "unknown" for inputs.
- The dependency index is a superset where formulas can't be read exactly (names, tables, `OFFSET`), so "downstream" errs towards linking.
- Display values are derived with a small number-format renderer (`display.rs`) for the common formats; unusual formats show the number.
