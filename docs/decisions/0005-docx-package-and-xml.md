# 0005: Reading and writing .docx packages without changing what we didn't touch

Date: 2026-10-15. Status: accepted.

## Context

CLAUDE.md's first non-negotiable is "never corrupt a user's file", and its stack rules ask for zip and XML crates "that leave untouched XML exactly as it was, including unknown elements, attributes and namespaces". A .docx is a zip of XML parts. Word, other Office versions, add-ins and other tools put things in those parts that we don't understand and must not lose: `mc:AlternateContent`, `w14`/`w15`/`w16` extensions, custom XML, content controls, comments in the XML itself.

`core` must stay pure (no I/O) and compile to WebAssembly. Returned files are untrusted input. Versions were checked on crates.io on 2026-10-15.

## Decision

### Never re-serialise XML we did not change

The engine parses each part into a read-only tree that knows the byte range of every node and attribute in the original text. A change is a list of byte-range replacements (`xml::Edits`). Everything outside those ranges is copied through unchanged, so:

- an unchanged part is byte-identical,
- in a changed part, every paragraph we did not edit is byte-identical,
- prefixes, attribute order, quoting, whitespace, comments and processing instructions are never normalised.

Fragments we generate use the destination part's own prefix for WordprocessingML. Content copied from another document is checked: every prefix it uses must be bound to the same namespace where it lands, or the change is shown as "bring this in by hand" instead of being written.

### Crates

| Need             | Choice                                                             | Licence                   | Dependencies                         |
| ---------------- | ------------------------------------------------------------------ | ------------------------- | ------------------------------------ |
| Read zip         | `rawzip 0.5.2`                                                     | MIT                       | none                                 |
| Inflate, deflate | `miniz_oxide 0.9.1`, `default-features = false`, `with-alloc`      | MIT OR Zlib OR Apache-2.0 | `adler2` (0BSD OR MIT OR Apache-2.0) |
| XML              | `roxmltree 0.21.1`, `default-features = false`, `std`, `positions` | MIT OR Apache-2.0         | `memchr` (Unlicense OR MIT)          |
| Write zip        | our own, `crates/core/src/package/write.rs` (about 150 lines)      | Apache-2.0                | none                                 |

- **rawzip** reads straight from a byte slice, so it needs no I/O or `Seek`. It exposes each entry's raw compressed bytes, CRC, DOS time and flags, and handles zip64 and data descriptors. Its upstream is fuzzed.
- **miniz_oxide** is pure Rust and builds for wasm. We inflate with `decompress_to_vec_with_limit`, using the entry's declared size as the limit, so a stream that inflates to more than it claims is rejected as damaged rather than filling memory.
- **roxmltree** gives `range()` for every node and `range()`, `range_qname()` and `range_value()` for every attribute. It resolves namespaces, refuses DTDs by default (so no entity expansion), and takes a node limit.
- **Our own zip writer**, grown from the Phase 0 fixture writer. An unchanged entry is written with its **original compressed bytes**, method, timestamp and CRC. Changed entries are deflated at level 6, or stored when deflate doesn't help. Sizes go in the local header, with no data descriptors, which is how Word writes its own files. Output depends only on the entries, so it is deterministic.

### Limits for untrusted files

- At most 10,000 entries, 128 MiB per part and 256 MiB in total.
- Every CRC and uncompressed size is checked.
- Encrypted entries, compression methods other than stored and deflate, entries whose data overlap, and names that differ only by case are all rejected.
- XML: 8 million nodes per part, no DTDs.
- Parts must be UTF-8 to be read. A UTF-16 part is left untouched; if the main document is UTF-16, the file is reported as not supported yet.

Errors carry a plain-language message for the panel and a structural detail for logs, never document text.

### Independent checks

Tests read our output with the `zip` crate (dev-only, with `deflate-flate2`), which has its own archive parsing and CRC check, so a bug in our container code cannot hide behind the same bug in our reader. Its inflater is flate2's Rust backend, which is miniz_oxide again, so deflate itself is checked independently only by Microsoft Word (`scripts/word-smoke.ps1`). Fixtures are made by `crates/testkit`, which never uses the engine.

## Alternatives

- **`zip 9.0.3` for reading and writing.** Mature, but it brings `indexmap`, `typed-path`, `nix` (on Unix) and `flate2`. Its writer recompresses unless each entry is raw-copied, and it is built around `Read + Seek`. Kept only as the independent test reader.
- **rawzip's own writer.** It always writes data descriptors (bit 3). That is valid, but Word never writes them, and we want our output to look like Word's.
- **`flate2` with `zlib-rs`.** zlib-rs is under the Zlib licence only, which `deny.toml` does not allow. flate2's Rust backend is miniz_oxide anyway, so we use it directly.
- **`quick-xml 0.42`.** Fast, and it reports positions. But it is a pull parser, so we would build and maintain the tree ourselves, and its writer re-serialises events, which changes quoting and whitespace.
- **DOM crates that re-serialise** (`xmltree`, `minidom`, `xot`) and **docx model crates** (`docx-rs`, `docx-rust`). They rebuild the XML from a model, which drops or reorders whatever they don't model.

## Consequences

- Untouched content is safe by construction, and the round-trip tests can demand byte equality rather than "equivalent XML".
- All editing code is span arithmetic. `Edits::apply` refuses overlapping edits, edits that split a character and edits outside the text, so a bug fails loudly instead of writing a broken file.
- UTF-16 parts can't be edited yet. No Word, LibreOffice or Google Docs output we know of uses them.
- The wasm bundle grows by the three crates; Phase 2 measures it against the budgets.
