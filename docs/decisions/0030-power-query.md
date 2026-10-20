# 0030: Power Query

Date: 2026-10-20. Status: accepted.

## Context

A workbook's Power Query queries are M code, stored as one base64 `DataMashup` item in a custom XML part ([MS-QDEFF](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-qdeff/41c5ab1e-6fcc-4355-9543-3125b01bed19)). The data they load sits in the sheet as a query table, which Phase 5 already treats as refreshed data (decision 0022). Phase 5 carried the item untracked (decision 0026). Office.js can list queries but not change them ([Excel.Workbook.queries](https://learn.microsoft.com/en-us/javascript/api/excel/excel.workbook?view=excel-js-preview), read-only).

## Decision

### Reading (`crates/core/src/xlsx/query.rs`)

- The `DataMashup` item is found in `customXml/`, in UTF-8 or UTF-16 (with its byte-order mark), and decoded by a hand-written, bounded base64 (decision 0027's limits).
- The [top-level stream](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-qdeff/22557f6d-7c29-4554-8fe4-7b7a54ac7a2b): a version, then four length-prefixed fields: the package parts, permissions, metadata and permission bindings.
- The package parts are an OPC zip, read with the existing package reader and its limits. `Formulas/Section1.m` holds the code.
- A small M lexer (comments, `"…"` with `""`, `#"quoted names"`) splits the section into its shared members, one per query.
- The metadata gives each query its `QueryID` (so a renamed query is matched), where it loads (`FillEnabled`, `FillTarget`), and when it last loaded (`FillLastUpdated`, `FillCount`).
- Permissions and permission bindings are skipped. The bindings are protected for one user and computer, and Excel may write them as a single 0x00 byte.
- An item that doesn't read stays "Preserved; semantic tracking not yet supported" (decision 0026).

Nothing is evaluated: no M runs, no source is reached, no query is refreshed.

### Comparing

Queries are matched by `QueryID`, then by name, then by alike code (as modules are, decision 0028). Code is compared line by line with the shared line diff: "Query Rates: 3 lines changed", "Added query Targets (3 lines)", "Renamed query Rates to FX rates". A change of where it loads says so ("It now loads to Rates (it is connection only)"). When only the last-load fields moved and the code is the same, the query was **refreshed** ("Query Rates was refreshed (3 rows, 2026-10-16)"). That is refreshed data, never an edit, and its loaded cells are refreshed data as in Phase 5.

### Landing

By hand (decision 0027): "Queries come in by hand: Lineage can't change query code in Excel. Open Data > Queries & Connections, edit the query and copy the code shown here." **Copy code** copies the whole query. A query that was only refreshed isn't an update of its own and isn't stored in the checkpoint's change list.

### Sent copies

The `DataMashup` item and its properties go with the copy (decision 0026), since the workbook's data needs them. Other custom XML is still left out.

## Alternatives

- **Writing M back through Office.js.** Not possible: `queries` is read-only.
- **Writing the item in the Rust writer.** That would change a part Excel protects with bindings it ties to one user, and Phase 5 brings changes only into the open workbook.
- **Using a general M parser.** Lineage only needs member boundaries, and a full grammar is much more untrusted surface.

## Consequences

- A query edit is shown as code, with its lines marked, and copied in by hand.
- Data a refresh loads is never shown as an edit, with or without the query's code changing.
- Excel shows "External Data Connections have been disabled" on a workbook with queries until the person enables them, as it did for the master's version at stop 2. Lineage never enables them.
