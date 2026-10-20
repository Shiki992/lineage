// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What changed between two versions of a workbook (decision 0022), after
//! alignment (decision 0021). Every change belongs to exactly one category:
//!
//! 1. **Input**: a typed value changed.
//! 2. **Formula**: the formula changed (even when its result didn't).
//! 3. **Knock-on**: a calculated result changed while its formula didn't.
//! 4. **Refreshed**: data from a connection, another workbook or a volatile
//!    function, never someone's edit.
//! 5. **Structure and formatting**: sheets, rows, columns, names, tables,
//!    formats and settings.
//!
//! and carries a provenance. Comparison works from the two files alone;
//! change events from the open workbook only raise confidence. A changed
//! calculated result never proves that anyone edited it.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::Workbook;
use super::align::{Matching, SheetAlignment, SheetPairs, align, pair_sheets};
use super::deps::Deps;
use super::display::display;
use super::formula::{Formula, RefKind, Reference, SheetPrefix, Token, render_prefix, tokenize};
use super::model::{Area, Cell, Content, FormulaKind, SheetKind, SheetState, UnsupportedKind};
use super::refs::{a1, col_name};
use super::value::{self, Value};
use crate::hash::Fnv;

/// The five categories (CLAUDE.md, "Excel engine").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Input,
    Formula,
    KnockOn,
    Refreshed,
    Structure,
}

/// Why a change happened, as far as the evidence goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Provenance {
    /// A change event from this session matches it.
    DirectEdit,
    /// The dependency graph links it to an input or formula change in this
    /// comparison, and nothing else upstream changed.
    Downstream,
    /// A data refresh, another workbook, or a volatile function.
    Refresh,
    /// The graph suggests it, but other explanations remain.
    LikelyDownstream,
    /// Anything else, including every change made while the panel was closed.
    Unknown,
}

/// A range the open workbook reported as edited in this session (an
/// `onChanged` event that wasn't Lineage's own write), in b's addresses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hint {
    pub sheet: String,
    pub area: Area,
}

/// One side of a changed cell, ready to show.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CellSide {
    /// What the cell shows, through its number format.
    pub shown: String,
    /// The formula, with `=`, when there is one.
    pub formula: Option<String>,
}

/// A cell whose content or format changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellChange {
    pub id: String,
    pub category: Category,
    pub provenance: Provenance,
    /// The sheet's name in b.
    pub sheet: String,
    pub sheet_a: usize,
    pub sheet_b: usize,
    /// (column, row) in each version.
    pub at_a: (u32, u32),
    pub at_b: (u32, u32),
    pub before: CellSide,
    pub after: CellSide,
    /// The format changed too (or only the format, for a structure change).
    pub format_changed: bool,
    /// Formula changes filled down a column share a group.
    pub group: u64,
    /// The row's label (the first text to the left), and the column's
    /// header, for the summary.
    pub label: Option<String>,
    pub header: Option<String>,
}

impl CellChange {
    /// "Assumptions!B4" in b.
    #[must_use]
    pub fn address(&self) -> String {
        format!(
            "{}!{}",
            render_prefix_name(&self.sheet),
            a1(self.at_b.0, self.at_b.1)
        )
    }
}

fn render_prefix_name(name: &str) -> String {
    let p = render_prefix(&SheetPrefix {
        book: None,
        first: name.to_owned(),
        last: None,
    });
    p.trim_end_matches('!').to_owned()
}

/// A calculated result that changed while its formula didn't: knock-on or
/// refreshed. Kept compact: a large model can have many.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calculated {
    pub category: Category,
    pub provenance: Provenance,
    pub sheet_b: usize,
    pub col: u32,
    pub row: u32,
    /// Nothing else reads this cell: an output of the model.
    pub terminal: bool,
}

/// What kind of structure or settings change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StructureKind {
    SheetAdded,
    SheetRemoved,
    SheetRenamed {
        from: String,
    },
    SheetMoved,
    SheetHidden,
    SheetShown,
    RowsAdded {
        rows: Vec<u32>,
    },
    RowsRemoved {
        rows: Vec<u32>,
    },
    RowsMoved {
        rows: Vec<u32>,
    },
    /// The rows were sorted by this column (its header, or its letter).
    RowsSorted {
        by: String,
        descending: bool,
    },
    RowsHidden {
        rows: Vec<u32>,
    },
    RowsShown {
        rows: Vec<u32>,
    },
    RowFormat {
        rows: Vec<u32>,
    },
    ColumnsAdded {
        cols: Vec<u32>,
    },
    ColumnsRemoved {
        cols: Vec<u32>,
    },
    ColumnsHidden {
        cols: Vec<u32>,
    },
    ColumnsShown {
        cols: Vec<u32>,
    },
    ColumnFormat {
        cols: Vec<u32>,
    },
    MergedCells,
    ConditionalFormats,
    DataValidation,
    Protection,
    Hyperlinks,
    Filter,
    PageSetup,
    SheetSettings,
    NameAdded {
        name: String,
    },
    NameRemoved {
        name: String,
    },
    NameChanged {
        name: String,
        from: String,
        to: String,
    },
    TableAdded {
        name: String,
    },
    TableRemoved {
        name: String,
    },
    TableChanged {
        name: String,
    },
    ConnectionChanged {
        name: String,
    },
    WorkbookSettings,
    Calculation,
    /// A part the engine keeps but doesn't track changed.
    Untracked {
        kind: UnsupportedKind,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureChange {
    pub id: String,
    /// The sheet's name in b (or in a, for a removed sheet), or "" for the
    /// workbook.
    pub sheet: String,
    pub kind: StructureKind,
}

/// Rows of one sheet that could not be matched with confidence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unmatched {
    pub sheet: String,
    pub rows_a: Vec<u32>,
    pub rows_b: Vec<u32>,
}

/// Everything that changed from `a` to `b`.
#[derive(Debug, Default)]
pub struct WorkbookDiff {
    pub sheets: SheetPairs,
    /// (sheet in a, sheet in b) → alignment.
    pub alignments: HashMap<(usize, usize), SheetAlignment>,
    /// Inputs, formulas, refreshed inputs and format-only cells.
    pub cells: Vec<CellChange>,
    pub calculated: Vec<Calculated>,
    pub structure: Vec<StructureChange>,
    pub unmatched: Vec<Unmatched>,
    /// Macros, compared as code (decision 0028). Empty when either side's
    /// macros can't be read: then the untracked change says so.
    pub macros: crate::vba::VbaDiff,
    /// Pivot tables and slicers (decision 0029).
    pub pivots: Vec<super::pivot::PivotChange>,
    pub slicers: Vec<super::pivot::SlicerChange>,
    /// Power Query queries (decision 0030).
    pub queries: Vec<super::query::QueryChange>,
}

impl WorkbookDiff {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
            && self.calculated.is_empty()
            && self.structure.is_empty()
            && self.unmatched.is_empty()
            && self.macros.is_empty()
            && self.pivots.is_empty()
            && self.slicers.is_empty()
            && self.queries.is_empty()
    }

    /// How many changes are in a category.
    #[must_use]
    pub fn count(&self, category: Category) -> usize {
        match category {
            Category::KnockOn | Category::Refreshed => {
                self.calculated
                    .iter()
                    .filter(|c| c.category == category)
                    .count()
                    + self.cells.iter().filter(|c| c.category == category).count()
            }
            Category::Structure => {
                self.structure.len() + self.cells.iter().filter(|c| c.category == category).count()
            }
            _ => self.cells.iter().filter(|c| c.category == category).count(),
        }
    }
}

fn id_of(parts: &[&str]) -> String {
    let mut h = Fnv::new();
    for p in parts {
        h.str(p).str("\u{1f}");
    }
    format!("x{:016x}", h.finish())
}

/// Which version a reference is read in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Version {
    A,
    B,
}

/// Maps rows and columns of either version to identities in b's space, so
/// formulas from both read the same when they mean the same cells.
struct Space<'a> {
    a: &'a Workbook,
    b: &'a Workbook,
    a_to_b_sheet: HashMap<usize, usize>,
    /// Per sheet of a: matched (row a, row b) and (col a, col b), sorted.
    rows: HashMap<usize, Vec<(u32, u32)>>,
    cols: HashMap<usize, Vec<(u32, u32)>>,
    unmatched_rows: HashMap<usize, HashSet<u32>>,
    unmatched_cols: HashMap<usize, HashSet<u32>>,
}

impl Space<'_> {
    fn sheet_of(
        &self,
        version: Version,
        prefix: Option<&SheetPrefix>,
        own: usize,
    ) -> Option<usize> {
        let wb = if version == Version::A {
            self.a
        } else {
            self.b
        };
        match prefix {
            None => Some(own),
            Some(p) if p.book.is_some() => None,
            Some(p) => wb.sheet_index(&p.first),
        }
    }

    fn sheet_id(&self, version: Version, sheet: usize) -> String {
        match version {
            Version::B => format!("s{sheet}"),
            Version::A => match self.a_to_b_sheet.get(&sheet) {
                Some(b) => format!("s{b}"),
                None => format!("a{sheet}"),
            },
        }
    }

    /// A row (or column) of a in b's numbering: matched ones directly,
    /// others by their offset from the nearest matched one above.
    fn map(pairs: &[(u32, u32)], unmatched: Option<&HashSet<u32>>, n: u32) -> String {
        if unmatched.is_some_and(|u| u.contains(&n)) {
            return format!("a{n}");
        }
        let i = pairs.partition_point(|p| p.0 <= n);
        if i == 0 {
            return n.to_string();
        }
        let (pa, pb) = pairs[i - 1];
        (i64::from(pb) + i64::from(n) - i64::from(pa)).to_string()
    }

    fn row(&self, version: Version, sheet: usize, row: u32) -> String {
        match version {
            Version::B => row.to_string(),
            Version::A => match self.rows.get(&sheet) {
                Some(p) => Self::map(p, self.unmatched_rows.get(&sheet), row),
                None => row.to_string(),
            },
        }
    }

    fn col(&self, version: Version, sheet: usize, col: u32) -> String {
        match version {
            Version::B => col.to_string(),
            Version::A => match self.cols.get(&sheet) {
                Some(p) => Self::map(p, self.unmatched_cols.get(&sheet), col),
                None => col.to_string(),
            },
        }
    }

    fn reference(&self, version: Version, r: &Reference, own: usize) -> String {
        let Some(sheet) = self.sheet_of(version, r.sheet.as_ref(), own) else {
            return format!("ext:{}", super::formula::render_reference(r));
        };
        let mut out = self.sheet_id(version, sheet);
        if let Some(last) = r.sheet.as_ref().and_then(|p| p.last.as_ref()) {
            let wb = if version == Version::A {
                self.a
            } else {
                self.b
            };
            out.push(':');
            out.push_str(
                &wb.sheet_index(last)
                    .map_or_else(|| last.clone(), |s| self.sheet_id(version, s)),
            );
        }
        out.push('!');
        let c = |c: super::formula::Coord| {
            format!(
                "{}C{}",
                if c.abs { "$" } else { "" },
                self.col(version, sheet, c.n)
            )
        };
        let w = |r: super::formula::Coord| {
            format!(
                "{}R{}",
                if r.abs { "$" } else { "" },
                self.row(version, sheet, r.n)
            )
        };
        match r.kind {
            RefKind::Cell { col, row } => out.push_str(&format!("{}{}", w(row), c(col))),
            RefKind::Area { c1, r1, c2, r2 } => {
                out.push_str(&format!("{}{}:{}{}", w(r1), c(c1), w(r2), c(c2)));
            }
            RefKind::Cols { c1, c2 } => out.push_str(&format!("{}:{}", c(c1), c(c2))),
            RefKind::Rows { r1, r2 } => out.push_str(&format!("{}:{}", w(r1), w(r2))),
        }
        if r.spill {
            out.push('#');
        }
        out
    }

    /// A key of what a formula means: references by identity, names and
    /// functions in one case.
    fn canonical(&self, version: Version, formula: &Formula, sheet: usize) -> u64 {
        let text = formula.render_with(|t| match t {
            Token::Ref(r) => Some(self.reference(version, r, sheet)),
            Token::Name { sheet: p, name } => Some(format!(
                "{}{}",
                p.as_ref()
                    .map(render_prefix)
                    .unwrap_or_default()
                    .to_uppercase(),
                name.to_uppercase()
            )),
            Token::Function(f) => Some(f.to_uppercase()),
            _ => None,
        });
        let mut h = Fnv::new();
        h.str(&text);
        h.finish()
    }
}

/// A formula with references written relative to its own cell, so the same
/// formula filled down a column reads the same in every row.
fn relative(formula: &Formula, col: u32, row: u32) -> String {
    let off = |c: super::formula::Coord, at: u32, tag: char| {
        if c.abs {
            format!("{tag}{}", c.n)
        } else {
            format!("{tag}[{}]", i64::from(c.n) - i64::from(at))
        }
    };
    formula.render_with(|t| match t {
        Token::Ref(r) => {
            let p = r.sheet.as_ref().map(render_prefix).unwrap_or_default();
            Some(match r.kind {
                RefKind::Cell { col: c, row: w } => {
                    format!("{p}{}{}", off(w, row, 'R'), off(c, col, 'C'))
                }
                RefKind::Area { c1, r1, c2, r2 } => format!(
                    "{p}{}{}:{}{}",
                    off(r1, row, 'R'),
                    off(c1, col, 'C'),
                    off(r2, row, 'R'),
                    off(c2, col, 'C')
                ),
                RefKind::Cols { c1, c2 } => {
                    format!("{p}{}:{}", off(c1, col, 'C'), off(c2, col, 'C'))
                }
                RefKind::Rows { r1, r2 } => {
                    format!("{p}{}:{}", off(r1, row, 'R'), off(r2, row, 'R'))
                }
            })
        }
        _ => None,
    })
}

/// Where a formula cell's text comes from.
#[derive(Clone, Copy)]
enum Slot<'a> {
    /// Its own text.
    Text(&'a str),
    /// A shared formula's other cell: the master's, moved by an offset.
    Child { si: u32, rows: i64, cols: i64 },
}

/// A sheet's formulas, read lazily. Every formula cell gets a cheap key at
/// once; its text, meaning and relative form are worked out only for cells
/// whose keys differ, because a large model's formulas are nearly all the
/// same in both versions (decision 0025, the stop 1 measurements).
struct Formulas<'a> {
    /// Shared formula masters: si → (row, column, text, tokens).
    masters: HashMap<u32, (u32, u32, &'a str, Formula)>,
    slots: HashMap<(u32, u32), Slot<'a>>,
}

impl<'a> Formulas<'a> {
    fn new(wb: &'a Workbook, sheet: usize) -> Self {
        let text = wb.sheet_text(sheet);
        let mut masters = HashMap::new();
        let mut slots = HashMap::new();
        let Some(s) = wb.sheets.get(sheet) else {
            return Self { masters, slots };
        };
        for row in &s.rows {
            for cell in &row.cells {
                if let Content::Formula(f) = &cell.content
                    && let FormulaKind::Shared { si, .. } = f.kind
                    && let Some(t) = &f.text
                {
                    masters.entry(si).or_insert_with(|| {
                        let t = t.get(text);
                        (row.r, cell.col, t, tokenize(t))
                    });
                }
            }
        }
        for row in &s.rows {
            for cell in &row.cells {
                let Content::Formula(f) = &cell.content else {
                    continue;
                };
                let slot = match (&f.text, &f.kind) {
                    (Some(t), _) => Slot::Text(t.get(text)),
                    (None, FormulaKind::Shared { si, .. }) => match masters.get(si) {
                        Some((r0, c0, _, _)) => Slot::Child {
                            si: *si,
                            rows: i64::from(row.r) - i64::from(*r0),
                            cols: i64::from(cell.col) - i64::from(*c0),
                        },
                        None => continue,
                    },
                    (None, _) => continue,
                };
                slots.insert((row.r, cell.col), slot);
            }
        }
        Self { masters, slots }
    }

    fn has(&self, row: u32, col: u32) -> bool {
        self.slots.contains_key(&(row, col))
    }

    /// Equal for equal text, without rendering anything.
    fn fast(&self, row: u32, col: u32) -> u64 {
        let mut h = Fnv::new();
        match self.slots.get(&(row, col)) {
            Some(Slot::Text(t)) => {
                h.str("t").str(t);
            }
            Some(Slot::Child { si, rows, cols }) => {
                let master = self.masters.get(si).map_or("", |m| m.2);
                h.str("c").str(master).u64(*rows as u64).u64(*cols as u64);
            }
            None => {}
        }
        h.finish()
    }

    fn formula(&self, row: u32, col: u32) -> Option<Formula> {
        match self.slots.get(&(row, col))? {
            Slot::Text(t) => Some(tokenize(t)),
            Slot::Child { si, rows, cols } => {
                self.masters.get(si).map(|m| m.3.shifted(*rows, *cols))
            }
        }
    }

    fn text(&self, row: u32, col: u32) -> Option<String> {
        match self.slots.get(&(row, col))? {
            Slot::Text(t) => Some((*t).to_owned()),
            Slot::Child { .. } => self.formula(row, col).map(|f| f.render()),
        }
    }
}

fn side_of(wb: &Workbook, text: &str, cell: Option<&Cell>, formula: Option<String>) -> CellSide {
    let Some(cell) = cell else {
        return CellSide::default();
    };
    let code = wb.styles.format_code(cell.style);
    let v = value::either(wb, text, cell);
    CellSide {
        shown: display(&v, code, wb.date1904),
        formula: formula.map(|f| format!("={f}")),
    }
}

/// The first text to the left of a column in a row: its label.
fn label_of(wb: &Workbook, text: &str, sheet: usize, row: u32, col: u32) -> Option<String> {
    let r = wb.sheets[sheet].row(row)?;
    r.cells
        .iter()
        .filter(|c| c.col < col)
        .find_map(|c| match value::input(wb, text, c) {
            Some(Value::Text(t)) if !t.trim().is_empty() => Some(t.trim().to_owned()),
            _ => None,
        })
}

fn header_of(
    wb: &Workbook,
    text: &str,
    sheet: usize,
    header: Option<u32>,
    row: u32,
    col: u32,
) -> Option<String> {
    let h = header.filter(|h| *h < row)?;
    let cell = wb.sheets[sheet].cell(col, h)?;
    match value::input(wb, text, cell) {
        Some(Value::Text(t)) if !t.trim().is_empty() => Some(t.trim().to_owned()),
        _ => None,
    }
}

fn hinted(hints: &[Hint], sheet: &str, col: u32, row: u32) -> bool {
    hints
        .iter()
        .any(|h| h.sheet.eq_ignore_ascii_case(sheet) && h.area.contains(col, row))
}

/// Groups numbers into runs of consecutive ones: [3, 4, 5, 9] → [[3, 4, 5], [9]].
fn runs(mut list: Vec<u32>) -> Vec<Vec<u32>> {
    list.sort_unstable();
    let mut out: Vec<Vec<u32>> = Vec::new();
    for n in list {
        match out.last_mut() {
            Some(run) if run.last() == Some(&(n - 1)) => run.push(n),
            _ => out.push(vec![n]),
        }
    }
    out
}

struct Builder<'a> {
    a: &'a Workbook,
    b: &'a Workbook,
    out: WorkbookDiff,
    /// b cells whose change feeds calculated results.
    seeds: Vec<(usize, u32, u32)>,
    refresh_seeds: Vec<(usize, u32, u32)>,
    /// Candidates for knock-on or refreshed: (sheet b, row, col).
    recalculated: Vec<(usize, u32, u32)>,
}

impl Builder<'_> {
    fn structure(&mut self, sheet: &str, kind: StructureKind) {
        let id = id_of(&[sheet, &format!("{kind:?}")]);
        self.out.structure.push(StructureChange {
            id,
            sheet: sheet.to_owned(),
            kind,
        });
    }

    fn sheets(&mut self) {
        let (a, b) = (self.a, self.b);
        for &i in &self.out.sheets.only_a.clone() {
            self.structure(&a.sheets[i].name, StructureKind::SheetRemoved);
        }
        for &j in &self.out.sheets.only_b.clone() {
            self.structure(&b.sheets[j].name, StructureKind::SheetAdded);
            // A new sheet's cells feed whatever reads them.
            for row in &b.sheets[j].rows {
                for cell in &row.cells {
                    self.seeds.push((j, cell.col, row.r));
                }
            }
        }
        let pairs = self.out.sheets.both.clone();
        let order_a: Vec<u64> = pairs.iter().map(|p| p.0 as u64 + 1).collect();
        let mut by_b = pairs.clone();
        by_b.sort_by_key(|p| p.1);
        let order_b: Vec<u64> = by_b.iter().map(|p| p.0 as u64 + 1).collect();
        let kept: HashSet<u64> = lcs_keys(&order_a, &order_b);
        for &(i, j) in &pairs {
            let (sa, sb) = (&a.sheets[i], &b.sheets[j]);
            if sa.name != sb.name {
                self.structure(
                    &sb.name,
                    StructureKind::SheetRenamed {
                        from: sa.name.clone(),
                    },
                );
            }
            if !kept.contains(&(i as u64 + 1)) {
                self.structure(&sb.name, StructureKind::SheetMoved);
            }
            match (sa.state, sb.state) {
                (SheetState::Visible, SheetState::Hidden | SheetState::VeryHidden) => {
                    self.structure(&sb.name, StructureKind::SheetHidden);
                }
                (SheetState::Hidden | SheetState::VeryHidden, SheetState::Visible) => {
                    self.structure(&sb.name, StructureKind::SheetShown);
                }
                _ => {}
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn sheet(
        &mut self,
        i: usize,
        j: usize,
        space: &Space<'_>,
        fa: &Formulas<'_>,
        fb: &Formulas<'_>,
        hints: &[Hint],
    ) {
        let (a, b) = (self.a, self.b);
        let (sa, sb) = (&a.sheets[i], &b.sheets[j]);
        let (ta, tb) = (a.sheet_text(i), b.sheet_text(j));
        let al = self.out.alignments[&(i, j)].clone();
        let name = sb.name.clone();
        // Data a connection loads, and pivot tables' output: what changes
        // there is refreshed, never typed (decisions 0022, 0029).
        let query_b: Vec<Area> = b
            .query_areas
            .iter()
            .chain(&b.pivot_areas)
            .filter(|(s, _)| *s == j)
            .map(|(_, a)| *a)
            .collect();
        let in_query = |col: u32, row: u32| query_b.iter().any(|q| q.contains(col, row));
        let pivot_b: Vec<Area> = b
            .pivot_areas
            .iter()
            .filter(|(s, _)| *s == j)
            .map(|(_, a)| *a)
            .collect();
        let in_pivot = |col: u32, row: u32| pivot_b.iter().any(|p| p.contains(col, row));
        let pivot_rows = |wb: &Workbook, sheet: usize, row: u32| {
            wb.pivot_areas
                .iter()
                .any(|(s, a)| *s == sheet && (a.row1..=a.row2).contains(&row))
        };

        // Rows.
        let mut hidden = Vec::new();
        let mut shown = Vec::new();
        let mut row_format = Vec::new();
        for &(ra, rb) in &al.rows.pairs {
            let (Some(row_a), Some(row_b)) = (sa.row(ra), sb.row(rb)) else {
                continue;
            };
            match (row_a.hidden, row_b.hidden) {
                (false, true) => hidden.push(rb),
                (true, false) => shown.push(rb),
                _ => {}
            }
            if row_a.format != row_b.format {
                row_format.push(rb);
            }
            let mut cells: BTreeMap<u32, (Option<&Cell>, Option<&Cell>)> = BTreeMap::new();
            for c in &row_a.cells {
                if let Some(cb) = al.cols.a_to_b.get(&c.col) {
                    cells.entry(*cb).or_default().0 = Some(c);
                }
            }
            for c in &row_b.cells {
                if al.cols.b_to_a.contains_key(&c.col) {
                    cells.entry(c.col).or_default().1 = Some(c);
                }
            }
            for (cb, (x, y)) in cells {
                let ca = al.cols.b_to_a[&cb];
                let fx = fa.has(ra, ca);
                let fy = fb.has(rb, cb);
                let vx = x.map_or(Value::Blank, |c| value::either(a, ta, c));
                let vy = y.map_or(Value::Blank, |c| value::either(b, tb, c));
                let style_x = x.map_or(a.styles.key(0), |c| a.styles.key(c.style));
                let style_y = y.map_or(b.styles.key(0), |c| b.styles.key(c.style));
                // Excel formats a pivot table's cells itself as it lays them
                // out: not someone's formatting (decision 0029).
                let format_changed = style_x != style_y && !in_pivot(cb, rb);
                let category = match (fx, fy) {
                    (false, false) => {
                        if vx.key() != vy.key() {
                            Some(if in_query(cb, rb) {
                                Category::Refreshed
                            } else {
                                Category::Input
                            })
                        } else if format_changed {
                            Some(Category::Structure)
                        } else {
                            None
                        }
                    }
                    (true, true) => {
                        // The same text, or the same meaning. Excel rewrites
                        // references when rows are inserted (the meaning
                        // stays), but not when rows are sorted (the text
                        // stays).
                        let same = fa.fast(ra, ca) == fb.fast(rb, cb) || {
                            match (fa.formula(ra, ca), fb.formula(rb, cb)) {
                                (Some(p), Some(q)) => {
                                    p.render() == q.render()
                                        || space.canonical(Version::A, &p, i)
                                            == space.canonical(Version::B, &q, j)
                                }
                                _ => false,
                            }
                        };
                        if !same {
                            Some(Category::Formula)
                        } else if vx.shown_key() != vy.shown_key() {
                            self.recalculated.push((j, rb, cb));
                            if format_changed {
                                Some(Category::Structure)
                            } else {
                                None
                            }
                        } else if format_changed {
                            Some(Category::Structure)
                        } else {
                            None
                        }
                    }
                    _ => Some(Category::Formula),
                };
                let Some(category) = category else {
                    continue;
                };
                if matches!(category, Category::Input | Category::Formula) {
                    self.seeds.push((j, cb, rb));
                }
                if category == Category::Refreshed {
                    self.refresh_seeds.push((j, cb, rb));
                }
                let provenance = match category {
                    Category::Refreshed => Provenance::Refresh,
                    _ if hinted(hints, &name, cb, rb) => Provenance::DirectEdit,
                    _ => Provenance::Unknown,
                };
                let before = side_of(a, ta, x, fa.text(ra, ca));
                let after = side_of(b, tb, y, fb.text(rb, cb));
                let group = if category == Category::Formula {
                    let rel = |f: Option<Formula>, c: u32, r: u32| {
                        f.map(|f| relative(&f, c, r)).unwrap_or_default()
                    };
                    let mut h = Fnv::new();
                    h.u64(j as u64)
                        .u64(u64::from(cb))
                        .str(&rel(fa.formula(ra, ca), ca, ra))
                        .str("→")
                        .str(&rel(fb.formula(rb, cb), cb, rb));
                    h.finish()
                } else {
                    0
                };
                let id = id_of(&[
                    &name,
                    &format!("{category:?}"),
                    &a1(cb, rb),
                    &before.shown,
                    before.formula.as_deref().unwrap_or(""),
                    &after.shown,
                    after.formula.as_deref().unwrap_or(""),
                ]);
                self.out.cells.push(CellChange {
                    id,
                    category,
                    provenance,
                    sheet: name.clone(),
                    sheet_a: i,
                    sheet_b: j,
                    at_a: (ca, ra),
                    at_b: (cb, rb),
                    before,
                    after,
                    format_changed,
                    group,
                    label: label_of(b, tb, j, rb, cb),
                    header: header_of(b, tb, j, al.header.1, rb, cb),
                });
            }
        }
        for run in runs(hidden) {
            self.structure(&name, StructureKind::RowsHidden { rows: run });
        }
        for run in runs(shown) {
            self.structure(&name, StructureKind::RowsShown { rows: run });
        }
        if !row_format.is_empty() {
            self.structure(&name, StructureKind::RowFormat { rows: row_format });
        }

        // Rows moved: a formula reading a moved row's address now reads
        // another record.
        for &rb in &al.rows.moved_b {
            if let Some(row) = sb.row(rb) {
                for c in &row.cells {
                    self.seeds.push((j, c.col, rb));
                }
            }
        }
        // Rows added and removed: their cells feed calculated results.
        for &rb in &al.rows.only_b {
            if let Some(row) = sb.row(rb) {
                for c in &row.cells {
                    self.seeds.push((j, c.col, rb));
                }
            }
        }
        for &ra in &al.rows.only_a {
            // A removed row changes what ranges around it hold.
            // Pairs are in a's row order.
            let at = al.rows.pairs.partition_point(|p| p.0 < ra);
            let below = al.rows.pairs.get(at).map(|p| p.1);
            let above = at
                .checked_sub(1)
                .and_then(|i| al.rows.pairs.get(i))
                .map(|p| p.1);
            for rb in [below, above].into_iter().flatten() {
                for &(_, cb) in &al.cols.pairs {
                    self.seeds.push((j, cb, rb));
                }
            }
        }
        // A pivot table grows and shrinks as it refreshes: its rows aren't
        // rows someone added.
        let added: Vec<u32> = al
            .rows
            .only_b
            .iter()
            .copied()
            .filter(|r| !pivot_rows(b, j, *r))
            .collect();
        let removed: Vec<u32> = al
            .rows
            .only_a
            .iter()
            .copied()
            .filter(|r| !pivot_rows(a, i, *r))
            .collect();
        for run in runs(added) {
            self.structure(&name, StructureKind::RowsAdded { rows: run });
        }
        for run in runs(removed) {
            self.structure(&name, StructureKind::RowsRemoved { rows: run });
        }
        if let Some((col, descending)) = al.sorted_by {
            let by = header_of(b, tb, j, al.header.1, u32::MAX, col)
                .unwrap_or_else(|| format!("column {}", col_name(col)));
            self.structure(&name, StructureKind::RowsSorted { by, descending });
        } else if !al.rows.moved_b.is_empty() {
            self.structure(
                &name,
                StructureKind::RowsMoved {
                    rows: al.rows.moved_b.clone(),
                },
            );
        }
        if !al.rows.unsure_a.is_empty() || !al.rows.unsure_b.is_empty() {
            self.out.unmatched.push(Unmatched {
                sheet: name.clone(),
                rows_a: al.rows.unsure_a.clone(),
                rows_b: al.rows.unsure_b.clone(),
            });
        }

        // Columns.
        self.columns(&name, j, &al.cols, sa, sb);

        // Everything else about the sheet.
        let ka = &sa.keys;
        let kb = &sb.keys;
        let pairs = [
            (
                ka.conditional != kb.conditional,
                StructureKind::ConditionalFormats,
            ),
            (
                ka.validation != kb.validation,
                StructureKind::DataValidation,
            ),
            (ka.protection != kb.protection, StructureKind::Protection),
            (ka.hyperlinks != kb.hyperlinks, StructureKind::Hyperlinks),
            (ka.filter != kb.filter, StructureKind::Filter),
            (ka.page != kb.page, StructureKind::PageSetup),
            (
                ka.format != kb.format || ka.other != kb.other,
                StructureKind::SheetSettings,
            ),
        ];
        for (changed, kind) in pairs {
            if changed {
                self.structure(&name, kind);
            }
        }
        let map_area = |area: &Area| -> String {
            format!(
                "{}:{}:{}:{}",
                space.row(Version::A, i, area.row1),
                space.col(Version::A, i, area.col1),
                space.row(Version::A, i, area.row2),
                space.col(Version::A, i, area.col2)
            )
        };
        let merged_a: HashSet<String> = sa.merged.iter().map(map_area).collect();
        let merged_b: HashSet<String> = sb
            .merged
            .iter()
            .map(|m| format!("{}:{}:{}:{}", m.row1, m.col1, m.row2, m.col2))
            .collect();
        if merged_a != merged_b {
            self.structure(&name, StructureKind::MergedCells);
        }
    }

    fn columns(
        &mut self,
        name: &str,
        j: usize,
        cols: &Matching,
        sa: &super::model::Sheet,
        sb: &super::model::Sheet,
    ) {
        let col_info = |s: &super::model::Sheet, c: u32| {
            s.cols
                .iter()
                .find(|ci| (ci.min..=ci.max).contains(&c))
                .map(|ci| (ci.hidden, ci.format))
        };
        let (mut hidden, mut shown, mut format) = (Vec::new(), Vec::new(), Vec::new());
        for &(ca, cb) in &cols.pairs {
            let (x, y) = (col_info(sa, ca), col_info(sb, cb));
            let (hx, fx) = x.unwrap_or((false, 0));
            let (hy, fy) = y.unwrap_or((false, 0));
            if !hx && hy {
                hidden.push(cb);
            } else if hx && !hy {
                shown.push(cb);
            }
            // Excel fits a pivot table's columns to it each time it updates.
            let under_pivot = self
                .b
                .pivot_areas
                .iter()
                .any(|(s, a)| *s == j && (a.col1..=a.col2).contains(&cb));
            if fx != fy && !under_pivot {
                format.push(cb);
            }
        }
        for run in runs(cols.only_b.clone()) {
            for &c in &run {
                for row in &sb.rows {
                    if row.cells.iter().any(|x| x.col == c) {
                        self.seeds.push((j, c, row.r));
                    }
                }
            }
            self.structure(name, StructureKind::ColumnsAdded { cols: run });
        }
        for run in runs(cols.only_a.clone()) {
            self.structure(name, StructureKind::ColumnsRemoved { cols: run });
        }
        for run in runs(hidden) {
            self.structure(name, StructureKind::ColumnsHidden { cols: run });
        }
        for run in runs(shown) {
            self.structure(name, StructureKind::ColumnsShown { cols: run });
        }
        if !format.is_empty() {
            self.structure(name, StructureKind::ColumnFormat { cols: format });
        }
    }

    fn workbook(&mut self, space: &Space<'_>) {
        let (a, b) = (self.a, self.b);
        // Names, except the ones Excel makes itself.
        let key = |n: &super::model::DefinedName, wb: &Workbook| {
            (
                n.name.to_lowercase(),
                n.local.map(|l| {
                    wb.sheets
                        .get(l)
                        .map_or(String::new(), |s| s.name.to_lowercase())
                }),
            )
        };
        let names_a: BTreeMap<_, _> = a
            .names
            .iter()
            .filter(|n| !n.is_builtin())
            .map(|n| (key(n, a), n))
            .collect();
        let names_b: BTreeMap<_, _> = b
            .names
            .iter()
            .filter(|n| !n.is_builtin())
            .map(|n| (key(n, b), n))
            .collect();
        for (k, n) in &names_a {
            match names_b.get(k) {
                None => self.structure(
                    "",
                    StructureKind::NameRemoved {
                        name: n.name.clone(),
                    },
                ),
                Some(m) => {
                    let fa = super::formula::tokenize(&n.refers_to);
                    let fb = super::formula::tokenize(&m.refers_to);
                    let own_a = n.local.unwrap_or(0);
                    let own_b = m.local.unwrap_or(0);
                    if space.canonical(Version::A, &fa, own_a)
                        != space.canonical(Version::B, &fb, own_b)
                    {
                        self.structure(
                            "",
                            StructureKind::NameChanged {
                                name: m.name.clone(),
                                from: n.refers_to.clone(),
                                to: m.refers_to.clone(),
                            },
                        );
                    }
                }
            }
        }
        for (k, n) in &names_b {
            if !names_a.contains_key(k) {
                self.structure(
                    "",
                    StructureKind::NameAdded {
                        name: n.name.clone(),
                    },
                );
            }
        }
        // Tables, by name.
        for t in &a.tables {
            match b
                .tables
                .iter()
                .find(|u| u.name.eq_ignore_ascii_case(&t.name))
            {
                None => self.structure(
                    &a.sheets[t.sheet].name,
                    StructureKind::TableRemoved {
                        name: t.name.clone(),
                    },
                ),
                Some(u) if u.key != t.key || u.columns != t.columns => {
                    self.structure(
                        &b.sheets[u.sheet].name,
                        StructureKind::TableChanged {
                            name: u.name.clone(),
                        },
                    );
                }
                Some(_) => {}
            }
        }
        for u in &b.tables {
            if !a
                .tables
                .iter()
                .any(|t| t.name.eq_ignore_ascii_case(&u.name))
            {
                self.structure(
                    &b.sheets[u.sheet].name,
                    StructureKind::TableAdded {
                        name: u.name.clone(),
                    },
                );
            }
        }
        for c in &b.connections {
            if a.connections
                .iter()
                .find(|d| d.name == c.name)
                .is_none_or(|d| d.key != c.key)
            {
                self.structure(
                    "",
                    StructureKind::ConnectionChanged {
                        name: c.name.clone(),
                    },
                );
            }
        }
        if a.settings != b.settings {
            self.structure("", StructureKind::WorkbookSettings);
        }
        if a.calc != b.calc {
            self.structure("", StructureKind::Calculation);
        }
        // Parts kept but not tracked: changed if their bytes changed.
        let mut kinds: Vec<UnsupportedKind> = Vec::new();
        for u in &b.unsupported {
            let before = a.package.part(&u.part);
            if before != b.package.part(&u.part) && !kinds.contains(&u.kind) {
                kinds.push(u.kind);
            }
        }
        for u in &a.unsupported {
            if b.package.part(&u.part).is_none() && !kinds.contains(&u.kind) {
                kinds.push(u.kind);
            }
        }
        kinds.sort();
        for kind in kinds {
            self.structure("", StructureKind::Untracked { kind });
        }
    }

    /// Sorts every changed calculated result into knock-on or refreshed.
    fn calculated(&mut self) {
        if self.recalculated.is_empty() {
            return;
        }
        let b = self.b;
        let deps = Deps::build(b);
        let direct = deps.downstream(self.seeds.iter().copied());
        // What can change by itself: refreshed inputs, volatile and external
        // formulas.
        let mut spontaneous: Vec<(usize, u32, u32)> = self.refresh_seeds.clone();
        let mut dynamic_seeds = Vec::new();
        for c in &deps.cells {
            if c.volatile || c.external {
                spontaneous.push((c.sheet, c.col, c.row));
            }
            if c.dynamic {
                dynamic_seeds.push((c.sheet, c.col, c.row));
            }
        }
        let refreshed = deps.downstream(spontaneous);
        let dynamic = deps.downstream(dynamic_seeds);
        let query = |sheet: usize, col: u32, row: u32| {
            b.query_areas
                .iter()
                .any(|(s, a)| *s == sheet && a.contains(col, row))
        };
        let mut readers = Vec::new();
        for &(sheet, row, col) in &self.recalculated {
            let Some(&id) = deps.at.get(&(sheet, row, col)) else {
                continue;
            };
            let f = &deps.cells[id];
            let (category, provenance) = if f.volatile || f.external || query(sheet, col, row) {
                (Category::Refreshed, Provenance::Refresh)
            } else if direct[id] {
                if refreshed[id] || f.dynamic || dynamic[id] {
                    (Category::KnockOn, Provenance::LikelyDownstream)
                } else {
                    (Category::KnockOn, Provenance::Downstream)
                }
            } else if refreshed[id] {
                (Category::Refreshed, Provenance::Refresh)
            } else {
                (Category::KnockOn, Provenance::Unknown)
            };
            readers.clear();
            deps.readers(sheet, col, row, &mut readers);
            self.out.calculated.push(Calculated {
                category,
                provenance,
                sheet_b: sheet,
                col,
                row,
                terminal: readers.is_empty(),
            });
        }
    }
}

/// Keys in the longest common subsequence of two orders.
fn lcs_keys(a: &[u64], b: &[u64]) -> HashSet<u64> {
    let (n, m) = (a.len(), b.len());
    let mut t = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[i][j] = if a[i] == b[j] {
                t[i + 1][j + 1] + 1
            } else {
                t[i + 1][j].max(t[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = HashSet::new();
    while i < n && j < m {
        if a[i] == b[j] {
            out.insert(a[i]);
            i += 1;
            j += 1;
        } else if t[i + 1][j] >= t[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// Everything that changed from `a` to `b`. `hints` are ranges the open
/// workbook reported as edited in this session; pass none for files.
#[must_use]
pub fn diff(a: &Workbook, b: &Workbook, hints: &[Hint]) -> WorkbookDiff {
    let sheets = pair_sheets(a, b);
    let mut alignments = HashMap::new();
    for &(i, j) in &sheets.both {
        if a.sheets[i].kind == SheetKind::Worksheet {
            alignments.insert((i, j), align(a, i, b, j));
        }
    }
    let mut space = Space {
        a,
        b,
        a_to_b_sheet: sheets.both.iter().copied().collect(),
        rows: HashMap::new(),
        cols: HashMap::new(),
        unmatched_rows: HashMap::new(),
        unmatched_cols: HashMap::new(),
    };
    for (&(i, _), al) in &alignments {
        let mut rows = al.rows.pairs.clone();
        rows.sort_unstable();
        space.rows.insert(i, rows);
        let mut cols = al.cols.pairs.clone();
        cols.sort_unstable();
        space.cols.insert(i, cols);
        space.unmatched_rows.insert(
            i,
            al.rows
                .only_a
                .iter()
                .chain(&al.rows.unsure_a)
                .copied()
                .collect(),
        );
        space
            .unmatched_cols
            .insert(i, al.cols.only_a.iter().copied().collect());
    }
    let mut builder = Builder {
        a,
        b,
        out: WorkbookDiff {
            sheets: sheets.clone(),
            alignments,
            ..WorkbookDiff::default()
        },
        seeds: Vec::new(),
        refresh_seeds: Vec::new(),
        recalculated: Vec::new(),
    };
    builder.sheets();
    let mut pairs: Vec<(usize, usize)> = builder.out.alignments.keys().copied().collect();
    pairs.sort_unstable();
    for (i, j) in pairs {
        let fa = Formulas::new(a, i);
        let fb = Formulas::new(b, j);
        builder.sheet(i, j, &space, &fa, &fb, hints);
    }
    builder.workbook(&space);
    builder.calculated();
    builder.objects();
    builder.out
}

impl Builder<'_> {
    /// Macros, queries, pivot tables and slicers.
    fn objects(&mut self) {
        let (a, b) = (self.a, self.b);
        let unreadable =
            |w: &Workbook, kind: UnsupportedKind| w.unsupported.iter().any(|u| u.kind == kind);
        if !unreadable(a, UnsupportedKind::Macros) && !unreadable(b, UnsupportedKind::Macros) {
            self.out.macros = crate::vba::diff::diff(a.vba.as_ref(), b.vba.as_ref());
        }
        if !unreadable(a, UnsupportedKind::PowerQuery)
            && !unreadable(b, UnsupportedKind::PowerQuery)
        {
            self.out.queries = super::query::diff(a.queries.as_ref(), b.queries.as_ref());
        }
        let names = |w: &Workbook| w.sheets.iter().map(|s| s.name.clone()).collect::<Vec<_>>();
        let pairs = self.out.sheets.both.clone();
        let map = |i: usize| pairs.iter().find(|p| p.0 == i).map(|p| p.1);
        let (mut pivots, slicers) =
            super::pivot::diff(&a.pivots, &b.pivots, &names(a), &names(b), &map);
        for p in &mut pivots {
            if let (Some(area), Some(j)) = (p.area, b.sheet_index(&p.sheet)) {
                p.cells = self
                    .out
                    .cells
                    .iter()
                    .filter(|c| {
                        c.category == Category::Refreshed
                            && c.sheet_b == j
                            && area.contains(c.at_b.0, c.at_b.1)
                    })
                    .count();
            }
        }
        self.out.pivots = pivots;
        self.out.slicers = slicers;
    }
}

#[cfg(test)]
mod tests;
