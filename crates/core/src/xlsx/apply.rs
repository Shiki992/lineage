// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! How accepted changes land in a workbook that is open in Excel (decision
//! 0023). Office.js can't replace an open workbook, so Rust produces an
//! **apply plan**: rows to insert and cells to set, in the order the task
//! pane carries them out. Each plan carries what its target cells hold
//! before (for the pane to check first), an exact way back, and the
//! workbook it should leave, so [`landed`] can check what Excel holds
//! afterwards.
//!
//! Rows are inserted bottom-up, so each insert's address is still right
//! when it runs. Cells are then set at their final addresses. Formulas are
//! written in en-US, with every reference turned into this version's own
//! rows and columns through alignment.

use std::collections::{BTreeMap, HashMap};

use super::Workbook;
use super::deps::each_formula;
use super::diff::{CellChange, WorkbookDiff};
use super::formula::{
    Coord, Formula, RefKind, Reference, SheetPrefix, Token, render_prefix, tokenize,
};
use super::merge::{Plan, Status, What};
use super::model::Content;
use super::refs::{MAX_COL, MAX_ROW, number_key};
use super::value::{self, Value};
use crate::error::{Error, Result};
use crate::merge::Decision;

/// What a cell is set to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Write {
    Clear,
    /// A number, by its lexeme: written as typed text, so Excel parses it
    /// exactly as it would read it from a file.
    Number(String),
    /// Text, exactly: never turned into a number or a formula.
    Text(String),
    Bool(bool),
    /// An error value such as `#N/A`.
    Error(String),
    /// A formula in en-US, without `=`.
    Formula(String),
}

/// One step for the pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    /// Insert `count` whole rows at row `at` of `sheet`, pushing row `at`
    /// and below down.
    InsertRows { sheet: String, at: u32, count: u32 },
    /// Delete `count` whole rows from row `at`.
    DeleteRows { sheet: String, at: u32, count: u32 },
    Set {
        sheet: String,
        col: u32,
        row: u32,
        value: Write,
    },
}

/// What a cell must hold before the plan runs, at its address then.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expect {
    pub sheet: String,
    pub col: u32,
    pub row: u32,
    pub value: Write,
}

/// An accepted change the plan couldn't write, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Skipped {
    pub id: String,
    pub reason: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ApplyPlan {
    pub steps: Vec<Step>,
    /// Run in order after a failure, to put the workbook back.
    pub undo: Vec<Step>,
    pub expect: Vec<Expect>,
    /// Items brought in, by id.
    pub brought: Vec<String>,
    pub skipped: Vec<Skipped>,
    /// Rows inserted per sheet of ours: (at, count), in ours' rows before
    /// the plan.
    pub inserts: BTreeMap<String, Vec<(u32, u32)>>,
}

impl ApplyPlan {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// Where a row of ours ends up after the plan's inserts.
fn final_row(inserts: &[(u32, u32)], row: u32) -> u32 {
    row + inserts
        .iter()
        .filter(|(at, _)| *at <= row)
        .map(|(_, n)| *n)
        .sum::<u32>()
}

/// The first row of an inserted block after every insert.
fn block_start(inserts: &[(u32, u32)], index: usize) -> u32 {
    let (at, _) = inserts[index];
    at + inserts
        .iter()
        .enumerate()
        .filter(|(i, (a, _))| *a < at || (*a == at && *i < index))
        .map(|(_, (_, n))| *n)
        .sum::<u32>()
}

/// A cell's content as a write, from the workbook it is in.
fn write_of(wb: &Workbook, sheet: usize, col: u32, row: u32, formula: Option<String>) -> Write {
    if let Some(f) = formula {
        return Write::Formula(f);
    }
    let text = wb.sheet_text(sheet);
    let Some(cell) = wb.sheets.get(sheet).and_then(|s| s.cell(col, row)) else {
        return Write::Clear;
    };
    match value::input(wb, text, cell) {
        Some(Value::Number(n)) => Write::Number(n.to_owned()),
        Some(Value::Text(t)) => Write::Text(t.to_owned()),
        Some(Value::Bool(b)) => Write::Bool(b),
        Some(Value::Error(e)) => Write::Error(e.to_owned()),
        Some(Value::Date(d)) => Write::Text(d.to_owned()),
        Some(Value::Blank) | None => Write::Clear,
    }
}

/// Maps rows (or columns) of one version to final rows of ours, by exact
/// match, or by offset from the nearest mapped one above.
#[derive(Default)]
struct Axis {
    /// (from, to), sorted by from.
    pairs: Vec<(u32, u32)>,
    /// Rows that exist on the source side but have no place in ours.
    gone: Vec<u32>,
}

impl Axis {
    fn map(&self, n: u32, max: u32) -> Option<u32> {
        if self.gone.binary_search(&n).is_ok() {
            return None;
        }
        let i = self.pairs.partition_point(|p| p.0 <= n);
        let to = if i == 0 {
            i64::from(n)
        } else {
            let (from, to) = self.pairs[i - 1];
            i64::from(to) + i64::from(n) - i64::from(from)
        };
        u32::try_from(to).ok().filter(|t| (1..=max).contains(t))
    }
}

/// Renders a formula read in one version into ours after the plan.
fn render(formula: &Formula, own_sheet: &str, maps: &Maps) -> Option<String> {
    let mut failed = false;
    let text = formula.render_with(|t| {
        let Token::Ref(r) = t else {
            return None;
        };
        if r.sheet.as_ref().is_some_and(|p| p.book.is_some()) {
            return None;
        }
        let source = r.sheet.as_ref().map_or(own_sheet, |p| p.first.as_str());
        let (Some(target), Some(row_axis), Some(col_axis)) =
            (maps.name(source), maps.rows(source), maps.cols(source))
        else {
            failed = true;
            return None;
        };
        let mr = |c: Coord| row_axis.map(c.n, MAX_ROW).map(|n| Coord { n, abs: c.abs });
        let mc = |c: Coord| col_axis.map(c.n, MAX_COL).map(|n| Coord { n, abs: c.abs });
        let kind = match r.kind {
            RefKind::Cell { col, row } => mc(col)
                .zip(mr(row))
                .map(|(col, row)| RefKind::Cell { col, row }),
            RefKind::Area { c1, r1, c2, r2 } => match (mc(c1), mr(r1), mc(c2), mr(r2)) {
                (Some(c1), Some(r1), Some(c2), Some(r2)) => Some(RefKind::Area { c1, r1, c2, r2 }),
                _ => None,
            },
            RefKind::Cols { c1, c2 } => mc(c1).zip(mc(c2)).map(|(c1, c2)| RefKind::Cols { c1, c2 }),
            RefKind::Rows { r1, r2 } => mr(r1).zip(mr(r2)).map(|(r1, r2)| RefKind::Rows { r1, r2 }),
        };
        let Some(kind) = kind else {
            failed = true;
            return None;
        };
        let sheet = r.sheet.as_ref().map(|p| SheetPrefix {
            book: None,
            first: target.clone(),
            last: p.last.as_ref().and_then(|l| maps.name(l)),
        });
        Some(super::formula::render_reference(&Reference {
            sheet,
            kind,
            spill: r.spill,
        }))
    });
    (!failed).then_some(text)
}

/// Where every sheet, row and column of theirs ends up in ours after the
/// plan.
struct Maps {
    /// Theirs sheet name → axes to ours' final rows and columns.
    theirs_rows: HashMap<String, Axis>,
    theirs_cols: HashMap<String, Axis>,
    /// Theirs sheet name → ours sheet name.
    theirs_to_ours: HashMap<String, String>,
}

impl Maps {
    /// The sheet of ours a sheet of theirs is.
    fn name(&self, theirs: &str) -> Option<String> {
        self.theirs_to_ours.get(&theirs.to_lowercase()).cloned()
    }

    fn rows(&self, theirs: &str) -> Option<&Axis> {
        self.theirs_rows.get(&theirs.to_lowercase())
    }

    fn cols(&self, theirs: &str) -> Option<&Axis> {
        self.theirs_cols.get(&theirs.to_lowercase())
    }
}

fn diff_pairs(d: &WorkbookDiff, a: usize, b: usize, rows: bool) -> Vec<(u32, u32)> {
    d.alignments
        .get(&(a, b))
        .map(|al| {
            if rows {
                al.rows.pairs.clone()
            } else {
                al.cols.pairs.clone()
            }
        })
        .unwrap_or_default()
}

/// The apply plan for these decisions (item id → decision). Items without
/// a decision stay as they are.
///
/// # Errors
/// [`Error::CannotEdit`] when a decision doesn't fit its item.
#[allow(clippy::too_many_lines)]
pub fn apply_plan(p: &Plan<'_>, decisions: &[(String, Decision)]) -> Result<ApplyPlan> {
    let mut out = ApplyPlan::default();
    let decided: HashMap<&str, Decision> =
        decisions.iter().map(|(id, d)| (id.as_str(), *d)).collect();
    for (id, d) in decisions {
        let item = p
            .item(id)
            .ok_or_else(|| Error::CannotEdit(format!("no item {id}")))?;
        if !item.choices().contains(d) {
            return Err(Error::CannotEdit(format!("{d:?} isn't a choice for {id}")));
        }
    }

    // 1. Rows to insert: (ours sheet, at, theirs sheet, theirs rows, id).
    let mut blocks: Vec<(usize, u32, usize, Vec<u32>, String)> = Vec::new();
    for item in &p.items {
        let What::Rows { sheet_theirs, rows } = &item.what else {
            continue;
        };
        let Some(d) = decided.get(item.id.as_str()) else {
            continue;
        };
        if !d.brings_in() {
            continue;
        }
        let Some(base_sheet) = p.base_of_theirs(*sheet_theirs) else {
            continue;
        };
        let Some(ours_sheet) = p.ours_sheet(base_sheet) else {
            continue;
        };
        let Some(anchor) =
            super::merge::anchor_above(&p.master, base_sheet, *sheet_theirs, rows[0])
        else {
            continue;
        };
        let Some(al) = p.mine.alignments.get(&(base_sheet, ours_sheet)) else {
            continue;
        };
        let Some(&ours_anchor) = al.rows.a_to_b.get(&anchor) else {
            continue;
        };
        // After this version's own rows added there, unless the master's
        // come first.
        let mut after = ours_anchor;
        if matches!(
            d,
            Decision::KeepBoth {
                masters_first: false
            }
        ) {
            while al.rows.only_b.contains(&(after + 1)) {
                after += 1;
            }
        }
        blocks.push((
            ours_sheet,
            after + 1,
            *sheet_theirs,
            rows.clone(),
            item.id.clone(),
        ));
    }
    let mut inserts: BTreeMap<String, Vec<(u32, u32)>> = BTreeMap::new();
    for (s, at, _, rows, _) in &blocks {
        let name = p.ours.sheets[*s].name.clone();
        inserts
            .entry(name)
            .or_default()
            .push((*at, u32::try_from(rows.len()).unwrap_or(0)));
    }

    // 2. Axes: ours rows to final rows, theirs rows to final rows.
    let mut maps = Maps {
        theirs_rows: HashMap::new(),
        theirs_cols: HashMap::new(),
        theirs_to_ours: HashMap::new(),
    };
    for (j, ts) in p.theirs.sheets.iter().enumerate() {
        let Some(base_sheet) = p.base_of_theirs(j) else {
            continue;
        };
        let Some(ours_sheet) = p.ours_sheet(base_sheet) else {
            continue;
        };
        let ours_name = p.ours.sheets[ours_sheet].name.clone();
        let ins = inserts.get(&ours_name).cloned().unwrap_or_default();
        // theirs row → base row → ours row → final row.
        let base_theirs: HashMap<u32, u32> = diff_pairs(&p.master, base_sheet, j, true)
            .into_iter()
            .map(|(a, b)| (b, a))
            .collect();
        let base_ours: HashMap<u32, u32> = diff_pairs(&p.mine, base_sheet, ours_sheet, true)
            .into_iter()
            .collect();
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        let mut gone: Vec<u32> = Vec::new();
        for row in &ts.rows {
            match base_theirs.get(&row.r).and_then(|br| base_ours.get(br)) {
                Some(or) => pairs.push((row.r, final_row(&ins, *or))),
                None => gone.push(row.r),
            }
        }
        // Rows of theirs being brought in.
        let order: Vec<usize> = (0..blocks.len()).collect();
        let ours_blocks: Vec<(u32, u32)> = ins.clone();
        for &k in &order {
            let (s, at, tj, rows, _) = &blocks[k];
            if *s != ours_sheet || *tj != j {
                continue;
            }
            let index = ours_blocks
                .iter()
                .position(|(a, n)| *a == *at && *n == u32::try_from(rows.len()).unwrap_or(0))
                .unwrap_or(0);
            let start = block_start(&ours_blocks, index);
            for (i, r) in rows.iter().enumerate() {
                pairs.push((*r, start + u32::try_from(i).unwrap_or(0)));
                gone.retain(|g| g != r);
            }
        }
        pairs.sort_unstable();
        gone.sort_unstable();
        maps.theirs_rows
            .insert(ts.name.to_lowercase(), Axis { pairs, gone });
        // Columns: theirs → base → ours.
        let base_theirs_c: HashMap<u32, u32> = diff_pairs(&p.master, base_sheet, j, false)
            .into_iter()
            .map(|(a, b)| (b, a))
            .collect();
        let base_ours_c: HashMap<u32, u32> = diff_pairs(&p.mine, base_sheet, ours_sheet, false)
            .into_iter()
            .collect();
        let mut cpairs: Vec<(u32, u32)> = base_theirs_c
            .iter()
            .filter_map(|(t, b)| base_ours_c.get(b).map(|o| (*t, *o)))
            .collect();
        cpairs.sort_unstable();
        maps.theirs_cols.insert(
            ts.name.to_lowercase(),
            Axis {
                pairs: cpairs,
                gone: Vec::new(),
            },
        );
        maps.theirs_to_ours
            .insert(ts.name.to_lowercase(), ours_name);
    }

    let theirs_formulas = |sheet: usize| -> HashMap<(u32, u32), Formula> {
        let mut m = HashMap::new();
        each_formula(p.theirs, sheet, |r, c, f, _| {
            m.insert((r, c), f.into_owned());
        });
        m
    };
    let mut theirs_cache: HashMap<usize, HashMap<(u32, u32), Formula>> = HashMap::new();

    // 3. Steps: inserts bottom-up, per sheet.
    for (sheet, list) in &inserts {
        let mut sorted = list.clone();
        sorted.sort_by_key(|x| std::cmp::Reverse(x.0));
        for (at, count) in sorted {
            out.steps.push(Step::InsertRows {
                sheet: sheet.clone(),
                at,
                count,
            });
        }
    }

    let mut sets: Vec<Step> = Vec::new();
    // The cells of inserted rows.
    for (s, _, tj, rows, id) in &blocks {
        let ours_name = p.ours.sheets[*s].name.clone();
        let tsheet = &p.theirs.sheets[*tj];
        let formulas = theirs_cache
            .entry(*tj)
            .or_insert_with(|| theirs_formulas(*tj));
        let mut ok = true;
        let mut cells = Vec::new();
        for r in rows {
            let Some(row) = tsheet.row(*r) else {
                continue;
            };
            let Some(final_r) = maps.rows(&tsheet.name).and_then(|a| a.map(*r, MAX_ROW)) else {
                ok = false;
                break;
            };
            for cell in &row.cells {
                let Some(col) = maps
                    .cols(&tsheet.name)
                    .and_then(|a| a.map(cell.col, MAX_COL))
                else {
                    continue;
                };
                let formula = match formulas.get(&(*r, cell.col)) {
                    Some(f) => match render(f, &tsheet.name, &maps) {
                        Some(t) => Some(t),
                        None => {
                            ok = false;
                            break;
                        }
                    },
                    None => None,
                };
                let value = write_of(p.theirs, *tj, cell.col, *r, formula);
                if value != Write::Clear {
                    cells.push(Step::Set {
                        sheet: ours_name.clone(),
                        col,
                        row: final_r,
                        value,
                    });
                }
            }
        }
        if ok {
            sets.extend(cells);
            out.brought.push(id.clone());
        } else {
            out.skipped.push(Skipped {
                id: id.clone(),
                reason: "A formula in these rows refers to rows this version doesn't have."
                    .to_owned(),
            });
        }
    }

    // Single cells.
    for item in &p.items {
        let What::Cell(c) = &item.what else {
            continue;
        };
        let Some(d) = decided.get(item.id.as_str()) else {
            continue;
        };
        if !d.brings_in() || !matches!(item.status, Status::Clean | Status::Overlap(_)) {
            continue;
        }
        match cell_step(p, c, &mut theirs_cache, &theirs_formulas, &maps, &inserts) {
            Ok((set, expect, undo)) => {
                sets.push(set);
                out.expect.push(expect);
                out.undo.push(undo);
                out.brought.push(item.id.clone());
            }
            Err(reason) => out.skipped.push(Skipped {
                id: item.id.clone(),
                reason,
            }),
        }
    }
    out.steps.extend(sets);

    // The way back: the cells first, then the inserted rows, bottom-up.
    for (sheet, list) in &inserts {
        let mut blocks_final: Vec<(u32, u32)> = (0..list.len())
            .map(|i| (block_start(list, i), list[i].1))
            .collect();
        blocks_final.sort_by_key(|x| std::cmp::Reverse(x.0));
        for (at, count) in blocks_final {
            out.undo.push(Step::DeleteRows {
                sheet: sheet.clone(),
                at,
                count,
            });
        }
    }
    out.inserts = inserts;
    Ok(out)
}

#[allow(clippy::too_many_arguments)]
fn cell_step(
    p: &Plan<'_>,
    c: &CellChange,
    cache: &mut HashMap<usize, HashMap<(u32, u32), Formula>>,
    theirs_formulas: &dyn Fn(usize) -> HashMap<(u32, u32), Formula>,
    maps: &Maps,
    inserts: &BTreeMap<String, Vec<(u32, u32)>>,
) -> std::result::Result<(Step, Expect, Step), String> {
    let no_place = || "Lineage couldn't find where this goes in this version.".to_owned();
    let ours_sheet = p.ours_sheet(c.sheet_a).ok_or_else(no_place)?;
    let al = p
        .mine
        .alignments
        .get(&(c.sheet_a, ours_sheet))
        .ok_or_else(no_place)?;
    let row = *al.rows.a_to_b.get(&c.at_a.1).ok_or_else(no_place)?;
    let col = *al.cols.a_to_b.get(&c.at_a.0).ok_or_else(no_place)?;
    let ours_name = p.ours.sheets[ours_sheet].name.clone();
    let ins = inserts.get(&ours_name).cloned().unwrap_or_default();
    let final_r = final_row(&ins, row);
    let tsheet = &p.theirs.sheets[c.sheet_b];
    let formulas = cache
        .entry(c.sheet_b)
        .or_insert_with(|| theirs_formulas(c.sheet_b));
    let formula = match formulas.get(&(c.at_b.1, c.at_b.0)) {
        Some(f) => Some(render(f, &tsheet.name, maps).ok_or_else(|| {
            "The master's formula refers to rows this version doesn't have.".to_owned()
        })?),
        None => None,
    };
    let value = write_of(p.theirs, c.sheet_b, c.at_b.0, c.at_b.1, formula);
    // What it holds now, and what puts it back (at its address after the
    // inserts, with references moved as Excel moves them).
    let ours_formula = super::deps::formula_at(p.ours, ours_sheet, row, col);
    let before = write_of(p.ours, ours_sheet, col, row, ours_formula.clone());
    let undo_value = match &before {
        Write::Formula(t) => Write::Formula(shift_for_inserts(&tokenize(t), &ours_name, inserts)),
        other => other.clone(),
    };
    Ok((
        Step::Set {
            sheet: ours_name.clone(),
            col,
            row: final_r,
            value,
        },
        Expect {
            sheet: ours_name.clone(),
            col,
            row,
            value: before,
        },
        Step::Set {
            sheet: ours_name,
            col,
            row: final_r,
            value: undo_value,
        },
    ))
}

/// A formula of ours as Excel rewrites it after rows are inserted: every
/// reference to a row at or below an insert moves down.
#[must_use]
pub fn shift_for_inserts(
    formula: &Formula,
    own_sheet: &str,
    inserts: &BTreeMap<String, Vec<(u32, u32)>>,
) -> String {
    formula.render_with(|t| {
        let Token::Ref(r) = t else {
            return None;
        };
        if r.sheet
            .as_ref()
            .is_some_and(|p| p.book.is_some() || p.last.is_some())
        {
            return None;
        }
        let sheet = r.sheet.as_ref().map_or(own_sheet, |p| p.first.as_str());
        let ins = inserts
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(sheet))
            .map(|(_, v)| v.as_slice())?;
        let mv = |c: Coord| Coord {
            n: final_row(ins, c.n).min(MAX_ROW),
            abs: c.abs,
        };
        let kind = match r.kind {
            RefKind::Cell { col, row } => RefKind::Cell { col, row: mv(row) },
            RefKind::Area { c1, r1, c2, r2 } => RefKind::Area {
                c1,
                r1: mv(r1),
                c2,
                r2: mv(r2),
            },
            RefKind::Rows { r1, r2 } => RefKind::Rows {
                r1: mv(r1),
                r2: mv(r2),
            },
            cols @ RefKind::Cols { .. } => cols,
        };
        if kind == r.kind {
            return None;
        }
        let prefix = r.sheet.as_ref().map(render_prefix).unwrap_or_default();
        Some(format!(
            "{prefix}{}",
            super::formula::render_reference(&Reference {
                sheet: None,
                kind,
                spill: r.spill,
            })
        ))
    })
}

/// What a cell holds, for comparing: an input by value, a formula by its
/// text in one case, ignoring spaces outside text.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Held {
    Value(u64),
    Formula(String),
}

fn normal_formula(text: &str) -> String {
    tokenize(text)
        .render_with(|t| match t {
            Token::Text(_) => None,
            Token::Other(o) => Some(o.split_whitespace().collect::<String>().to_uppercase()),
            _ => None,
        })
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase()
}

fn held_of_write(w: &Write) -> Option<Held> {
    Some(match w {
        Write::Clear => return None,
        Write::Number(n) => Held::Value(Value::Number(n).key()),
        Write::Text(t) => Held::Value(Value::Text(t).key()),
        Write::Bool(b) => Held::Value(Value::Bool(*b).key()),
        Write::Error(e) => Held::Value(Value::Error(e).key()),
        Write::Formula(f) => Held::Formula(normal_formula(f)),
    })
}

/// Every non-blank cell of a workbook, by (sheet name in lower case, row,
/// column).
fn held(wb: &Workbook) -> BTreeMap<(String, u32, u32), Held> {
    let mut out = BTreeMap::new();
    for (i, s) in wb.sheets.iter().enumerate() {
        let name = s.name.to_lowercase();
        let text = wb.sheet_text(i);
        let mut formulas: HashMap<(u32, u32), String> = HashMap::new();
        each_formula(wb, i, |r, c, f, _| {
            formulas.insert((r, c), f.render());
        });
        for row in &s.rows {
            for cell in &row.cells {
                let h = match &cell.content {
                    Content::Formula(_) => match formulas.get(&(row.r, cell.col)) {
                        Some(f) => Held::Formula(normal_formula(f)),
                        None => continue,
                    },
                    _ => match value::input(wb, text, cell) {
                        Some(Value::Blank) | None => continue,
                        Some(v) => Held::Value(v.key()),
                    },
                };
                out.insert((name.clone(), row.r, cell.col), h);
            }
        }
    }
    out
}

/// What ours should hold after the plan: its own cells moved by the
/// inserts (and their formulas rewritten as Excel does), and the plan's
/// writes.
fn expected(ours: &Workbook, plan: &ApplyPlan) -> BTreeMap<(String, u32, u32), Held> {
    let mut out = BTreeMap::new();
    for ((sheet, row, col), h) in held(ours) {
        let ins = plan
            .inserts
            .iter()
            .find(|(n, _)| n.to_lowercase() == sheet)
            .map(|(_, v)| v.clone())
            .unwrap_or_default();
        let own = ours
            .sheets
            .iter()
            .find(|s| s.name.to_lowercase() == sheet)
            .map_or(String::new(), |s| s.name.clone());
        let h = match h {
            Held::Formula(f) => Held::Formula(normal_formula(&shift_for_inserts(
                &tokenize(&f),
                &own,
                &plan.inserts,
            ))),
            v => v,
        };
        out.insert((sheet, final_row(&ins, row), col), h);
    }
    for step in &plan.steps {
        if let Step::Set {
            sheet,
            col,
            row,
            value,
        } = step
        {
            let key = (sheet.to_lowercase(), *row, *col);
            match held_of_write(value) {
                Some(h) => {
                    out.insert(key, h);
                }
                None => {
                    out.remove(&key);
                }
            }
        }
    }
    out
}

/// Checks that `actual` (what Excel holds after running the plan on `ours`)
/// is the workbook the plan meant: every input by value and every formula
/// by its text, on every sheet. Calculated results are left out: Excel
/// recalculates. Anything else, such as typing during the plan, is a
/// mismatch, and the pane runs the way back.
///
/// # Errors
/// [`Error::NotLanded`], naming the first cell that differs.
pub fn landed(ours: &Workbook, plan: &ApplyPlan, actual: &Workbook) -> Result<()> {
    compare(&expected(ours, plan), &held(actual))
}

/// Checks that the way back put `actual` back to `ours`.
///
/// # Errors
/// [`Error::NotLanded`], naming the first cell that differs.
pub fn put_back(ours: &Workbook, actual: &Workbook) -> Result<()> {
    compare(&held(ours), &held(actual))
}

fn compare(
    want: &BTreeMap<(String, u32, u32), Held>,
    have: &BTreeMap<(String, u32, u32), Held>,
) -> Result<()> {
    for (key, w) in want {
        match have.get(key) {
            Some(h) if h == w => {}
            other => {
                return Err(Error::NotLanded(format!(
                    "{}!{}: expected {w:?}, found {other:?}",
                    key.0,
                    super::refs::a1(key.2, key.1)
                )));
            }
        }
    }
    if let Some((key, h)) = have.iter().find(|(k, _)| !want.contains_key(*k)) {
        return Err(Error::NotLanded(format!(
            "{}!{}: unexpected {h:?}",
            key.0,
            super::refs::a1(key.2, key.1)
        )));
    }
    Ok(())
}

/// True when a cell of `wb` holds what `w` writes, compared as the pane
/// does before running a plan.
#[must_use]
pub fn holds(wb: &Workbook, sheet: usize, col: u32, row: u32, w: &Write) -> bool {
    let formula = super::deps::formula_at(wb, sheet, row, col);
    let now = write_of(wb, sheet, col, row, formula);
    match (&now, w) {
        (Write::Number(a), Write::Number(b)) => number_key(a) == number_key(b),
        (Write::Formula(a), Write::Formula(b)) => normal_formula(a) == normal_formula(b),
        _ => now == *w,
    }
}

#[cfg(test)]
mod tests;
