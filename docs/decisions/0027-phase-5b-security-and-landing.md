# 0027: Macros, queries and pivot tables: security, and how their changes land

Date: 2026-10-20. Status: accepted.

## Context

Phase 5B (`docs/PROMPTS.md`) makes Lineage understand three things Phase 5 only carried (decision 0026): VBA macros, pivot tables with their slicers, and Power Query. Two of them are code. CLAUDE.md asks Lineage never to run anything inside a file, to flag macro changes in returned files and have them approved separately, and to treat returned files as untrusted input.

Office.js can't write any of them. It has no VBA API, `Workbook.queries` is read-only ([Excel.Workbook](https://learn.microsoft.com/en-us/javascript/api/excel/excel.workbook?view=excel-js-preview)), and pivot tables can be changed only through their own methods, which can't rebuild a definition exactly. Phase 5 brings the master's changes only into the open workbook, through Office.js (decision 0023).

## Decision

### Nothing runs

Lineage only parses stored bytes. It has no VBA compiler or interpreter, no M evaluator, and no query or macro runner, and it never refreshes a pivot or a query. Nothing Lineage does needs macros enabled. Both Excel checks ran with macros disabled (Excel's default at stop 2, and `msoAutomationSecurityForceDisable` in the stop 1 script), and Lineage never changes a Trust Center setting.

### Every parser has limits, and a fuzz target

- Compound files (MS-CFB): at most 64 MB and 65,536 directory entries, sector chains checked for loops, a directory depth of 32.
- VBA decompression: output capped per module (16 MB) and per project (64 MB); at most 10,000 dir stream records.
- The signature's DER walk is depth-capped. Signatures are read for the signer's name, never verified.
- The DataMashup item: at most 64 MB of base64 and 10,000 queries; the inner package goes through the package reader's limits (decision 0005).

Fuzz targets: `cfb_reader`, `vba_decompress`, `vba_project` and `mashup`, 30 seconds each in CI. libFuzzer doesn't build on this Windows machine, so they ran only in CI. The existing `xlsx_reader` target reaches the pivot reader.

### How changes land: by hand (the maintainer's choice, 2026-10-19)

The engine compares them, and the merge makes each master change to a module, reference, query, pivot table or slicer one item, with status `ByHand` (`Manual::Macro`, `Query` or `Pivot`). Its card shows the code or the fields and says how to copy them in by hand: the VBA editor (Alt+F11), Data › Queries & Connections, or the pivot's fields. It is "already there" once the version matches, and says so when the version changed it too. **Copy code** puts the whole module or query on the clipboard, not only the changed lines. **Open the master's version** opens the master's checkpoint as a new workbook to copy from.

A refresh is never an item. A pivot table or query that was only refreshed is summarised, like calculated results (decision 0022), and isn't stored in the checkpoint's change list. Its cells are offered as refreshed data, as Phase 5 does for query tables. Found at stop 2: stored, the refresh could never be decided, so the panel said "5 updates" over 4 cards and the version could never catch up.

### Files that come back with new macro code

When a returned file's macro code is new or changed against the copy that left, the review shows, first: **"This file came back with new macro code. Lineage never runs it. Check each macro before you bring it in."** What I've sent says "Came back with N changes, including new macro code". Each module is its own card, with **Skip** only, never in a group decision. Copying a returned module's code asks every time ("This code came from outside. Lineage never runs it, but Excel will once it is in your workbook and macros are on. Copy it only if you trust it.") and copies only after **Copy their code**. "Open their version" and "Open the master's version" are never offered for such a file.

### Opening the master's version without its macros (stop 2)

Excel asks before an add-in creates a workbook with macros: "Trust this add-in? The add-in … wants to create a file with macros, which could be a security risk." Trusting it would let the add-in create macro workbooks from then on, which is more than Lineage needs. So **Open the master's version** opens the checkpoint without its VBA project (`xlsx::clean::without_macros`, decision 0028). The pane shows the code to copy, and nothing in the opened workbook can run. If Excel still refuses to open a copy (for example "Don't allow" on a sent copy with macros), the pane says "Excel didn't open the workbook. Nothing was changed."

### No new crates

Everything is in `lineage-core` on the crates it already has: the compound-file reader, VBA decompression, the DER walk, base64 and the M lexer are written by hand, each small and bounded. The inner Power Query package goes through the existing zip reader.

### The open change format (additive, v0)

`location.target` gains `macro`, `query`, `pivot` and `slicer`, each requiring `location.object` (the module, query, pivot table or slicer's name). The part pattern now allows `.bin` parts and `customXml/`. Existing change lists stay valid. Examples: `valid/xlsx-macros-queries-pivots.json`, `invalid/xlsx-macro-without-object.json`.

### Budgets (agreed with the maintainer at stop 1, 2026-10-19)

Macros, queries and pivot tables add at most 100 ms natively and 300 ms in the task pane's wasm to opening and comparing a workbook. Measured with `cargo xtask bench-objects` (release, natively), on the computer of decision 0025; the numbers are in `docs/STATUS.md`.

## Alternatives

- **A VBA writer**, so the master's macro changes could be written into the version's file. The spike says Excel would need to recompile what Lineage writes, and it didn't (decision 0028). And with Office.js unable to put a VBA project into the open workbook, nothing in the pane could use it.
- **Bringing changes in through a closed-file write** (the Rust writer, decision 0023). Rejected for this phase: every Phase 5 bring-in is into the open workbook, and a second path would split the check-and-take-back-out guarantees.
- **Opening the master's version with its macros**, and letting Excel ask. Rejected: it asks the person to make a security decision just to look at the master.

## Consequences

- Macro, query and pivot changes take a person's hands in Excel, card by card. The pane makes the copy exact (Copy code) and says what to do.
- A returned file's macros never reach the master without the person copying them in themselves.
- A version that changed a module too is told, but Lineage doesn't merge code line by line.
