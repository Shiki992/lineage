# 0020: Where workbook bytes come from, and which Excel Lineage needs

Date: 2026-10-18. Status: accepted for Excel for Windows and Mac. Excel on the web is the maintainer's run (below).

## Context

The engine works from a workbook's bytes (decisions 0010, 0022). In Word and PowerPoint the pane reads the open file with `getFileAsync(Compressed)`, unsaved edits included. Excel is different in two ways:

- [`Office.Document.getFileAsync`](<https://learn.microsoft.com/javascript/api/office/office.document#office-office-document-getfileasync-member(1)>) gives the compressed file in Excel on Windows and Mac. **Excel on the web gives only PDF.** Slices are at most 4 MB.
- Excel on the web limits each request and response to 5 MB, and one range read to 5,000,000 cells. A pane that stops responding for 5 seconds is restarted ([resource limits](https://learn.microsoft.com/office/dev/add-ins/concepts/resource-limits-and-performance-optimization)).

The ExcelApi sets Lineage uses, from [Excel requirement sets](https://learn.microsoft.com/javascript/api/requirement-sets/excel/excel-api-requirement-sets) (checked 2026-10-17):

| Set  | What Lineage uses                                                   | First in    |
| ---- | ------------------------------------------------------------------- | ----------- |
| 1.7  | `workbook.properties.custom` for Lineage's ids                      | Office 2019 |
| 1.11 | `Workbook.save` after a bring-in                                    | Office 2021 |
| 1.14 | `triggerSource` on change events, to leave out Lineage's own writes | Office 2021 |
| 1.16 | `Range.valuesAsJson`, the only way to write text exactly            | Office 2024 |

## Decision

### Where the bytes come from

- **Excel for Windows and Mac:** `getFileAsync(Compressed)`, as in Word and PowerPoint. The stop 2 spike (decision 0023) showed it includes unsaved edits, so the pane never saves before comparing or planning. Each read of the invented model took 125 to 150 ms.
- **Excel on the web:** there is no way to read the open file. The pane reads the file as last saved, from OneDrive through Graph (the stored file the dev folder or OneDrive storage already reads), and says "Showing the last saved version". Excel on the web saves continuously, so this is seconds behind at most. Bringing changes in reads the saved file the same way, after `Workbook.save`. This path is the maintainer's to check, since it needs a signed-in OneDrive.
- **Too large:** the engine refuses a workbook of more than 1.5 million filled cells before reading it fully, and the panel says it is too large for Lineage yet (decision 0025).

### Which Excel

- **The panel** needs ExcelApi 1.7, for the ids. Below it, the pane says to update Excel.
- **Bringing changes in** (offers and kept changes from a returned workbook) needs ExcelApi 1.16, checked at run time with `isSetSupported`. Below it, the panel still shows what changed, and the button says to update Excel. The maintainer chose this over writing text through `values` with a quote prefix, which changes the cell's style (2026-10-18).
- **Edit hints** need 1.14. Without them "What changed" works from the bytes alone, and never calls a change a direct edit.
- **Saving** after a bring-in needs 1.11; without it the workbook is left for the person to save.

### Carrying out a plan

From the spike:

- numbers are written as JavaScript numbers through `values`;
- text is written as strings through `valuesAsJson`;
- formulas (en-US) and errors go through `formulas`; booleans through `values`;
- row inserts and deletes sync one at a time; cells sync in batches of 1,000, well inside the 5 MB limit.

The engine runs in a Web Worker in the Excel pane (decision 0025), so a long comparison never blocks Office.

## Alternatives

- **Read the workbook cell by cell through Office.js.** That hits the 5 MB and 5-million-cell limits, gives values as Excel shows them, not as stored (no shared formulas, no lexemes), and can't see names, tables or styles as the file holds them.
- **Ask the person to save before every comparison.** Not needed on desktop, where reading includes unsaved edits. On the web, Excel saves continuously.
- **Support Office 2021 for bringing changes in.** It lacks `valuesAsJson`; see above.

## Consequences

- Office 2021 users can see what changed but must update Excel to bring changes in.
- On the web, what the panel compares can be a few seconds behind the screen, and it says so.
