// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Three-way merge of workbooks (decision 0023): which of the master's
//! changes (`base` → `theirs`) can come into a tailored version (`base` →
//! `ours`) by themselves, which overlap with the version's own changes, and
//! which must be brought in by hand.
//!
//! The unit is a cell, matched through alignment (decision 0021), never by
//! address alone. Different cells merge only when the engine can show they
//! are independent: a formula the master changed whose inputs the version
//! changed (or the other way round) is an overlap, not two clean changes.
//! Calculated results are never merged: Excel recalculates.

use std::collections::HashMap;

use super::Workbook;
use super::deps::Deps;
use super::diff::{Category, CellChange, StructureChange, StructureKind, WorkbookDiff, diff};
use super::model::{Area, UnsupportedKind};
use crate::merge::Decision;

/// How the two sides overlap on one cell or row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Conflict {
    /// Both changed the same cell.
    BothChanged,
    /// One side changed a formula, the other one of its inputs.
    FormulaAndInputs,
    /// The master changed a cell in a row this version deleted.
    EditedVsDeleted,
    /// Both added rows at the same place.
    BothAdded,
    /// Lineage couldn't match the row in this version with confidence.
    CouldNotMatch,
}

/// Why a change must be brought in by hand in this version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Manual {
    /// A cell's format: styles don't carry between workbooks yet.
    Format,
    /// Rows the master removed: removing rows in Excel turns every formula
    /// that pointed at them into #REF!, which can't be undone exactly.
    RowsRemoved,
    /// Rows the master moved or sorted.
    RowsMoved,
    /// Columns added, removed or hidden.
    Columns,
    /// A sheet added, removed or renamed.
    Sheet,
    /// Names, tables, merged cells, conditional formats, data validation,
    /// filters, protection, page setup and settings.
    Settings,
    /// The sheet or row it goes in isn't in this version.
    Placement,
    /// The master's formula uses a defined name this version doesn't have
    /// (names come in by hand for now).
    Name,
    /// Data from a connection: refresh this workbook's data instead.
    Refreshed,
    /// A part kept but not tracked yet.
    Untracked(UnsupportedKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    Clean,
    /// This version already has the same content.
    AlreadyThere,
    Overlap(Conflict),
    ByHand(Manual),
}

/// What an item brings in.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum What {
    /// One cell of the master (theirs), as the diff reports it.
    Cell(CellChange),
    /// Rows the master added: its sheet and row numbers in the master.
    Rows { sheet_theirs: usize, rows: Vec<u32> },
    /// A structure change, never written in v0.
    Structure(StructureChange),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: String,
    pub what: What,
    pub status: Status,
    /// This version's change to the same cell, for an overlap.
    pub mine: Option<CellChange>,
}

impl Item {
    /// The choices the panel offers.
    #[must_use]
    pub fn choices(&self) -> Vec<Decision> {
        match self.status {
            Status::Clean => vec![Decision::BringIn, Decision::Decline],
            Status::Overlap(Conflict::BothAdded) => vec![
                Decision::KeepMine,
                Decision::KeepBoth {
                    masters_first: false,
                },
                Decision::KeepBoth {
                    masters_first: true,
                },
            ],
            // Rows this version deleted or couldn't match have no place
            // for the master's side.
            Status::Overlap(Conflict::EditedVsDeleted | Conflict::CouldNotMatch) => {
                vec![Decision::KeepMine]
            }
            Status::Overlap(_) => vec![Decision::KeepMine, Decision::UseMasters],
            Status::ByHand(_) => vec![Decision::Decline],
            Status::AlreadyThere => Vec::new(),
        }
    }
}

/// The merge of `theirs` into `ours`.
#[derive(Debug)]
pub struct Plan<'d> {
    pub base: &'d Workbook,
    pub ours: &'d Workbook,
    pub theirs: &'d Workbook,
    /// base → ours.
    pub mine: WorkbookDiff,
    /// base → theirs: what is on offer.
    pub master: WorkbookDiff,
    pub items: Vec<Item>,
}

impl Plan<'_> {
    #[must_use]
    pub fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }

    /// The sheet of ours that a sheet of base became.
    #[must_use]
    pub fn ours_sheet(&self, base_sheet: usize) -> Option<usize> {
        self.mine
            .sheets
            .both
            .iter()
            .find(|(a, _)| *a == base_sheet)
            .map(|(_, b)| *b)
    }

    /// The sheet of base that a sheet of theirs came from.
    #[must_use]
    pub fn base_of_theirs(&self, theirs_sheet: usize) -> Option<usize> {
        self.master
            .sheets
            .both
            .iter()
            .find(|(_, b)| *b == theirs_sheet)
            .map(|(a, _)| *a)
    }
}

/// Where a base row went in ours: matched, deleted, or unsure.
enum Mapped {
    To,
    Deleted,
    Unsure,
}

fn map_row(d: &WorkbookDiff, base_sheet: usize, ours_sheet: usize, row: u32) -> Mapped {
    let Some(al) = d.alignments.get(&(base_sheet, ours_sheet)) else {
        return Mapped::Unsure;
    };
    if al.rows.a_to_b.contains_key(&row) {
        return Mapped::To;
    }
    if al.rows.unsure_a.contains(&row) {
        Mapped::Unsure
    } else {
        Mapped::Deleted
    }
}

/// The areas a cell's formula reads, in base coordinates, from the base
/// version and from the changed version (mapped back through alignment).
fn read_areas(
    base_deps: &Deps,
    other_deps: &Deps,
    d: &WorkbookDiff,
    base_sheet: usize,
    other_sheet: usize,
    at_base: (u32, u32),
    at_other: (u32, u32),
) -> Vec<(usize, Area)> {
    let mut out = Vec::new();
    if let Some(f) = base_deps.formula(base_sheet, at_base.1, at_base.0) {
        out.extend(f.precedents.iter().map(|p| (p.sheet, p.area)));
    }
    if let Some(f) = other_deps.formula(other_sheet, at_other.1, at_other.0) {
        for p in &f.precedents {
            // The other version's sheet and rows, back to base.
            let Some(&(sa, _)) = d.sheets.both.iter().find(|(_, b)| *b == p.sheet) else {
                continue;
            };
            let Some(al) = d.alignments.get(&(sa, p.sheet)) else {
                out.push((sa, p.area));
                continue;
            };
            // By the offset from the nearest matched row at or above. Pairs
            // are in order on both sides when no row moved; when rows moved,
            // the whole columns are taken instead.
            let back = |r: u32| -> u32 {
                let i = al.rows.pairs.partition_point(|x| x.1 <= r);
                if i == 0 {
                    return r;
                }
                let (pa, pb) = al.rows.pairs[i - 1];
                u32::try_from(i64::from(pa) + i64::from(r) - i64::from(pb)).unwrap_or(r)
            };
            let area = if al.rows.moved_b.is_empty() {
                Area {
                    row1: back(p.area.row1),
                    row2: back(p.area.row2),
                    ..p.area
                }
            } else {
                Area {
                    row1: 1,
                    row2: super::refs::MAX_ROW,
                    ..p.area
                }
            };
            out.push((sa, area));
        }
    }
    out
}

fn reads(areas: &[(usize, Area)], sheet: usize, col: u32, row: u32) -> bool {
    areas
        .iter()
        .any(|(s, a)| *s == sheet && a.contains(col, row))
}

/// Merges the master's changes since `base` into `ours`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn plan<'d>(base: &'d Workbook, ours: &'d Workbook, theirs: &'d Workbook) -> Plan<'d> {
    let mine = diff(base, ours, &[]);
    let master = diff(base, theirs, &[]);
    let base_deps = Deps::build(base);
    let ours_deps = Deps::build(ours);
    let theirs_deps = Deps::build(theirs);

    // This version's own cell changes, by base position.
    let mut mine_at: HashMap<(usize, u32, u32), &CellChange> = HashMap::new();
    for c in &mine.cells {
        if matches!(c.category, Category::Input | Category::Formula) {
            mine_at.insert((c.sheet_a, c.at_a.0, c.at_a.1), c);
        }
    }
    // This version's formula changes, with what they read in base terms.
    let mine_formulas: Vec<(&CellChange, Vec<(usize, Area)>)> = mine
        .cells
        .iter()
        .filter(|c| c.category == Category::Formula)
        .map(|c| {
            (
                c,
                read_areas(
                    &base_deps, &ours_deps, &mine, c.sheet_a, c.sheet_b, c.at_a, c.at_b,
                ),
            )
        })
        .collect();
    let mine_inputs: Vec<&CellChange> = mine
        .cells
        .iter()
        .filter(|c| matches!(c.category, Category::Input | Category::Formula))
        .collect();

    let mut items = Vec::new();
    for c in &master.cells {
        let status = match c.category {
            Category::Structure => Status::ByHand(Manual::Format),
            Category::Refreshed => Status::ByHand(Manual::Refreshed),
            Category::KnockOn => continue,
            Category::Input | Category::Formula => {
                let base_sheet = c.sheet_a;
                match mine
                    .sheets
                    .both
                    .iter()
                    .find(|(a, _)| *a == base_sheet)
                    .map(|p| p.1)
                {
                    None => Status::ByHand(Manual::Placement),
                    Some(ours_sheet) => match map_row(&mine, base_sheet, ours_sheet, c.at_a.1) {
                        Mapped::Deleted => Status::Overlap(Conflict::EditedVsDeleted),
                        Mapped::Unsure => Status::Overlap(Conflict::CouldNotMatch),
                        Mapped::To => {
                            let col_ok = mine
                                .alignments
                                .get(&(base_sheet, ours_sheet))
                                .is_some_and(|al| al.cols.a_to_b.contains_key(&c.at_a.0));
                            if !col_ok {
                                Status::ByHand(Manual::Placement)
                            } else if c
                                .after
                                .formula
                                .as_deref()
                                .is_some_and(|f| needs_missing_name(f, ours))
                            {
                                Status::ByHand(Manual::Name)
                            } else if let Some(m) = mine_at.get(&(base_sheet, c.at_a.0, c.at_a.1)) {
                                if m.after == c.after {
                                    Status::AlreadyThere
                                } else {
                                    Status::Overlap(Conflict::BothChanged)
                                }
                            } else {
                                // A formula of the master's whose inputs this
                                // version changed, or an input of the master's
                                // that a formula this version changed reads.
                                let theirs_reads = if c.category == Category::Formula {
                                    read_areas(
                                        &base_deps,
                                        &theirs_deps,
                                        &master,
                                        base_sheet,
                                        c.sheet_b,
                                        c.at_a,
                                        c.at_b,
                                    )
                                } else {
                                    Vec::new()
                                };
                                let tangled = mine_inputs
                                    .iter()
                                    .any(|m| reads(&theirs_reads, m.sheet_a, m.at_a.0, m.at_a.1))
                                    || mine_formulas.iter().any(|(_, areas)| {
                                        reads(areas, base_sheet, c.at_a.0, c.at_a.1)
                                    });
                                if tangled {
                                    Status::Overlap(Conflict::FormulaAndInputs)
                                } else {
                                    Status::Clean
                                }
                            }
                        }
                    },
                }
            }
        };
        let mine_side = match status {
            Status::Overlap(Conflict::BothChanged) => mine_at
                .get(&(c.sheet_a, c.at_a.0, c.at_a.1))
                .map(|m| (*m).clone()),
            _ => None,
        };
        items.push(Item {
            id: c.id.clone(),
            what: What::Cell(c.clone()),
            status,
            mine: mine_side,
        });
    }

    for s in &master.structure {
        let status = match &s.kind {
            StructureKind::RowsAdded { rows } => {
                let Some(j) = theirs.sheet_index(&s.sheet) else {
                    continue;
                };
                let status = rows_status(&mine, &master, j, rows);
                items.push(Item {
                    id: s.id.clone(),
                    what: What::Rows {
                        sheet_theirs: j,
                        rows: rows.clone(),
                    },
                    status,
                    mine: None,
                });
                continue;
            }
            StructureKind::RowsRemoved { .. } => Status::ByHand(Manual::RowsRemoved),
            StructureKind::RowsMoved { .. } | StructureKind::RowsSorted { .. } => {
                Status::ByHand(Manual::RowsMoved)
            }
            StructureKind::ColumnsAdded { .. }
            | StructureKind::ColumnsRemoved { .. }
            | StructureKind::ColumnsHidden { .. }
            | StructureKind::ColumnsShown { .. }
            | StructureKind::ColumnFormat { .. } => Status::ByHand(Manual::Columns),
            StructureKind::SheetAdded
            | StructureKind::SheetRemoved
            | StructureKind::SheetRenamed { .. }
            | StructureKind::SheetMoved
            | StructureKind::SheetHidden
            | StructureKind::SheetShown => Status::ByHand(Manual::Sheet),
            StructureKind::Untracked { kind } => Status::ByHand(Manual::Untracked(*kind)),
            _ => Status::ByHand(Manual::Settings),
        };
        items.push(Item {
            id: s.id.clone(),
            what: What::Structure(s.clone()),
            status,
            mine: None,
        });
    }

    Plan {
        base,
        ours,
        theirs,
        mine,
        master,
        items,
    }
}

/// Rows the master added: where they go in ours, and whether ours added
/// rows at the same place.
fn rows_status(
    mine: &WorkbookDiff,
    master: &WorkbookDiff,
    theirs_sheet: usize,
    rows: &[u32],
) -> Status {
    let Some(&(base_sheet, _)) = master.sheets.both.iter().find(|(_, b)| *b == theirs_sheet) else {
        return Status::ByHand(Manual::Placement);
    };
    let Some(ours_sheet) = mine
        .sheets
        .both
        .iter()
        .find(|(a, _)| *a == base_sheet)
        .map(|p| p.1)
    else {
        return Status::ByHand(Manual::Placement);
    };
    let Some(anchor) = anchor_above(master, base_sheet, theirs_sheet, rows[0]) else {
        return Status::ByHand(Manual::Placement);
    };
    match map_row(mine, base_sheet, ours_sheet, anchor) {
        Mapped::To => {}
        _ => return Status::ByHand(Manual::Placement),
    }
    // Ours added rows right after the same base row too.
    if let Some(al) = mine.alignments.get(&(base_sheet, ours_sheet))
        && let Some(&ours_anchor) = al.rows.a_to_b.get(&anchor)
        && al.rows.only_b.contains(&(ours_anchor + 1))
    {
        return Status::Overlap(Conflict::BothAdded);
    }
    Status::Clean
}

/// True when a formula (with `=`) uses a name `wb` doesn't define, that
/// isn't a table either.
#[must_use]
pub fn needs_missing_name(formula: &str, wb: &Workbook) -> bool {
    let f = super::formula::tokenize(formula.trim_start_matches('='));
    f.names().any(|(prefix, name)| {
        prefix.is_none_or(|p| p.book.is_none())
            && !wb.names.iter().any(|n| n.name.eq_ignore_ascii_case(name))
            && !wb.tables.iter().any(|t| t.name.eq_ignore_ascii_case(name))
    })
}

/// The base row just above `row` of theirs: the row the new rows follow.
#[must_use]
pub fn anchor_above(
    master: &WorkbookDiff,
    base_sheet: usize,
    theirs_sheet: usize,
    row: u32,
) -> Option<u32> {
    let al = master.alignments.get(&(base_sheet, theirs_sheet))?;
    (1..row).rev().find_map(|r| al.rows.b_to_a.get(&r).copied())
}

#[cfg(test)]
mod tests;
