# 0025: Phase 5 dependencies, and the workbook budgets

Date: 2026-10-17, budgets 2026-10-18. Status: accepted.

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

### Budgets (agreed with the maintainer at stop 1, 2026-10-18)

Measured on this computer: Intel Core i9-12900H (14 cores), 32 GB, Windows 11, Rust 1.99. The wasm runs were the engine built as the task pane builds it (`opt-level = "z"`, `wasm-bindgen` 0.2.129, names removed), run in Node 24, whose V8 is the engine Excel's WebView2 uses; each case ran in a fresh process, so its wasm memory (which only grows) is the case's peak. The native runs are `cargo xtask bench-xlsx`, release build, with the peak working set from `scripts/bench-xlsx.ps1`. The workbooks are invented (`cargo xtask bench-xlsx prepare`): an Orders sheet of six columns, one of them a shared Total formula, and a Summary of five `SUMIF`s.

| Case                | Cells     | wasm: open one | wasm: compare two (both opens, diff, view) | wasm memory | Native: compare |
| ------------------- | --------- | -------------- | ------------------------------------------ | ----------- | --------------- |
| The financial model | 167       | 29 ms          | 49 ms                                      | 1 MB        | 4 ms            |
| 61 linked sheets    | 5,164     | 84 ms          | 140 ms                                     | 4 MB        | 24 ms           |
| 50,000 rows, edited | 300,018   | 1.5 s          | 4.7 s                                      | 89 MB       | 1.8 s, 110 MB   |
| 50,000 rows, sorted | 300,018   | 1.8 s          | 4.5 s                                      | 96 MB       | 2.3 s, 113 MB   |
| 200,000, edited     | 1,200,018 | 9.9 s          | 22.5 s                                     | 353 MB      | 7.6 s, 419 MB   |
| 200,000, sorted     | 1,200,018 | 8.2 s          | 25.1 s                                     | 377 MB      | 10.2 s, 427 MB  |

Native stage times for 200,000 rows sorted: zip 0.25 s, model 2.7 s (each version), alignment 1.75 s, dependencies 0.6 s, the whole diff 5.9 s. The workbook engine alone is 495,549 bytes of wasm; `opt-level = 3` made it 830 KB and only about 1.4 times faster, so the build stays at `"z"`.

The measurements found two bugs before any budget was set, both fixed with tests: filling gaps between matched rows was quadratic (20.5 s for an edited 50,000-row sheet, now 0.3 s), and sorting rows showed the summary's `SUMIF` ranges as changed formulas (a formula is now unchanged when its text or its meaning is; decision 0022).

The budgets, for a computer like this one:

1. **Up to 300,000 cells** (about 50,000 rows of six columns): opening a workbook at most 2 s, "What changed" at most 5 s, wasm memory at most 150 MB.
2. **300,000 to 1,500,000 cells:** it works, with a note: "This is a large workbook. Comparing it can take up to a minute." "What changed" at 1.2 million cells at most 30 s.
3. **Over 1,500,000 cells in one workbook**, or an operation whose versions together exceed about 1 GB of estimated memory: "This workbook is too large for Lineage yet." Nothing is changed. There is no command line (decision 0001); the native path is only for measurement.
4. **The engine runs in a Web Worker in the Excel pane**, so no workbook command blocks the pane (Office restarts an add-in that is unresponsive for 5 seconds, [Resource limits](https://learn.microsoft.com/office/dev/add-ins/concepts/resource-limits-and-performance-optimization)), and the panel can say what it is doing. Word and PowerPoint keep calling the engine as they do.
5. **The wasm stays within the 4 MB raw limit** (decision 0010), which CI enforces; the workbook engine adds about 0.5 MB.

The times are recorded and checked by hand in each phase's STATUS, not enforced in CI: they depend on the computer.
