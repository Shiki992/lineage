# 0029: Pivot tables, slicers, and refreshed pivot data

Date: 2026-10-20. Status: accepted.

## Context

A pivot table is two things in a workbook. Its definition (which fields are rows, columns, values and filters, its layout) lives in `xl/pivotTables/` and its cache in `xl/pivotCache/`, both carried untracked in Phase 5 (decision 0026). Its output is plain cells in the sheet's `sheetData`, which Excel rewrites on every refresh. In Phase 5 those cells read as typed inputs, so refreshing a pivot looked like someone had edited a dozen numbers.

The reference is ECMA-376 Part 1 §18.10, through the Open XML SDK's [PivotCacheDefinition](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.pivotcachedefinition) and [PivotTableDefinition](https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.pivottabledefinition) pages. Slicers are the x14 extension.

## Decision

### Reading (`crates/core/src/xlsx/pivot.rs`)

- **Caches:** the source (a sheet and range, a name, a table, a connection, or external), the fields and calculated fields with their formulas. The refresh evidence (`refreshedDate`, `refreshedBy`, the record count, a hash of the records and shared items) is kept apart from the definition.
- **Pivot tables:** name, sheet, location and cache; rows, columns and where Values sits; report filters with their selection, hidden items, and label and value filters; values (field, summarised by, shown as, number format); calculated items; layout (form, totals, subtotals, style). Everything else is folded into a key. Attributes in other namespaces and `multipleFieldNames` are left out of it, because Excel adds or drops them on a plain save (found at stop 1).
- **Slicers:** pivot slicers (caption, cache, selected items). Table slicers and timelines stay "Preserved; semantic tracking not yet supported".
- OLAP and data-model pivots are read as far as their parts allow, and their caches are marked (`olap`). The panel doesn't yet say they are read only in part.

Pivot tables are matched by sheet and name, then by location and source. Each is one whole object: "Pivot table SeatsByPlan changed" with "Rows: Month (was Plan)" and its fields before and after, "Slicer Region: shows North, West (was all)".

### A pivot's cells are refreshed data

- Cells inside a pivot table's location in the later version are never input or formula changes. After a refresh they are **Refreshed data** ("Pivot table RevenueByRegion was refreshed: 2 values changed. From a data refresh. Nobody typed these values."). After a definition change they stay refreshed data, and the pivot's card says its data was refreshed.
- Rows a pivot grows or shrinks by are not rows someone added or removed.
- **Excel's own formatting of the pivot's cells, and its fitting of the pivot's columns, are left out** of structure and formatting. Found at stop 2: Excel re-formats a pivot's cells and resizes its columns when it refreshes, and the first Save checkpoint showed 13 structure changes nobody made. Now it shows "1 input, 2 pivot table changes, 2 refreshed values".
- A formula that reads a pivot's cells by reference is downstream of refreshed data (decision 0022). `GETPIVOTDATA` names the pivot by one cell, so its result can move while that cell doesn't. That case isn't handled specially, and wasn't tested.

### Landing

A pivot or slicer change comes in by hand (decision 0027): "Change its fields to match the list shown here." A pivot that was only refreshed isn't an update of its own. Its cells are offered as refreshed data, which says to refresh the workbook's data to get them. It isn't stored in the checkpoint's change list, which would leave it waiting forever (found at stop 2).

### Sent copies

A sent copy blanks each cache's `refreshedBy`, which names the person who last refreshed it. The rest of the pivot goes as it is.

## Alternatives

- **Treating a pivot's output cells as untracked, and skipping them.** A refresh changes numbers people read, and "Refreshed data" is the honest name for it.
- **Rebuilding pivots from Office.js.** Its pivot API can add and move fields, but not reproduce a definition exactly (layout, filters, calculated items), and a partial rebuild would be a silent merge.
- **Comparing the cache records value by value.** They are refreshed data too, and their effect is already in the pivot's cells.

## Consequences

- A pivot refresh reads as one line of refreshed data, not as edits.
- A pivot's own formatting by hand (a person bolding a pivot cell) is also left out, since it can't be told from Excel's. Formatting outside pivots is unchanged.
- Bringing in a pivot change takes the person's hands in the PivotTable Fields pane.
