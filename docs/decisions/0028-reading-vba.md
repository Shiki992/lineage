# 0028: Reading VBA macros, and why Lineage only compares them

Date: 2026-10-20. Status: accepted.

## Context

A macro workbook's VBA lives in `xl/vbaProject.bin`, a compound file holding compressed module source and Excel's compiled caches. Phase 5 carried it byte for byte and could only say "Macros changed" (decision 0026). Phase 5B reads it as code. The maintainer chose (2026-10-19) to run a Windows-only spike of writing VBA, and to ship VBA as compare-only unless the spike showed writing was safe.

Specs, checked 2026-10-19: [MS-OVBA](https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-ovba/575462ba-bf67-4190-9fac-c275523c75fc) rev 15.0 (2026-05-19) and [MS-CFB](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-cfb/53989ce4-7b05-4f8d-829b-d08d6148375b) rev 12.0 (2024-04-23). The signature parts' names and relationships (`vbaProjectSignature`, `…Agile`, `…V3`) follow implementations such as EPPlus.

## Decision

### Reading (`crates/core/src/vba/`)

- `cfb.rs`: compound files, version 3 and 4 sectors, the DIFAT, FAT and MiniFAT, and the directory walk, with the limits in decision 0027. Streams are found by path.
- `compress.rs`: MS-OVBA 2.4.1 decompression, capped.
- `dir.rs`: the dir stream's records. Code page, project name, references (registered, project, control, original) and MODULE records (name, Unicode name, stream, text offset, kind, private). PROJECTVERSION is the one record whose size isn't its length field.
- `project.rs`: the PROJECT stream's `Module=`, `Class=`, `BaseClass=` and `Document=` lines, and "locked for viewing" (the `GC` field, decrypted as 2.4.3.3 says).
- Module source is read at MODULEOFFSET, so the compiled cache before it is never used. SRP streams are ignored.
- Kinds: standard modules, classes, forms (the designer storage is hashed, so a changed layout reads as changed), and workbook and sheet code.
- Code pages: 1252, Latin-1 and UTF-8 are decoded exactly. Any other shows non-ASCII as "?" and is compared by its bytes, so a change hidden that way still says "Characters Lineage can't show changed". Lineage never re-encodes.
- `signature.rs`: the `vbaProjectSignature`, Agile and V3 parts, walked as DER for the signer's name. Not verified. The Macros group says "Signed by …. Lineage doesn't check signatures. Changing any macro removes the signature."
- A project that doesn't read stays "Preserved; semantic tracking not yet supported" (decision 0026).

### Comparing (`vba/diff.rs`, `code.rs`)

Modules are matched by name, then by likeness (at least 60% alike, for a rename). Lines are compared with the patience LCS, and each changed line is named by the procedure it is in: "Macro RefreshForecast: 1 line changed", "Added module Mailer (4 lines)", "Renamed module Module1 to Forecast". `Attribute VB_*` lines are compared apart from the code ("Its settings changed"), and so are references, the signature and the lock. The code view shows the changed lines with three lines of context, line numbers, and a − or + glyph with the words Removed and Added, never colour alone.

### The spike: Lineage doesn't write VBA

`crates/testkit/src/vba.rs` writes projects in the spec's on-write form: `_VBA_PROJECT` with version 0xFFFF and no performance cache, each module's text at offset 0, no SRP streams. That is the form that makes Excel recompile.

- **Stop 1** (`scripts/excel-resave-5b.ps1`, Excel for Windows): Excel opened the workbooks without a repair and saved them keeping every module's code exactly (`fixtures/xlsx/excel/model-macros-saved.xlsm`; `excel_keeps_every_macro_line`). But **Debug › Compile** in the VBA editor stopped with "Invalid procedure call or argument" (checked through the editor; nothing ran).
- **Stop 2:** after one line of the master's code was edited in the VBA editor, Excel couldn't save the workbook: "Errors were detected while saving … Microsoft Excel may be able to save the file by removing or repairing some features." The save was cancelled.

So a project Lineage writes is kept by Excel only while nobody touches its code. That isn't safe enough to write into anyone's macros. There was no Mac to try, and by-hand landing (decision 0027) leaves nothing that would use a writer. The testkit projects stay test fixtures. The master-side macro edit in real Excel needs a project Excel made itself (see STATUS).

### One write: a workbook without its macros

`xlsx::clean::without_macros` removes the VBA project for **Open the master's version** (decision 0027): the project part, its relationships and signature parts, and their content types. It also turns the workbook's content type into the macro-free one. Every other part stays byte for byte, as the round-trip test checks on the full model and the signed project. A workbook with Excel 4.0 macro sheets, which only a macro-enabled workbook may hold, is left as it is. In Excel at stop 2 the master's version opened as an `.xlsx` workbook with both pivot tables and no VBA project, without the trust prompt.

## Alternatives

- **A VBA writer in the spec's on-write form.** Rejected on the spike (above).
- **Writing the compiled caches too.** They're undocumented and version-specific, so Lineage would be guessing.
- **Reading the compiled cache instead of the source.** Same reason, and the source is what people read.
- **An existing crate** for compound files or VBA. Each would be a new dependency to justify, for a few hundred lines whose limits on untrusted files Lineage wants to choose itself.

## Consequences

- Macro changes are compared exactly and brought in by hand, module by module.
- If a later phase needs to write VBA, it starts from a project Excel saved, and the spike must be run again, on Mac too.
