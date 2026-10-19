# Manual test: the Excel pilots

The Phase 5 "done when" check, run by hand in Excel for Windows: the founder's financial model with versions for investors, a household budget whose version is sorted, and a job tracker whose version is filtered. Each master change is brought into an open tailored workbook through its pane, and a copy that came back is kept. About 30 minutes.

Use invented workbooks only. `cargo xtask fixtures` writes them to `fixtures/xlsx/synthetic/`:

- `model.xlsx`: the financial model. Assumptions!B4 is the price per seat ($49); Revenue, P&L and Summary are formulas.
- `budget.xlsx`: a Transactions table and a Summary of SUMIFS by category.
- `tracker.xlsx`: applications with an AutoFilter, and a Counts sheet.

Copy them into a scratch folder first, so Lineage writes nothing next to the fixtures.

## Once, before the first run

1. **Trust the development certificate:**
   ```bash
   powershell -ExecutionPolicy Bypass -File scripts/dev-cert.ps1
   ```
2. **Sideload** with Word, PowerPoint and Excel closed. The one manifest lists all three apps:
   ```bash
   powershell -ExecutionPolicy Bypass -File scripts/sideload-word.ps1
   ```
3. Point the history at a scratch folder, then start the task pane's server and leave it running:
   ```bash
   $env:LINEAGE_DEV_STORAGE = "C:\Temp\lineage-excel-run"
   pnpm --filter @lineage/office-addin dev
   ```

Afterwards, audit the folder with the Rust engine:

```bash
cargo test -p lineage-harness --test dev_folder -- --ignored --nocapture
```

Excel add-ins can't open a file on this computer, so **Open** on a version can't open it. Open it yourself from the folder's `Tailored versions`.

The pane in Excel needs ExcelApi 1.7. Bringing changes in needs 1.16 (Office 2024 or Microsoft 365); with an older Excel the review shows what changed and says to update Excel (decision 0020).

## 1. The financial model

| #   | Do                                                                                                                                                                      | Expect                                                                                                                                                                                                                                                                         |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | Open the scratch copy of `model.xlsx` (rename it "Startup model.xlsx"). **Home › Lineage**. If the button is missing, **Home › Add-ins › Developer Add-ins › Lineage**. | The pane opens on **Start tracking**.                                                                                                                                                                                                                                          |
| 2   | **Start tracking**. **Make a tailored version**: "Startup model – Northwind", then "Startup model – Harbor Lane".                                                       | "Lineage is tracking this file. This is v1." Then "Made “Startup model – Northwind.xlsx”." each time.                                                                                                                                                                          |
| 3   | Open Harbor Lane's version. **Send a copy**: "Who it's for" = Harbor Lane Capital, Lineage ID on.                                                                       | The format says "Excel (.xlsx, or .xlsm with macros)" and "no notes, comments or document properties". With hidden sheets or rows, a warning first says how many go with the copy. **Save copy and record it** saves "Startup model – Harbor Lane (Harbor Lane Capital).xlsx". |
| 4   | Open Northwind's version, type 12 in Assumptions!B3 (new seats a month), save. In its pane: **Save checkpoint**.                                                        | "What changed" shows **Inputs**: "'15' changed to '12'", "Edited in this session", and **Knock-on changes** summarised ("This changed … calculated cells, including …"), not listed one by one. **Go to cell** selects B3; **Side by side** shows 15 and 12, both "number".    |
| 5   | In the master, type 55 in B4 (the price). **Save checkpoint**, then offer it to both versions.                                                                          | "1 input, 83 knock-on changes since v1".                                                                                                                                                                                                                                       |
| 6   | In Northwind's open version: **Review them**, **Bring in**, **Bring in 1 change**.                                                                                      | B4 becomes $55 within a second or two, B3 stays 12, and the numbers below recalculate. "Brought in 1 change. Lineage saved a checkpoint of this workbook." The workbook is saved. The panel says it's up to date.                                                              |
| 7   | Open the copy saved in step 3, type 3% in B5 (churn), save it under a new name, and close it. In the master's pane: **A file came back**, choose it.                    | "This came back from Harbor Lane Capital", recognised by its Lineage ID. **Review** lists "'2.00%' changed to '3.00%'".                                                                                                                                                        |
| 8   | Review it in Harbor Lane's version (open it first): **Keep**, **Keep 1 change**.                                                                                        | B5 becomes 3.00% in Harbor Lane's version, which keeps the new price if it took it. "Kept 1 change …".                                                                                                                                                                         |

## 2. The household budget

| #   | Do                                                                                                                                                                           | Expect                                                                                                                                                                                                 |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 9   | Track the budget and make "Budget – Mortgage broker". In the version, sort Transactions by Amount, largest first. **Save checkpoint**.                                       | What changed is one line: "Sorted the rows on Transactions by Amount, largest first", not every row.                                                                                                   |
| 10  | In the master, insert a row in the middle of the table (right-click › Insert › Table rows above) and type a transaction; change February's rent below it. Save a checkpoint. | "1 input, …, 1 structure or format change": "Added row 5 on Transactions" and the rent. The rows below aren't reported as changed.                                                                     |
| 11  | In the broker's open version: **Review them**, **Bring in** on both, **Bring in 2 changes**.                                                                                 | The new row arrives where it belongs in the version's own order, the table grows, the summary recalculates. The card for the row says "New rows take the formatting of the row above." The sort stays. |

## 3. The job tracker

| #   | Do                                                                                                                                                                                                                 | Expect                                                                                                                                                                                                                                                                      |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 12  | Track the tracker and make "Job search – Coach". In the version, filter Status to hide Rejected. **Save checkpoint**.                                                                                              | The hidden rows and the filter are listed under **Structure and formatting**.                                                                                                                                                                                               |
| 13  | In the master, set one application to "Interview" and add two applications at the end. Save a checkpoint.                                                                                                          | "1 input, 1 structure or format change" or similar: the status, and "Added rows … on Applications".                                                                                                                                                                         |
| 14  | In the coach's open version: **Review them**, **Bring in** on everything. Just before pressing **Bring in 2 changes**, be ready to type in another cell of the version; type there as soon as you have pressed it. | Either the changes land (if your typing came after Lineage read the workbook back), or: "Excel didn't take the changes exactly as Lineage made them, so Lineage took its changes back out. What you typed meanwhile stays. Nothing was recorded." Press it again: it lands. |

## Afterwards

- Run the audit above: every checkpoint restores, every blob checks, and each version on disk is one of its node's checkpoints.
- **My documents** (footer link): compare two checkpoints of the model in the five categories; **Sent copies › Open as sent** downloads the .xlsx that left, byte for byte.
- Close Excel and Word before you sideload again.

## Excel on the web

Not supported yet: the pane says to open the workbook in Excel for Windows or Mac (decision 0020).
