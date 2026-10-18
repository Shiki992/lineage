# 0025: Phase 5 dependencies, and the workbook budgets

Date: 2026-10-17. Status: dependencies accepted; budgets proposed at stop 1 (below).

## Context

Phase 5 (`docs/PROMPTS.md`) brings Excel workbooks to the engine, including 50,000-row and 200,000-row sheets. CLAUDE.md asks for streaming parsing where it helps, a small dependency tree, and a decision note for every new crate. It also says never to invent performance budgets: propose them from measured fixtures and get them agreed first.

The engine reads XML with roxmltree (decision 0005), which builds a tree of the whole part. A 200,000-row sheet is 50 to 100 MB of XML, and a tree of it would be several times that, which is too much for the task pane's WebAssembly memory.

## Decision

### Dependencies

| Crate       | Version | Licence | Released   | Used by        | Why                                                                                                                                                                                                                            |
| ----------- | ------- | ------- | ---------- | -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `quick-xml` | 0.42.0  | MIT     | 2026-08-22 | `lineage-core` | A pull parser for worksheets and the shared-strings table, so a sheet is read row by row and never held as a tree. Default features off: no serde, no encodings, no async. Brings only `memchr`, which roxmltree already uses. |

How it is used (`crates/core/src/xlsx/stream.rs`):

- Namespaces are resolved (`NsReader`), so `x:`-prefixed SpreadsheetML from other programs reads the same as Excel's.
- DTDs are refused, as for every part (decision 0005). Only XML's five predefined entities and character references are resolved.
- Every event's byte offset is kept. A cell's number, formula and cached result are spans into the part, and each row knows its exact bytes, so the writer can splice rows without re-serialising anything else.
- Everything outside `<sheetData>` (conditional formats, data validation, page setup, extensions) is hashed into canonical keys from the same events, with attributes in a fixed order and Excel's revision ids (`xr:uid`, `x14ac:dyDescent`) left out.

The workbook part, styles, tables and connections are small, and stay on roxmltree.

Development and tests only:

- the fuzz crate depends on `lineage-testkit` (a workspace crate), to wrap fuzzed worksheet XML in a valid workbook.

### Alternatives

- **roxmltree for everything.** Simplest, and every other part uses it. Rejected for worksheets on memory: it keeps a node per element and attribute range for the whole part.
- **A hand-written scanner for `<sheetData>`.** No dependency, and the grammar inside `<sheetData>` is small. But it still has to resolve namespace prefixes, entities, CDATA and comments correctly, on untrusted input. quick-xml is widely used and fuzzed by its own project.
- **xmlparser** (roxmltree's former tokenizer). Its repository is archived.

### Budgets

Proposed at stop 1 from measurements; see the section added then.
