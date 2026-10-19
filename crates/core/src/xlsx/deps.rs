// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What each formula refers to, and the reverse: which formulas a cell
//! feeds. Used to say that a calculated cell moved because of an input
//! change (a knock-on change), and to find a formula whose inputs changed on
//! the other side of a merge. It never calculates.
//!
//! Precedents are a superset where the formula can't be read exactly: a
//! name is followed to what it refers to, a structured reference to the
//! table columns it names, and a formula with `OFFSET` or `INDIRECT` is
//! marked as having precedents nobody can know from the text.

use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};

use super::Workbook;
use super::formula::{Formula, SheetPrefix, Token, tokenize};
use super::model::{Area, Content, FormulaKind};
use super::refs::{MAX_COL, MAX_ROW};

/// A rectangle on a sheet a formula reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Precedent {
    pub sheet: usize,
    pub area: Area,
}

/// One formula cell and what it reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaCell {
    pub sheet: usize,
    pub col: u32,
    pub row: u32,
    pub precedents: Vec<Precedent>,
    /// `OFFSET`, `INDIRECT` or `CELL`: it reads things its text doesn't say.
    pub dynamic: bool,
    /// Excel recalculates it every time (`TODAY`, `RAND`, …, or `ca="1"`).
    pub volatile: bool,
    /// It reads another workbook.
    pub external: bool,
}

/// Areas taller than this go in a short list scanned for every lookup;
/// shorter ones are found by binary search.
const SMALL: u32 = 64;

#[derive(Debug, Default)]
struct ColumnIndex {
    /// (first row, last row, formula), sorted by first row.
    small: Vec<(u32, u32, usize)>,
    large: Vec<(u32, u32, usize)>,
}

/// Every formula of a workbook and a reverse index of what it reads.
#[derive(Debug, Default)]
pub struct Deps {
    pub cells: Vec<FormulaCell>,
    /// (sheet, row, column) → formula, for formula cells.
    pub at: HashMap<(usize, u32, u32), usize>,
    /// (sheet, column) → the formulas that read that column.
    index: HashMap<(usize, u32), ColumnIndex>,
    /// Formulas that read whole rows or more than this many columns, scanned
    /// for every lookup on their sheet.
    wide: HashMap<usize, Vec<(Area, usize)>>,
}

/// The formula of every formula cell on a sheet, with shared formulas
/// expanded to read as they do in their own cells.
///
/// Calls `f(row, column, formula, always)` for each, in order.
pub fn each_formula(
    wb: &Workbook,
    sheet: usize,
    mut f: impl FnMut(u32, u32, Cow<'_, Formula>, bool),
) {
    let text = wb.sheet_text(sheet);
    let Some(s) = wb.sheets.get(sheet) else {
        return;
    };
    // Shared formula masters: si → (row, column, tokens).
    let mut masters: HashMap<u32, (u32, u32, Formula)> = HashMap::new();
    for row in &s.rows {
        for cell in &row.cells {
            if let Content::Formula(fm) = &cell.content
                && let FormulaKind::Shared { si, .. } = fm.kind
                && let Some(t) = &fm.text
            {
                masters
                    .entry(si)
                    .or_insert_with(|| (row.r, cell.col, tokenize(t.get(text))));
            }
        }
    }
    for row in &s.rows {
        for cell in &row.cells {
            let Content::Formula(fm) = &cell.content else {
                continue;
            };
            match (&fm.text, &fm.kind) {
                (Some(t), FormulaKind::Shared { si, .. }) => match masters.get(si) {
                    Some((r0, c0, master)) if (*r0, *c0) == (row.r, cell.col) => {
                        f(row.r, cell.col, Cow::Borrowed(master), fm.always);
                    }
                    _ => f(
                        row.r,
                        cell.col,
                        Cow::Owned(tokenize(t.get(text))),
                        fm.always,
                    ),
                },
                (Some(t), _) => f(
                    row.r,
                    cell.col,
                    Cow::Owned(tokenize(t.get(text))),
                    fm.always,
                ),
                (None, FormulaKind::Shared { si, .. }) => {
                    if let Some((r0, c0, master)) = masters.get(si) {
                        let shifted = master.shifted(
                            i64::from(row.r) - i64::from(*r0),
                            i64::from(cell.col) - i64::from(*c0),
                        );
                        f(row.r, cell.col, Cow::Owned(shifted), fm.always);
                    }
                }
                (None, _) => {}
            }
        }
    }
}

/// The text of the formula in a cell, shared formulas expanded.
#[must_use]
pub fn formula_at(wb: &Workbook, sheet: usize, row: u32, col: u32) -> Option<String> {
    let mut found = None;
    each_formula(wb, sheet, |r, c, f, _| {
        if (r, c) == (row, col) {
            found = Some(f.render());
        }
    });
    found
}

struct Resolver<'a> {
    wb: &'a Workbook,
    /// Lower-cased sheet name → position.
    sheets: BTreeMap<String, usize>,
}

impl Resolver<'_> {
    fn sheet_range(&self, prefix: Option<&SheetPrefix>, own: usize) -> Option<Vec<usize>> {
        let Some(p) = prefix else {
            return Some(vec![own]);
        };
        if p.book.is_some() {
            return None;
        }
        let first = *self.sheets.get(&p.first.to_lowercase())?;
        let last = match &p.last {
            Some(l) => *self.sheets.get(&l.to_lowercase())?,
            None => first,
        };
        Some((first.min(last)..=first.max(last)).collect())
    }

    /// Adds what `formula` (read on `sheet`, in cell `row`) refers to.
    fn add(
        &self,
        formula: &Formula,
        sheet: usize,
        row: u32,
        col: u32,
        out: &mut FormulaCell,
        depth: usize,
    ) {
        if formula.has_dynamic_references() {
            out.dynamic = true;
        }
        if formula.is_volatile() {
            out.volatile = true;
        }
        if formula.is_external() {
            out.external = true;
        }
        for (token, _) in &formula.tokens {
            match token {
                Token::Ref(r) => {
                    let (c1, r1, c2, r2) = r.bounds();
                    let area = Area {
                        col1: c1,
                        row1: r1,
                        col2: c2,
                        row2: r2,
                    };
                    match self.sheet_range(r.sheet.as_ref(), sheet) {
                        Some(sheets) => {
                            for s in sheets {
                                out.precedents.push(Precedent { sheet: s, area });
                            }
                        }
                        None => out.external = true,
                    }
                    if r.spill {
                        // A spill range is as big as Excel makes it.
                        out.dynamic = true;
                    }
                }
                Token::Name {
                    sheet: prefix,
                    name,
                } => {
                    if prefix.as_ref().is_some_and(|p| p.book.is_some()) {
                        out.external = true;
                        continue;
                    }
                    let scope = match prefix {
                        Some(p) => self.sheets.get(&p.first.to_lowercase()).copied(),
                        None => Some(sheet),
                    };
                    let defined =
                        self.wb
                            .names
                            .iter()
                            .find(|n| {
                                n.name.eq_ignore_ascii_case(name)
                                    && n.local.is_some()
                                    && n.local == scope
                            })
                            .or_else(|| {
                                self.wb.names.iter().find(|n| {
                                    n.name.eq_ignore_ascii_case(name) && n.local.is_none()
                                })
                            });
                    match defined {
                        Some(n) if depth < 8 => {
                            let target = tokenize(&n.refers_to);
                            self.add(&target, n.local.unwrap_or(sheet), row, col, out, depth + 1);
                        }
                        Some(_) => out.dynamic = true,
                        // A table's name on its own means the whole table.
                        None => {
                            if let Some(t) = self
                                .wb
                                .tables
                                .iter()
                                .find(|t| t.name.eq_ignore_ascii_case(name))
                            {
                                out.precedents.push(Precedent {
                                    sheet: t.sheet,
                                    area: t.area,
                                });
                            }
                        }
                    }
                }
                Token::Structured(text) => self.structured(text, sheet, row, col, out),
                _ => {}
            }
        }
    }

    /// `Table[Column]`, `Table[[#This Row],[Column]]`, `[@Column]`.
    fn structured(&self, text: &str, sheet: usize, row: u32, col: u32, out: &mut FormulaCell) {
        let (name, inner) = text.split_once('[').unwrap_or(("", text));
        let table = if name.is_empty() {
            self.wb
                .tables
                .iter()
                .find(|t| t.sheet == sheet && t.area.contains(col, row))
        } else {
            self.wb
                .tables
                .iter()
                .find(|t| t.name.eq_ignore_ascii_case(name))
        };
        let Some(t) = table else {
            out.dynamic = true;
            return;
        };
        let inner = inner.to_ascii_lowercase();
        let this_row = inner.contains("#this row") || inner.starts_with('@');
        let first_data = t.area.row1 + t.header_rows;
        let last_data = t.area.row2.saturating_sub(t.totals_rows);
        let rows = if this_row {
            (row, row)
        } else if inner.contains("#all") {
            (t.area.row1, t.area.row2)
        } else {
            (first_data, last_data.max(first_data))
        };
        let mut named = false;
        for (i, column) in t.columns.iter().enumerate() {
            let lower = column.to_ascii_lowercase();
            let mentioned = inner.contains(&format!("[{lower}]"))
                || inner.trim_end_matches(']') == lower
                || inner.trim_start_matches('@').trim_end_matches(']') == lower;
            if mentioned {
                named = true;
                let c = t.area.col1 + u32::try_from(i).unwrap_or(0);
                out.precedents.push(Precedent {
                    sheet: t.sheet,
                    area: Area {
                        col1: c,
                        row1: rows.0,
                        col2: c,
                        row2: rows.1,
                    },
                });
            }
        }
        if !named {
            out.precedents.push(Precedent {
                sheet: t.sheet,
                area: Area {
                    col1: t.area.col1,
                    row1: rows.0,
                    col2: t.area.col2,
                    row2: rows.1,
                },
            });
        }
    }
}

impl Deps {
    /// Reads every formula of the workbook.
    #[must_use]
    pub fn build(wb: &Workbook) -> Self {
        let resolver = Resolver {
            wb,
            sheets: wb
                .sheets
                .iter()
                .enumerate()
                .map(|(i, s)| (s.name.to_lowercase(), i))
                .collect(),
        };
        let mut deps = Self::default();
        for sheet in 0..wb.sheets.len() {
            each_formula(wb, sheet, |row, col, formula, always| {
                let mut cell = FormulaCell {
                    sheet,
                    col,
                    row,
                    precedents: Vec::new(),
                    dynamic: false,
                    volatile: always,
                    external: false,
                };
                resolver.add(&formula, sheet, row, col, &mut cell, 0);
                let id = deps.cells.len();
                deps.at.insert((sheet, row, col), id);
                for p in &cell.precedents {
                    let a = p.area;
                    if a.col2 - a.col1 > 256
                        || a.row1 == 1 && a.row2 == MAX_ROW && a.col2 == MAX_COL
                    {
                        deps.wide.entry(p.sheet).or_default().push((a, id));
                        continue;
                    }
                    for c in a.col1..=a.col2 {
                        let column = deps.index.entry((p.sheet, c)).or_default();
                        if a.row2 - a.row1 < SMALL {
                            column.small.push((a.row1, a.row2, id));
                        } else {
                            column.large.push((a.row1, a.row2, id));
                        }
                    }
                }
                deps.cells.push(cell);
            });
        }
        for column in deps.index.values_mut() {
            column.small.sort_unstable();
        }
        deps
    }

    /// The formulas that read this cell directly.
    pub fn readers(&self, sheet: usize, col: u32, row: u32, out: &mut Vec<usize>) {
        if let Some(column) = self.index.get(&(sheet, col)) {
            // Small areas start at most SMALL rows above `row`.
            let lo = row.saturating_sub(SMALL);
            let from = column.small.partition_point(|&(r1, _, _)| r1 < lo);
            for &(r1, r2, id) in &column.small[from..] {
                if r1 > row {
                    break;
                }
                if r2 >= row {
                    out.push(id);
                }
            }
            for &(r1, r2, id) in &column.large {
                if (r1..=r2).contains(&row) {
                    out.push(id);
                }
            }
        }
        if let Some(wide) = self.wide.get(&sheet) {
            for (area, id) in wide {
                if area.contains(col, row) {
                    out.push(*id);
                }
            }
        }
    }

    /// Every formula that reads any of `cells`, directly or through other
    /// formulas, as a flag per formula.
    #[must_use]
    pub fn downstream(&self, cells: impl IntoIterator<Item = (usize, u32, u32)>) -> Vec<bool> {
        let mut seen = vec![false; self.cells.len()];
        let mut queue: Vec<(usize, u32, u32)> = cells.into_iter().collect();
        let mut readers = Vec::new();
        while let Some((sheet, col, row)) = queue.pop() {
            readers.clear();
            self.readers(sheet, col, row, &mut readers);
            for &id in &readers {
                if !seen[id] {
                    seen[id] = true;
                    let c = &self.cells[id];
                    queue.push((c.sheet, c.col, c.row));
                }
            }
        }
        seen
    }

    /// The formula in a cell, if it holds one.
    #[must_use]
    pub fn formula(&self, sheet: usize, row: u32, col: u32) -> Option<&FormulaCell> {
        self.at.get(&(sheet, row, col)).map(|&id| &self.cells[id])
    }
}

#[cfg(test)]
mod tests {
    use lineage_testkit::workbooks;

    use super::*;

    fn open(wb: &lineage_testkit::xlsx::Workbook) -> Workbook {
        Workbook::open(&wb.to_bytes()).expect("opens")
    }

    #[test]
    fn shared_formulas_expand_in_their_own_cells() {
        let wb = open(&workbooks::model_master());
        let revenue = wb.sheet_index("Revenue").expect("revenue");
        assert_eq!(
            formula_at(&wb, revenue, 2, 3).as_deref(),
            Some("B2*Price*(1-Churn)")
        );
        assert_eq!(
            formula_at(&wb, revenue, 9, 3).as_deref(),
            Some("B9*Price*(1-Churn)")
        );
        assert_eq!(formula_at(&wb, revenue, 25, 5).as_deref(), Some("C25-D25"));
        assert_eq!(
            formula_at(&wb, revenue, 3, 2).as_deref(),
            Some("B2+NewSeats")
        );
        assert_eq!(
            formula_at(&wb, revenue, 25, 2).as_deref(),
            Some("B24+NewSeats")
        );
    }

    #[test]
    fn a_changed_input_reaches_everything_downstream() {
        let wb = open(&workbooks::model_master());
        let deps = Deps::build(&wb);
        let assumptions = wb.sheet_index("Assumptions").expect("sheet");
        // Price per seat, Assumptions!B4, through the name Price.
        let reached = deps.downstream([(assumptions, 2, 4)]);
        let count = reached.iter().filter(|r| **r).count();
        let revenue = wb.sheet_index("Revenue").expect("sheet");
        let summary = wb.sheet_index("Summary").expect("sheet");
        let reaches = |sheet, row, col| {
            deps.at
                .get(&(sheet, row, col))
                .is_some_and(|&id| reached[id])
        };
        assert!(reaches(revenue, 2, 3), "MRR month 1");
        assert!(reaches(revenue, 25, 5), "gross margin month 24");
        assert!(reaches(summary, 2, 2), "year 1 revenue on the summary");
        assert!(!reaches(revenue, 2, 2), "seats don't depend on the price");
        assert!(!reaches(summary, 6, 2), "TODAY() doesn't");
        // 24 × MRR, COGS, margin; 4 × P&L revenue, COGS, margin, EBITDA …
        assert!(count > 72, "{count}");
        let today = deps.formula(summary, 6, 2).expect("TODAY");
        assert!(today.volatile);
    }

    #[test]
    fn tables_and_whole_columns_are_followed() {
        let wb = open(&workbooks::budget_master());
        let deps = Deps::build(&wb);
        let summary = wb.sheet_index("Summary").expect("sheet");
        // Changing an amount (Transactions!D5) reaches every SUMIFS and the total.
        let reached = deps.downstream([(0, 4, 5)]);
        for row in 2..=8 {
            let id = deps.at[&(summary, row, 2)];
            assert!(reached[id], "Summary!B{row}");
        }
        let wb = open(&workbooks::tracker_master());
        let deps = Deps::build(&wb);
        let reached = deps.downstream([(0, 5, 3)]);
        assert!(
            reached.iter().filter(|r| **r).count() >= 4,
            "COUNTIF over E:E"
        );
    }

    #[test]
    fn dynamic_and_external_formulas_are_marked() {
        let wb = open(&workbooks::connected_master());
        let deps = Deps::build(&wb);
        let prices = wb.sheet_index("Prices").expect("sheet");
        let eur = deps.formula(prices, 2, 3).expect("a formula");
        assert!(!eur.dynamic);
        assert!(eur.precedents.iter().any(|p| p.sheet == 0));
    }

    #[test]
    fn a_large_sheet_indexes_quickly() {
        let wb = open(&workbooks::large(20_000));
        let deps = Deps::build(&wb);
        assert_eq!(deps.cells.len(), 20_000 + 5);
        // Every SUMIF on the summary reads the Total column.
        let reached = deps.downstream([(0, 5, 100)]);
        assert_eq!(reached.iter().filter(|r| **r).count(), 1 + 5);
    }
}
