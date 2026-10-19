# 0021: Lining up rows and columns before comparing cells

Date: 2026-10-18. Status: accepted.

## Context

People sort, filter, insert and delete rows in workbooks all the time. A comparison by cell address would report a sorted sheet as every row changed, and would put an edit on the wrong record after an insert. CLAUDE.md ("Excel engine", Row and column alignment) asks for alignment as its own stage before cell comparison, keys first, then composite keys, then content similarity, and explicit ambiguity instead of a guess. It must scale to the 50,000-row and 200,000-row fixtures.

## Decision

Alignment (`crates/core/src/xlsx/align.rs`) runs per pair of sheets and gives each axis a `Matching`: matched pairs, inserted, deleted, **could not match** (on each side), and moved (out of the longest common order).

### Sheets

By name (case-insensitive, as Excel matches), then by `sheetId` (a rename keeps it). The rest were added or removed.

### The header row

A table's header row, or else the first of the top ten rows that has two or more text cells, no numbers and no formulas, and rows below it.

### Columns

By header text where there is a header, otherwise by an order-free key of the column's values (the same values in any order give the same key), as a longest common subsequence. Between matched columns, gaps of equal size pair by position (a renamed or rewritten column). Anything else was inserted or deleted.

### Rows, in order of trust

1. **A key column**, over the matched columns. It must be unique and at least 90% filled on both sides, hold text or whole numbers, and not be formatted as a date (a corrected date would turn the record into a deletion and an insertion). It is scored: an ID-like header ("ID", "code", "ref", "number", "#", "key", "SKU") first, then ID-like values ("APP-007", whole numbers), then a text label column. At least half its values must be on both sides for an identifier, 80% for a label column. Ties go to the leftmost column.
2. **A composite key** of two columns (dates allowed), unique together on both sides, with at least 80% of its values on both. The highest overlap wins, so for the budget (Date, Description) wins over (Date, Category) when a line is recategorised.
3. **Whole content**: rows with the same input values over the matched columns pair up, so a re-sorted sheet matches fully. Identical rows are interchangeable while unchanged. With a key, a row whose key isn't on the other side is a different record and is never matched by content.
4. **Similarity**, when there is no key: candidates are found through values at most four rows share, and scored by the share of filled cells that are equal. A pair needs at least 50% (`LIKENESS`), must be each other's best, and must be strictly better than the runner-up. **Two equally likely rows are not guessed**: the row is "could not match", and it is neither matched nor counted as deleted.
5. **Rows that hold only formulas** (calculated lines such as Gross margin) pair by position in gaps of equal size between matched rows.

Rows down to the header (titles and the header itself) pair by content, then by position.

### Moves, sorting and filtering

- Moved rows are those off the longest increasing order of the matches.
- When b's matched rows are in the order of one column and a's were not, the alignment says the rows were **re-sorted** by that column, so the panel can say "Sorted by Amount" instead of listing every move.
- Hidden rows are a visibility change on a matched row, never a reorder.

### Measured

On this computer (debug build), 20,000 rows sorted against the original align in well under the test's time; release figures for 50,000 and 200,000 rows are recorded at stop 1 (decision 0025).

## Alternatives

- **Comparing by address.** Rejected: CLAUDE.md forbids putting an edit on the wrong record.
- **A full minimum-cost matching** (Hungarian) on similarity. Cubic, and its "best" answer hides ties that should go to the person.
- **Trusting any unique column as a key.** Found wrong in tests: with three rows, a Colour column looked unique and stable at 50%.

## Consequences

- A key column added or removed between versions changes how rows match; the panel can name the key used ("Matched rows by ID").
- User-marked key columns (CLAUDE.md) aren't built in this phase; `RowKey` can hold them.
- Rows that could not be matched surface in "What changed" as their own group, and the merge treats them as overlaps.
