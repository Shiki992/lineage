# 0026: What a sent workbook leaves out, and the parts Lineage keeps without tracking

Date: 2026-10-18. Status: accepted.

## Context

CLAUDE.md: copies that leave are clean, with no history, comments, tracked changes or hidden data, only the minimal Lineage ids. For workbooks, "hidden data" is not simple: hidden sheets, rows and columns often hold the lookups and inputs a model's formulas read, and removing them changes the numbers. The maintainer chose (2026-10-17): keep hidden sheets, rows and columns, and warn.

CLAUDE.md also asks that VBA, pivot tables, Power Query, charts, external links and other advanced parts be kept unmodified, never described as tracked, and labelled "Preserved; semantic tracking not yet supported."

## Decision

### A clean sent workbook (`crates/core/src/xlsx/clean.rs`)

Left out:

- notes and threaded comments, the people who wrote them, and the VML drawings that only showed notes (a drawing with form controls stays), with the sheets' links to them;
- revision logs (`xl/revisions/`), the thumbnail, add-in data;
- custom XML, except Power Query's item and its properties;
- personal metadata (core and app properties emptied, as for documents and decks);
- every custom property except the two Lineage ids, or none when the person turns them off.

Kept, and counted in the report so Send a copy can say so:

- **hidden sheets, rows and columns** ("2 hidden sheets go with this copy: formulas may read them");
- **macros** ("Macros go with this copy");
- Power Query queries, which the workbook's data needs.

The copy must reopen, and every relationship must point at a part, or nothing is sent.

### Parts kept, not tracked

`Workbook.unsupported` lists them by kind: macros (`vbaProject.bin`), pivot tables and caches, Power Query (the `DataMashup` item), charts, links to other workbooks, slicers and timelines, pictures, linked data types and pictures in cells, chart sheets, and macro or dialog sheets. Their parts are never rewritten. A change to their bytes is a structure change: "Macros changed. Preserved; semantic tracking not yet supported." "What changed" lists every kind a workbook has under that label, so a workbook with them is never shown as fully tracked.

The writer (decision 0023) leaves them byte for byte (tested on a `.xlsx` with a VBA part), and refuses to insert rows on a sheet a chart or pivot table reads, since it can't move their references.

## Alternatives

- **Remove hidden sheets, as decks remove hidden slides.** Rejected by the maintainer: formulas that read them would break.
- **Remove Power Query with the rest of the custom XML.** The workbook's queries would stop working.

## Consequences

- A sent workbook can carry hidden data the person forgot about; Send a copy names it before saving.
- Macros, pivots and queries are carried, not understood, until Phase 5B.

## Update, Phase 5B (2026-10-20)

Now tracked, when their parts read (decisions 0028 to 0030): VBA macros (`vbaProject.bin` and its signature parts), pivot tables and their caches, pivot slicers, and Power Query (the `DataMashup` item). Each leaves the "Preserved; semantic tracking not yet supported" list only when it was read; a part that doesn't read stays on it, as before. Still kept and labelled untracked: charts, links to other workbooks, table slicers and timelines, pictures, linked data types and pictures in cells, chart sheets, and Excel 4.0 macro or dialog sheets.

A sent copy still carries macros and queries. It now also blanks the pivot caches' `refreshedBy`, which names a person. Each hidden-content line says why it stays ("… formulas may read them"), because the closing line also follows "Macros go with this copy", where that reason doesn't fit (seen in Excel at the Phase 5B check).
