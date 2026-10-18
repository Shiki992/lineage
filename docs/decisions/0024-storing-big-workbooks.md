# 0024: Storing big workbooks

Date: 2026-10-18. Status: accepted.

## Context

A checkpoint is a manifest that lists every part of the file, and each part is a blob named by the SHA-256 of its bytes (decision 0009). A part shared by every checkpoint is stored once. Blobs are stored as they are, uncompressed.

That works for documents and decks, where the parts that change are small. A workbook's sheets are not small. One worksheet with 200,000 rows is about 50 MB of XML, and a single changed cell makes it a new blob. Ten checkpoints of a large model would add half a gigabyte to the person's OneDrive.

## Decision

Large XML parts are stored in chunks, and each chunk is deflated (`crates/core/src/package/chunks.rs`, `crates/lineage/src/blob.rs`).

- **Which parts.** Any `.xml` part of 1 MB or more. In practice these are worksheets and the shared strings. Smaller parts, and all binary parts, are stored exactly as before.
- **Where the cuts go.** Only at the start of a `<row>` element, or of an `<si>` element in the shared strings. A cut is content-defined: it falls after a row whose bytes hash (FNV) to a multiple of 64, once the chunk is at least 256 KB, and is forced at 4 MB. The cuts depend on the rows' own content, not on their positions.
- **How chunks are stored.** Each chunk is deflated (miniz_oxide, already a dependency) and stored as a blob named by the SHA-256 of the deflated bytes. Equal chunks are stored once, like any blob.
- **The manifest.** A chunked part keeps its `blob` field, which is the SHA-256 of the whole part, and adds `chunks: [{ blob, len }]`, where `len` is the chunk's inflated size. The field is left out when it is empty, so every manifest for a document, a deck or a small workbook is byte for byte what it was before. Old manifests read exactly as before.
- **Restoring.** Every chunk is checked against its hash, inflated with its `len` as the limit (so a damaged or hostile blob cannot inflate without bound), and checked for that exact size. Then the joined part is checked against the part's hash. Any mismatch refuses the restore; nothing is guessed.
- **Listing blobs to fetch.** `manifest_parts` lists the chunk blobs of a chunked part rather than the part's own hash, so the pane's prefetch and the blob cache see what is actually stored.

Measured on the 30,000-row orders workbook: a snapshot stores under a quarter of the raw size, and one changed quantity stores under a fifth of that again as new blobs.

## What chunking does not fix

An inserted or deleted row renumbers every row below it (`<row r="…">` and every `<c r="…">`). Every chunk below the insert then has new bytes and is stored again. Chunking helps most with edits to values and formulas, sorting within a region, and appended rows. Rows inserted near the top of a large sheet still cost close to a full copy of that sheet, though deflated. The test for an insert in the middle checks only that the chunks above it are kept.

## Alternatives

- **Deflate whole parts, no chunks.** This is simpler and makes each checkpoint about four times smaller. But every edit still stores the whole sheet again.
- **Store the zip as it is.** It is already compressed. But no part could then be shared between checkpoints or versions, and restoring a single part would mean reading the whole file.
- **Name chunks by their inflated hash.** That would let two compressors give the same name. But then the stored bytes could not be checked before inflating them, and the stored blob is what untrusted storage holds.
- **Store cell-level deltas between checkpoints.** That is the smallest option. But a restore would then depend on every checkpoint before it, and one damaged blob would break every later checkpoint. A checkpoint stays self-contained.
- **Cut at fixed sizes.** One inserted byte would shift every cut after it.

## Consequences

- Big workbooks cost far less storage per checkpoint, and value edits cost little.
- The manifest format gains one optional field. An older engine reading a chunked manifest refuses it (`deny_unknown_fields`) instead of restoring the wrong bytes.
- Restoring a big workbook means inflating its chunks. That is measured with the budgets in decision 0025.
- Inserts near the top of large sheets remain expensive. A row-identity encoding of the sheet could fix that later without changing the manifest's shape again.
