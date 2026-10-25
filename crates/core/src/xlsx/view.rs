// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The workbook "What changed" view: the diff (decision 0022), grouped into
//! the five categories and worded for people, and the open change format.
//!
//! Inputs and formulas are listed. A formula filled down a column is one
//! card. Knock-on changes are summarised ("This changed 83 calculated cells,
//! including Summary!B2"), never listed one by one. Every label uses the
//! glossary in CLAUDE.md.

use std::collections::BTreeMap;

use super::Workbook;
use super::diff::{
    Calculated, Category, CellChange, CellSide, Editor, Provenance, StructureChange, StructureKind,
    WorkbookDiff,
};
use super::display::display;
use super::model::UnsupportedKind;
use super::refs::{a1, area_name, col_name};
use super::value;
use crate::diff::spec::{ChangeList, Location, SpecChange};
use crate::summary::quote;

/// The label the panel puts on parts the engine keeps but doesn't track.
pub const UNTRACKED: &str = "Preserved; semantic tracking not yet supported.";

/// Said on every update card for rows that come in.
pub const ROWS_FORMATTED: &str = "New rows take the formatting of the row above.";

/// How many examples a summarised group shows.
pub const EXAMPLES: usize = 5;

/// One card: a cell, a group of cells, or a structure change.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Card {
    pub id: String,
    pub summary: String,
    /// "Assumptions!B4 · Price per seat".
    pub place: Option<String>,
    pub sheet: String,
    /// Where to jump: a cell or range in the later version.
    pub address: Option<String>,
    pub before: Option<CellSide>,
    pub after: Option<CellSide>,
    /// How many cells it stands for (filled-down formulas).
    pub cells: usize,
    /// Why, in plain words, when the evidence says something.
    pub why: Option<String>,
    /// For an update from the master: every item id this card stands for
    /// (a filled-down formula is one card), the choices, why it comes in
    /// by hand, and this version's side of an overlap.
    pub ids: Vec<String>,
    pub choices: Vec<crate::view::Choice>,
    pub by_hand: Option<String>,
    pub mine: Option<CellSide>,
    /// What the overlap is, in words.
    pub conflict: Option<String>,
    /// A macro's or query's code: removed and added lines, with a little
    /// context (decisions 0028, 0030).
    pub code: Vec<crate::code::CodeLine>,
    /// A pivot table's fields before and after (decision 0029).
    pub fields: Option<FieldLists>,
    /// One line per thing that changed in a macro, query or pivot.
    pub details: Vec<String>,
    /// A macro's or query's whole code in the later version, for "Copy
    /// code": it comes in by hand (decision 0027).
    pub copy: Option<String>,
}

/// A pivot table's fields, before and after.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FieldLists {
    pub before: Option<super::pivot::Fields>,
    pub after: Option<super::pivot::Fields>,
}

/// The groups for workbook parts beyond cells (Phase 5B).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Object {
    Macros,
    Queries,
    Pivots,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub category: Category,
    /// For the Macros, Queries and Pivot tables groups; their changes are
    /// structure (or refreshed data) in the open change format.
    pub object: Option<Object>,
    pub title: &'static str,
    /// One line for the whole group.
    pub summary: String,
    pub cards: Vec<Card>,
    /// Cards not shown.
    pub more: usize,
}

/// Rows that could not be matched, on one sheet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnmatchedCard {
    pub sheet: String,
    pub summary: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkbookView {
    pub headline: String,
    /// In the order of the five categories; empty ones left out.
    pub groups: Vec<Group>,
    pub unmatched: Vec<UnmatchedCard>,
    /// "Macros: Preserved; semantic tracking not yet supported."
    pub untracked: Vec<String>,
    /// How rows were matched, where a key was used.
    pub notes: Vec<String>,
    /// Macro code is new or different in the later version: what makes a
    /// returned file "come back with new macro code".
    pub new_macro_code: bool,
    /// "Signed by Fabrikam Test Signing. …" when the later version's macros
    /// are signed.
    pub signature: Option<String>,
}

/// Said when a workbook came back with new or changed macro code.
pub const NEW_MACRO_CODE: &str = "This file came back with new macro code. Lineage never runs it. Check each macro before you bring it in.";

#[must_use]
pub fn title(category: Category) -> &'static str {
    match category {
        Category::Input => "Inputs",
        Category::Formula => "Formulas",
        Category::KnockOn => "Knock-on changes",
        Category::Refreshed => "Refreshed data",
        Category::Structure => "Structure and formatting",
    }
}

/// The plain-language reason, or nothing when there is nothing to say.
#[must_use]
pub fn why(category: Category, provenance: Provenance) -> Option<String> {
    Some(
        match (category, provenance) {
            (_, Provenance::DirectEdit) => "Edited in this session",
            (_, Provenance::Downstream) => "Follows from a change above",
            (_, Provenance::LikelyDownstream) => "Probably follows from a change above",
            (_, Provenance::Refresh) => "From a data refresh or another workbook",
            (Category::KnockOn, Provenance::Unknown) => {
                "Cause unknown: nothing it depends on changed"
            }
            _ => return None,
        }
        .to_owned(),
    )
}

/// A changed cell's reason, naming a co-author when the edit was theirs.
fn cell_why(c: &CellChange) -> Option<String> {
    match (c.provenance, c.edited_by) {
        (Provenance::DirectEdit, Some(Editor::CoAuthor)) => {
            Some("Edited by someone you share it with, in this session".to_owned())
        }
        (Provenance::DirectEdit, Some(Editor::Both)) => {
            Some("Edited in this session, by you and by someone you share it with".to_owned())
        }
        _ => why(c.category, c.provenance),
    }
}

fn spec_editor(e: Editor) -> &'static str {
    match e {
        Editor::You => "you",
        Editor::CoAuthor => "coAuthor",
        Editor::Both => "both",
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn shown(side: &CellSide) -> String {
    if side.shown.is_empty() {
        "empty".to_owned()
    } else {
        quote(&side.shown)
    }
}

fn place(c: &CellChange) -> String {
    let mut out = c.address();
    let names: Vec<&str> = [c.label.as_deref(), c.header.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    if !names.is_empty() {
        out.push_str(" · ");
        out.push_str(&names.join(", "));
    }
    out
}

fn cell_card(c: &CellChange) -> Card {
    let summary = match c.category {
        Category::Formula => match (&c.before.formula, &c.after.formula) {
            (Some(x), Some(y)) => format!("The formula changed from {} to {}", quote(x), quote(y)),
            (None, Some(y)) => format!("{} became the formula {}", shown(&c.before), quote(y)),
            (Some(x), None) => format!("The formula {} became {}", quote(x), shown(&c.after)),
            (None, None) => "The formula changed".to_owned(),
        },
        Category::Structure => "Its format changed".to_owned(),
        _ if c.before.shown.is_empty() => format!("{} added", shown(&c.after)),
        _ if c.after.shown.is_empty() => format!("{} removed", shown(&c.before)),
        _ => format!("{} changed to {}", shown(&c.before), shown(&c.after)),
    };
    let summary = if c.format_changed && c.category != Category::Structure {
        format!("{summary}, and its format")
    } else {
        summary
    };
    Card {
        id: c.id.clone(),
        summary,
        place: Some(place(c)),
        sheet: c.sheet.clone(),
        address: Some(a1(c.at_b.0, c.at_b.1)),
        before: Some(c.before.clone()),
        after: Some(c.after.clone()),
        cells: 1,
        why: cell_why(c),
        ..Card::default()
    }
}

fn rows_text(rows: &[u32]) -> String {
    match rows {
        [one] => format!("row {one}"),
        [first, .., last] if usize::try_from(last - first).ok() == Some(rows.len() - 1) => {
            format!("rows {first} to {last}")
        }
        many => plural(many.len(), "row", "rows"),
    }
}

fn cols_text(cols: &[u32]) -> String {
    match cols {
        [one] => format!("column {}", col_name(*one)),
        [first, .., last] => format!("columns {} to {}", col_name(*first), col_name(*last)),
        [] => String::new(),
    }
}

/// One line for a structure change.
#[must_use]
pub fn structure_summary(s: &StructureChange) -> String {
    let on = if s.sheet.is_empty() {
        String::new()
    } else {
        format!(" on {}", s.sheet)
    };
    match &s.kind {
        StructureKind::SheetAdded => format!("Added the sheet {}", s.sheet),
        StructureKind::SheetRemoved => format!("Removed the sheet {}", s.sheet),
        StructureKind::SheetRenamed { from } => format!("Renamed the sheet {from} to {}", s.sheet),
        StructureKind::SheetMoved => format!("Moved the sheet {}", s.sheet),
        StructureKind::SheetHidden => format!("Hid the sheet {}", s.sheet),
        StructureKind::SheetShown => format!("Showed the sheet {}", s.sheet),
        StructureKind::RowsAdded { rows } => format!("Added {}{on}", rows_text(rows)),
        StructureKind::RowsRemoved { rows } => format!("Removed {}{on}", rows_text(rows)),
        StructureKind::RowsMoved { rows } => {
            format!("Moved {}{on}", plural(rows.len(), "row", "rows"))
        }
        StructureKind::RowsSorted { by, descending } => format!(
            "Sorted the rows{on} by {by}{}",
            if *descending { ", largest first" } else { "" }
        ),
        StructureKind::RowsHidden { rows } => format!("Hid {}{on}", rows_text(rows)),
        StructureKind::RowsShown { rows } => format!("Showed {}{on}", rows_text(rows)),
        StructureKind::RowFormat { rows } => {
            format!("Changed the format of {}{on}", rows_text(rows))
        }
        StructureKind::ColumnsAdded { cols } => format!("Added {}{on}", cols_text(cols)),
        StructureKind::ColumnsRemoved { cols } => format!("Removed {}{on}", cols_text(cols)),
        StructureKind::ColumnsHidden { cols } => format!("Hid {}{on}", cols_text(cols)),
        StructureKind::ColumnsShown { cols } => format!("Showed {}{on}", cols_text(cols)),
        StructureKind::ColumnFormat { cols } => format!(
            "Changed the width or format of {}{on}",
            plural(cols.len(), "column", "columns")
        ),
        StructureKind::MergedCells => format!("Merged or split cells{on}"),
        StructureKind::ConditionalFormats => format!("Changed the conditional formatting{on}"),
        StructureKind::DataValidation => format!("Changed the data validation{on}"),
        StructureKind::Protection => format!("Changed the protection{on}"),
        StructureKind::Hyperlinks => format!("Changed the links{on}"),
        StructureKind::Filter => format!("Changed the filter{on}"),
        StructureKind::PageSetup => format!("Changed the page setup{on}"),
        StructureKind::SheetSettings => format!("Changed the sheet settings{on}"),
        StructureKind::NameAdded { name } => format!("Added the name {name}"),
        StructureKind::NameRemoved { name } => format!("Removed the name {name}"),
        StructureKind::NameChanged { name, from, to } => {
            format!(
                "The name {name} now refers to {} instead of {}",
                quote(to),
                quote(from)
            )
        }
        StructureKind::TableAdded { name } => format!("Added the table {name}{on}"),
        StructureKind::TableRemoved { name } => format!("Removed the table {name}{on}"),
        StructureKind::TableChanged { name } => format!("Changed the table {name}'s settings{on}"),
        StructureKind::ConnectionChanged { name } => format!("Changed the data connection {name}"),
        StructureKind::WorkbookSettings => "Changed the workbook's settings".to_owned(),
        StructureKind::Calculation => "Changed how the workbook calculates".to_owned(),
        StructureKind::Untracked { kind } => format!("{} changed. {UNTRACKED}", kind.label()),
    }
}

fn structure_address(s: &StructureChange) -> Option<String> {
    match &s.kind {
        StructureKind::RowsAdded { rows }
        | StructureKind::RowsHidden { rows }
        | StructureKind::RowsShown { rows }
        | StructureKind::RowFormat { rows } => Some(format!("{}:{}", rows.first()?, rows.last()?)),
        StructureKind::ColumnsAdded { cols }
        | StructureKind::ColumnsHidden { cols }
        | StructureKind::ColumnsShown { cols } => Some(format!(
            "{}:{}",
            col_name(*cols.first()?),
            col_name(*cols.last()?)
        )),
        _ => None,
    }
}

fn structure_card(s: &StructureChange) -> Card {
    Card {
        id: s.id.clone(),
        summary: structure_summary(s),
        place: (!s.sheet.is_empty()).then(|| s.sheet.clone()),
        sheet: s.sheet.clone(),
        address: structure_address(s),
        before: None,
        after: None,
        cells: 1,
        why: None,
        ..Card::default()
    }
}

/// The address of a calculated cell, with its sheet.
fn calc_address(b: &Workbook, c: &Calculated) -> String {
    let name = b.sheets.get(c.sheet_b).map_or("", |s| s.name.as_str());
    let card = CellChange {
        id: String::new(),
        category: c.category,
        provenance: c.provenance,
        edited_by: None,
        sheet: name.to_owned(),
        sheet_a: 0,
        sheet_b: c.sheet_b,
        at_a: (c.col, c.row),
        at_b: (c.col, c.row),
        before: CellSide::default(),
        after: CellSide::default(),
        format_changed: false,
        group: 0,
        label: None,
        header: None,
    };
    card.address()
}

/// A calculated cell's card, with both sides read from the workbooks.
fn calc_card(a: &Workbook, b: &Workbook, d: &WorkbookDiff, c: &Calculated) -> Card {
    let side = |wb: &Workbook, sheet: usize, col: u32, row: u32| -> CellSide {
        let text = wb.sheet_text(sheet);
        wb.sheets
            .get(sheet)
            .and_then(|s| s.cell(col, row))
            .map(|cell| CellSide {
                shown: display(
                    &value::either(wb, text, cell),
                    wb.styles.format_code(cell.style),
                    wb.date1904,
                ),
                formula: super::deps::formula_at(wb, sheet, row, col).map(|f| format!("={f}")),
            })
            .unwrap_or_default()
    };
    let after = side(b, c.sheet_b, c.col, c.row);
    // The same cell in a, through the alignment.
    let before = d
        .alignments
        .iter()
        .find(|((_, j), _)| *j == c.sheet_b)
        .and_then(|((i, _), al)| {
            let ra = al.rows.b_to_a.get(&c.row)?;
            let ca = al.cols.b_to_a.get(&c.col)?;
            Some(side(a, *i, *ca, *ra))
        })
        .unwrap_or_default();
    let label = b.sheets.get(c.sheet_b).and_then(|s| {
        let text = b.sheet_text(c.sheet_b);
        s.row(c.row)?
            .cells
            .iter()
            .filter(|x| x.col < c.col)
            .find_map(|x| match value::input(b, text, x) {
                Some(value::Value::Text(t)) if !t.trim().is_empty() => Some(t.trim().to_owned()),
                _ => None,
            })
    });
    let address = calc_address(b, c);
    Card {
        id: format!("{}-{}", c.sheet_b, a1(c.col, c.row)),
        summary: format!("{} changed to {}", shown(&before), shown(&after)),
        place: Some(match label {
            Some(l) => format!("{address} · {l}"),
            None => address,
        }),
        sheet: b
            .sheets
            .get(c.sheet_b)
            .map_or_else(String::new, |s| s.name.clone()),
        address: Some(a1(c.col, c.row)),
        before: Some(before),
        after: Some(after),
        cells: 1,
        why: why(c.category, c.provenance),
        ..Card::default()
    }
}

/// Puts the model's outputs first: cells nothing else reads, on sheets
/// named like a summary.
fn example_order(b: &Workbook, list: &mut [&Calculated]) {
    let rank = |c: &Calculated| {
        let name = b
            .sheets
            .get(c.sheet_b)
            .map_or(String::new(), |s| s.name.to_lowercase());
        let summary = ["summary", "dashboard", "overview", "report", "total"]
            .iter()
            .any(|w| name.contains(w));
        (!c.terminal, !summary, c.sheet_b, c.row, c.col)
    };
    list.sort_by_key(|c| rank(c));
}

/// The view of a diff from `a` to `b`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn view(d: &WorkbookDiff, a: &Workbook, b: &Workbook) -> WorkbookView {
    let mut groups = Vec::new();

    // Inputs.
    let inputs: Vec<&CellChange> = d
        .cells
        .iter()
        .filter(|c| c.category == Category::Input)
        .collect();
    if !inputs.is_empty() {
        let cards: Vec<Card> = inputs.iter().map(|c| cell_card(c)).collect();
        groups.push(Group {
            object: None,
            category: Category::Input,
            title: title(Category::Input),
            summary: format!(
                "{} changed",
                plural(inputs.len(), "typed value", "typed values")
            ),
            cards,
            more: 0,
        });
    }

    // Formulas: one card per group of filled-down cells.
    let formulas: Vec<&CellChange> = d
        .cells
        .iter()
        .filter(|c| c.category == Category::Formula)
        .collect();
    if !formulas.is_empty() {
        let mut by_group: BTreeMap<(usize, u32, u64), Vec<&CellChange>> = BTreeMap::new();
        for c in &formulas {
            by_group
                .entry((c.sheet_b, c.at_b.0, c.group))
                .or_default()
                .push(c);
        }
        let mut cards: Vec<(u32, Card)> = by_group
            .into_values()
            .map(|cells| {
                let first = cells[0];
                let mut card = cell_card(first);
                if cells.len() > 1 {
                    let rows: Vec<u32> = cells.iter().map(|c| c.at_b.1).collect();
                    let (lo, hi) = (
                        *rows.iter().min().unwrap_or(&1),
                        *rows.iter().max().unwrap_or(&1),
                    );
                    let range = area_name(&super::model::Area {
                        col1: first.at_b.0,
                        row1: lo,
                        col2: first.at_b.0,
                        row2: hi,
                    });
                    card.summary = format!(
                        "The formula changed in {} cells ({}!{range}), filled down. In {}: {}",
                        cells.len(),
                        first.sheet,
                        a1(first.at_b.0, first.at_b.1),
                        card.summary
                            .to_lowercase()
                            .trim_start_matches("the formula ")
                            .to_owned()
                    );
                    card.address = Some(range);
                    card.cells = cells.len();
                }
                (first.at_b.1, card)
            })
            .collect();
        cards.sort_by_key(|(row, c)| (c.sheet.clone(), *row));
        groups.push(Group {
            object: None,
            category: Category::Formula,
            title: title(Category::Formula),
            summary: format!("{} changed", plural(formulas.len(), "formula", "formulas")),
            cards: cards.into_iter().map(|(_, c)| c).collect(),
            more: 0,
        });
    }

    // Macros, queries and pivot tables (Phase 5B).
    groups.extend(object_groups(d, b));

    // Knock-on changes: summarised, with examples.
    let mut knock_on: Vec<&Calculated> = d
        .calculated
        .iter()
        .filter(|c| c.category == Category::KnockOn)
        .collect();
    if !knock_on.is_empty() {
        example_order(b, &mut knock_on);
        let including: Vec<String> = knock_on
            .iter()
            .take(2)
            .map(|c| calc_address(b, c))
            .collect();
        let unknown = knock_on
            .iter()
            .filter(|c| c.provenance == Provenance::Unknown)
            .count();
        let mut summary = format!(
            "This changed {}, including {}.",
            plural(knock_on.len(), "calculated cell", "calculated cells"),
            including.join(" and ")
        );
        if unknown > 0 {
            summary.push_str(&format!(
                " For {} nothing it depends on changed, so the cause is unknown.",
                plural(unknown, "of them", "of them")
            ));
        }
        let cards: Vec<Card> = knock_on
            .iter()
            .take(EXAMPLES)
            .map(|c| calc_card(a, b, d, c))
            .collect();
        groups.push(Group {
            object: None,
            category: Category::KnockOn,
            title: title(Category::KnockOn),
            summary,
            more: knock_on.len() - cards.len(),
            cards,
        });
    }

    // Refreshed data: values from connections and volatile functions.
    let refreshed_inputs: Vec<&CellChange> = d
        .cells
        .iter()
        .filter(|c| c.category == Category::Refreshed)
        .collect();
    let refreshed_calc: Vec<&Calculated> = d
        .calculated
        .iter()
        .filter(|c| c.category == Category::Refreshed)
        .collect();
    let total = refreshed_inputs.len() + refreshed_calc.len();
    if total > 0 {
        let mut cards: Vec<Card> = refreshed_inputs
            .iter()
            .take(EXAMPLES)
            .map(|c| cell_card(c))
            .collect();
        for c in refreshed_calc
            .iter()
            .take(EXAMPLES.saturating_sub(cards.len()))
        {
            cards.push(calc_card(a, b, d, c));
        }
        groups.push(Group {
            object: None,
            category: Category::Refreshed,
            title: title(Category::Refreshed),
            summary: format!(
                "{} came from a data refresh, another workbook or a function like TODAY(). Nobody typed {}.",
                plural(total, "value", "values"),
                if total == 1 { "it" } else { "them" }
            ),
            more: total - cards.len(),
            cards,
        });
    }

    // Structure and formatting.
    let format_cells: Vec<&CellChange> = d
        .cells
        .iter()
        .filter(|c| c.category == Category::Structure)
        .collect();
    let structure: Vec<&StructureChange> = d
        .structure
        .iter()
        .filter(|s| !matches!(s.kind, StructureKind::Untracked { .. }))
        .collect();
    if !structure.is_empty() || !format_cells.is_empty() {
        let mut cards: Vec<Card> = structure.iter().map(|s| structure_card(s)).collect();
        let mut by_sheet: BTreeMap<&str, Vec<&CellChange>> = BTreeMap::new();
        for c in &format_cells {
            by_sheet.entry(&c.sheet).or_default().push(c);
        }
        for (sheet, cells) in by_sheet {
            if let [one] = cells.as_slice() {
                cards.push(cell_card(one));
            } else {
                cards.push(Card {
                    id: cells[0].id.clone(),
                    summary: format!(
                        "Changed the format of {} on {sheet}",
                        plural(cells.len(), "cell", "cells")
                    ),
                    place: Some(sheet.to_owned()),
                    sheet: sheet.to_owned(),
                    address: Some(a1(cells[0].at_b.0, cells[0].at_b.1)),
                    before: None,
                    after: None,
                    cells: cells.len(),
                    why: None,
                    ..Card::default()
                });
            }
        }
        let n = structure.len() + format_cells.len();
        groups.push(Group {
            object: None,
            category: Category::Structure,
            title: title(Category::Structure),
            summary: format!(
                "{} to sheets, rows, columns, names or formats",
                plural(n, "change", "changes")
            ),
            cards,
            more: 0,
        });
    }

    let unmatched: Vec<UnmatchedCard> = d
        .unmatched
        .iter()
        .map(|u| UnmatchedCard {
            sheet: u.sheet.clone(),
            summary: format!(
                "Lineage couldn't tell which rows on {} are the same: {} in the earlier version and {} in this one. Check them yourself.",
                u.sheet,
                rows_text(&u.rows_a),
                rows_text(&u.rows_b)
            ),
        })
        .collect();

    let mut untracked: Vec<String> = Vec::new();
    let mut kinds: Vec<UnsupportedKind> = b.unsupported.iter().map(|u| u.kind).collect();
    kinds.sort();
    kinds.dedup();
    for kind in kinds {
        let changed = d
            .structure
            .iter()
            .any(|s| matches!(s.kind, StructureKind::Untracked { kind: k } if k == kind));
        untracked.push(if changed {
            format!("{}: changed. {UNTRACKED}", kind.label())
        } else {
            format!("{}: {UNTRACKED}", kind.label())
        });
    }

    let mut notes = Vec::new();
    let mut aligned: Vec<(&(usize, usize), &super::align::SheetAlignment)> =
        d.alignments.iter().collect();
    aligned.sort_by_key(|(k, _)| **k);
    for ((_, j), al) in aligned {
        let name = &b.sheets[*j].name;
        match &al.key {
            super::align::RowKey::Column(_, header)
                if !header.is_empty() && !al.rows.moved_b.is_empty() =>
            {
                notes.push(format!("Rows on {name} were matched by {header}."));
            }
            _ => {}
        }
    }
    // Between the budgets' two sizes (decision 0025): it works, more slowly.
    if a.cell_count().max(b.cell_count()) >= super::LARGE_CELLS {
        notes.push("This is a large workbook, so comparing it takes a little longer.".to_owned());
    }

    let counted: Vec<String> = groups
        .iter()
        .map(|g| {
            let n = g.cards.iter().map(|c| c.cells).sum::<usize>() + g.more;
            let (one, many) = match (g.object, g.category) {
                (Some(Object::Macros), _) => ("macro change", "macro changes"),
                (Some(Object::Queries), _) => ("query change", "query changes"),
                (Some(Object::Pivots), _) => ("pivot table change", "pivot table changes"),
                (None, c) => category_nouns(c),
            };
            plural(n, one, many)
        })
        .collect();
    let headline = if groups.is_empty() && unmatched.is_empty() {
        "No changes".to_owned()
    } else if counted.is_empty() {
        "Some rows couldn't be matched".to_owned()
    } else {
        counted.join(", ")
    };
    WorkbookView {
        headline,
        groups,
        unmatched,
        untracked,
        notes,
        new_macro_code: d.macros.new_code(),
        signature: signature_line(b),
    }
}

/// "Signed by …", when `wb`'s macros are signed.
#[must_use]
pub fn signature_line(wb: &Workbook) -> Option<String> {
    let s = wb.vba.as_ref()?.signature.as_ref()?;
    let who = s.signer.name.as_deref().map_or_else(
        || "The macros are signed".to_owned(),
        |n| format!("Signed by {n}"),
    );
    Some(format!(
        "{who}. Lineage doesn't check signatures. Changing any macro removes the signature."
    ))
}

fn category_nouns(c: Category) -> (&'static str, &'static str) {
    match c {
        Category::Input => ("input", "inputs"),
        Category::Formula => ("formula", "formulas"),
        Category::KnockOn => ("knock-on change", "knock-on changes"),
        Category::Refreshed => ("refreshed value", "refreshed values"),
        Category::Structure => ("structure or format change", "structure or format changes"),
    }
}

/// A card's detail lines without the one its summary already says.
#[must_use]
pub fn without(details: &[String], summary: &str) -> Vec<String> {
    details.iter().filter(|d| *d != summary).cloned().collect()
}

/// A card for a macro module, query or pivot change.
fn object_card(id: &str, summary: String, sheet: &str) -> Card {
    Card {
        id: id.to_owned(),
        summary,
        sheet: sheet.to_owned(),
        cells: 1,
        ..Card::default()
    }
}

/// The Macros, Queries and Pivot tables groups.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn object_groups(d: &WorkbookDiff, b: &Workbook) -> Vec<Group> {
    use super::pivot::PivotChangeKind;
    use super::query::QueryChangeKind;
    use crate::vba::diff::SignatureChange;
    let mut out = Vec::new();

    let m = &d.macros;
    if !m.is_empty() {
        let mut cards = Vec::new();
        if m.project_added {
            cards.push(object_card(
                "macros-added",
                "This version has macros now".to_owned(),
                "",
            ));
        }
        if m.project_removed {
            cards.push(object_card(
                "macros-removed",
                "The macros were removed".to_owned(),
                "",
            ));
        }
        for c in &m.modules {
            let mut card = object_card(&c.id, c.summary.clone(), "");
            card.place = Some(format!("{} {}", c.kind.noun(), c.name));
            card.code.clone_from(&c.code);
            card.copy.clone_from(&c.text);
            card.details = without(&c.details, &c.summary);
            card.why = Some("Macro code. Lineage never runs it.".to_owned());
            cards.push(card);
        }
        for r in &m.references_added {
            cards.push(object_card(
                &format!("reference-added-{r}"),
                format!("Added reference {r}"),
                "",
            ));
        }
        for r in &m.references_removed {
            cards.push(object_card(
                &format!("reference-removed-{r}"),
                format!("Removed reference {r}"),
                "",
            ));
        }
        if let Some(s) = &m.signature {
            let summary = match s {
                SignatureChange::Added { signer } => match signer {
                    Some(n) => format!("The macros are now signed by {n}"),
                    None => "The macros are now signed".to_owned(),
                },
                SignatureChange::Removed { signer } => match signer {
                    Some(n) => format!("The macros are no longer signed (by {n})"),
                    None => "The macros are no longer signed".to_owned(),
                },
                SignatureChange::Changed { signer } => match signer {
                    Some(n) => format!("The macros were signed again, by {n}"),
                    None => "The macros were signed again".to_owned(),
                },
            };
            cards.push(object_card("signature", summary, ""));
        }
        if let Some(locked) = m.locked {
            cards.push(object_card(
                "locked",
                if locked {
                    "The macros are now locked for viewing".to_owned()
                } else {
                    "The macros are no longer locked for viewing".to_owned()
                },
                "",
            ));
        }
        out.push(Group {
            category: Category::Structure,
            object: Some(Object::Macros),
            title: "Macros",
            summary: format!(
                "{} to the macros. Lineage never runs macros.",
                plural(cards.len(), "change", "changes")
            ),
            cards,
            more: 0,
        });
    }

    if !d.queries.is_empty() {
        let refreshed_only = d
            .queries
            .iter()
            .all(|q| q.kind == QueryChangeKind::Refreshed);
        let cards: Vec<Card> = d
            .queries
            .iter()
            .map(|q| {
                let mut card = object_card(&q.id, q.summary.clone(), "");
                card.place = Some(format!("Query {}", q.name));
                card.code.clone_from(&q.code);
                card.copy.clone_from(&q.text);
                card.details = without(&q.details, &q.summary);
                card.why = Some(if q.kind == QueryChangeKind::Refreshed {
                    "From a data refresh. Nobody edited the query.".to_owned()
                } else {
                    "Power Query code. Lineage never runs it.".to_owned()
                });
                card
            })
            .collect();
        out.push(Group {
            category: if refreshed_only {
                Category::Refreshed
            } else {
                Category::Structure
            },
            object: Some(Object::Queries),
            title: "Queries",
            summary: format!(
                "{} to Power Query queries",
                plural(cards.len(), "change", "changes")
            ),
            cards,
            more: 0,
        });
    }

    if !d.pivots.is_empty() || !d.slicers.is_empty() {
        let mut cards: Vec<Card> = d
            .pivots
            .iter()
            .map(|p| {
                let mut card = object_card(&p.id, p.summary(), &p.sheet);
                card.place = Some(format!("{} · pivot table {}", p.sheet, p.name));
                card.address = p.area.map(|a| area_name(&a));
                card.details.clone_from(&p.details);
                if p.kind == PivotChangeKind::Changed && p.cells > 0 {
                    card.details.push(format!(
                        "{} in its cells changed with it",
                        plural(p.cells, "value", "values")
                    ));
                }
                card.fields = Some(FieldLists {
                    before: p.before.clone(),
                    after: p.after.clone(),
                });
                card.why = (p.kind == PivotChangeKind::Refreshed)
                    .then(|| "From a data refresh. Nobody typed these values.".to_owned());
                card
            })
            .collect();
        for s in &d.slicers {
            let mut card = object_card(&s.id, s.summary.clone(), "");
            card.place = Some(format!("Slicer {}", s.caption));
            cards.push(card);
        }
        let refreshed_only = d.slicers.is_empty()
            && d.pivots
                .iter()
                .all(|p| p.kind == PivotChangeKind::Refreshed);
        let _ = b;
        out.push(Group {
            category: if refreshed_only {
                Category::Refreshed
            } else {
                Category::Structure
            },
            object: Some(Object::Pivots),
            title: "Pivot tables",
            summary: format!(
                "{} to pivot tables and slicers",
                plural(cards.len(), "change", "changes")
            ),
            cards,
            more: 0,
        });
    }
    out
}

/// Why an item of the merge comes in by hand, in plain words.
#[must_use]
pub fn manual_reason(m: super::merge::Manual) -> String {
    use super::merge::Manual;
    match m {
        Manual::Format => "Formats come in by hand for now. Copy the format from the master.".to_owned(),
        Manual::RowsRemoved => {
            "Removing rows can break the formulas that point at them, so remove these rows by hand.".to_owned()
        }
        Manual::RowsMoved => "Sort or move these rows by hand to match the master.".to_owned(),
        Manual::Columns => "Columns come in by hand for now.".to_owned(),
        Manual::Sheet => "Sheets come in by hand for now.".to_owned(),
        Manual::Settings => "Names, tables, filters and settings come in by hand for now.".to_owned(),
        Manual::Placement => "Lineage couldn't find where this goes in this version. Copy it in by hand.".to_owned(),
        Manual::Name => {
            "This formula uses a name this version doesn't have yet. Add the name, then copy the formula in.".to_owned()
        }
        Manual::Refreshed => "This came from a data refresh. Refresh this workbook's data to get it.".to_owned(),
        Manual::Untracked(kind) => format!("{}: {UNTRACKED} Copy the change in by hand.", kind.label()),
        Manual::Macro => "Macros come in by hand: Lineage can't change macro code in Excel. Open the VBA editor (Alt+F11) and copy the code shown here.".to_owned(),
        Manual::Query => "Queries come in by hand: Lineage can't change query code in Excel. Open Data > Queries & Connections, edit the query and copy the code shown here.".to_owned(),
        Manual::Pivot => "Pivot tables come in by hand: Lineage can't rebuild a pivot table exactly. Change its fields to match the list shown here.".to_owned(),
    }
}

/// A group's place in the panel: inputs and formulas, then macros, queries
/// and pivot tables, then knock-on changes, refreshed data and structure.
fn slot(category: Category, object: Option<Object>) -> u8 {
    match (object, category) {
        (Some(Object::Macros), _) => 2,
        (Some(Object::Queries), _) => 3,
        (Some(Object::Pivots), _) => 4,
        (None, Category::Input) => 0,
        (None, Category::Formula) => 1,
        (None, Category::KnockOn) => 5,
        (None, Category::Refreshed) => 6,
        (None, Category::Structure) => 7,
    }
}

/// The title of a Macros, Queries or Pivot tables group.
#[must_use]
pub fn object_title(object: Object) -> &'static str {
    match object {
        Object::Macros => "Macros",
        Object::Queries => "Queries",
        Object::Pivots => "Pivot tables",
    }
}

/// What a by-hand object item says when this version changed it too.
#[must_use]
pub fn object_both(theirs: &crate::view::Theirs) -> String {
    match theirs {
        crate::view::Theirs::Master => {
            "You changed this too. Compare both versions, then choose by hand.".to_owned()
        }
        crate::view::Theirs::Returned { who } => {
            format!("You and {who} both changed this. Compare both, then choose by hand.")
        }
    }
}

fn conflict_text(c: super::merge::Conflict, theirs: &crate::view::Theirs) -> String {
    use super::merge::Conflict;
    let they = match theirs {
        crate::view::Theirs::Master => "the master".to_owned(),
        crate::view::Theirs::Returned { who } => who.clone(),
    };
    match c {
        Conflict::BothChanged => format!("You and {they} both changed this cell"),
        Conflict::FormulaAndInputs => {
            format!("You changed what this formula reads, or the formula itself, and so did {they}")
        }
        Conflict::EditedVsDeleted => {
            format!("{they} changed a row you removed").replacen("the m", "The m", 1)
        }
        Conflict::BothAdded => format!("You and {they} both added rows here"),
        Conflict::CouldNotMatch => "Lineage couldn't match this row in your version".to_owned(),
    }
}

/// The updates on offer from a merge (`theirs` into this version), as the
/// common lists every panel reads, with the five-category view inside for
/// the Excel pane: cards carry their choices, and a formula filled down a
/// column is one card standing for every cell.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn from_plan(
    p: &super::merge::Plan<'_>,
    theirs: &crate::view::Theirs,
) -> crate::view::WhatChanged {
    use super::merge::{Status, What};
    use crate::view::{ChangeCard, Choice, OverlapCard, WhatChanged};
    let mut out = WhatChanged::default();
    let mut groups: BTreeMap<(u8, Category, Option<Object>), Vec<Card>> = BTreeMap::new();
    let choices_of = |item: &super::merge::Item| -> Vec<Choice> {
        item.choices()
            .into_iter()
            .map(|d| Choice {
                decision: d,
                label: theirs.label(d).to_owned(),
            })
            .collect()
    };
    // Filled-down formula changes with the same status are one card.
    let mut formula_groups: BTreeMap<(usize, u32, u64, String), Vec<&super::merge::Item>> =
        BTreeMap::new();
    for item in &p.items {
        if let What::Cell(c) = &item.what
            && c.category == Category::Formula
            && c.group != 0
        {
            formula_groups
                .entry((c.sheet_b, c.at_b.0, c.group, format!("{:?}", item.status)))
                .or_default()
                .push(item);
        }
    }
    let grouped: std::collections::HashSet<&str> = formula_groups
        .values()
        .filter(|v| v.len() > 1)
        .flat_map(|v| v.iter().map(|i| i.id.as_str()))
        .collect();

    let mut push = |item: &super::merge::Item,
                    mut card: Card,
                    category: Category,
                    object: Option<Object>,
                    out: &mut WhatChanged| {
        card.choices = choices_of(item);
        let text = segments(&card);
        match item.status {
            Status::AlreadyThere => {
                out.already_there.push(ChangeCard {
                    id: item.id.clone(),
                    summary: card.summary.clone(),
                    place: card.place.clone(),
                    text,
                    choices: Vec::new(),
                    by_hand: None,
                });
                return;
            }
            Status::Clean => out.changes.push(ChangeCard {
                id: item.id.clone(),
                summary: card.summary.clone(),
                place: card.place.clone(),
                text,
                choices: card.choices.clone(),
                by_hand: None,
            }),
            Status::ByHand(m) => {
                card.by_hand = Some(manual_reason(m));
                out.by_hand.push(ChangeCard {
                    id: item.id.clone(),
                    summary: card.summary.clone(),
                    place: card.place.clone(),
                    text,
                    choices: card.choices.clone(),
                    by_hand: card.by_hand.clone(),
                });
            }
            Status::Overlap(conflict) => {
                card.conflict = Some(conflict_text(conflict, theirs));
                let side = |s: Option<&CellSide>| -> Vec<crate::diff::Segment> {
                    s.map(|s| s.formula.clone().unwrap_or_else(|| s.shown.clone()))
                        .filter(|t| !t.is_empty())
                        .map(|t| {
                            vec![crate::diff::Segment {
                                kind: crate::diff::SegmentKind::Added,
                                text: t,
                            }]
                        })
                        .unwrap_or_default()
                };
                out.overlaps.push(OverlapCard {
                    id: item.id.clone(),
                    summary: card.conflict.clone().unwrap_or_default(),
                    place: card.place.clone(),
                    before: card
                        .before
                        .as_ref()
                        .map(|b| b.formula.clone().unwrap_or_else(|| b.shown.clone())),
                    mine: side(card.mine.as_ref()),
                    masters: side(card.after.as_ref()),
                    choices: card.choices.clone(),
                });
            }
        }
        if card.ids.is_empty() {
            card.ids = vec![item.id.clone()];
        }
        groups
            .entry((slot(category, object), category, object))
            .or_default()
            .push(card);
    };

    for item in &p.items {
        if grouped.contains(item.id.as_str()) {
            continue;
        }
        match &item.what {
            What::Cell(c) => {
                let mut card = cell_card(c);
                card.mine = item.mine.as_ref().map(|m| m.after.clone());
                push(item, card, c.category, None, &mut out);
            }
            What::Rows { sheet_theirs, rows } => {
                let sheet = p.theirs.sheets.get(*sheet_theirs);
                let name = sheet.map_or(String::new(), |s| s.name.clone());
                let text = p.theirs.sheet_text(*sheet_theirs);
                let lines: Vec<String> = rows
                    .iter()
                    .filter_map(|r| sheet.and_then(|s| s.row(*r)))
                    .map(|row| {
                        row.cells
                            .iter()
                            .map(|c| value::either(p.theirs, text, c).text())
                            .filter(|t| !t.is_empty())
                            .collect::<Vec<_>>()
                            .join(" · ")
                    })
                    .collect();
                let card = Card {
                    id: item.id.clone(),
                    summary: format!("Added {} on {name}", rows_text(rows)),
                    place: Some(name.clone()),
                    sheet: name,
                    address: Some(format!("{}:{}", rows[0], rows[rows.len() - 1])),
                    after: Some(CellSide {
                        shown: lines.join("\n"),
                        formula: None,
                    }),
                    cells: rows.len(),
                    // Excel's own insert formats a new row like the row
                    // above it, and formats come in by hand (decision 0023).
                    why: Some(ROWS_FORMATTED.to_owned()),
                    ..Card::default()
                };
                push(item, card, Category::Structure, None, &mut out);
            }
            What::Structure(s) => {
                push(item, structure_card(s), Category::Structure, None, &mut out);
            }
            What::Object(o) => {
                let mut card = Card {
                    id: item.id.clone(),
                    summary: o.summary.clone(),
                    place: Some(o.name.clone()),
                    sheet: o.sheet.clone(),
                    cells: 1,
                    code: o.code.clone(),
                    copy: o.copy.clone(),
                    details: o.details.clone(),
                    fields: o.fields.clone(),
                    ..Card::default()
                };
                if o.both {
                    card.conflict = Some(object_both(theirs));
                }
                push(item, card, Category::Structure, Some(o.object), &mut out);
            }
        }
    }
    for items in formula_groups.values().filter(|v| v.len() > 1) {
        let What::Cell(first) = &items[0].what else {
            continue;
        };
        let mut card = cell_card(first);
        let rows: Vec<u32> = items
            .iter()
            .filter_map(|i| match &i.what {
                What::Cell(c) => Some(c.at_b.1),
                _ => None,
            })
            .collect();
        let (lo, hi) = (
            rows.iter().min().copied().unwrap_or(1),
            rows.iter().max().copied().unwrap_or(1),
        );
        let range = area_name(&super::model::Area {
            col1: first.at_b.0,
            row1: lo,
            col2: first.at_b.0,
            row2: hi,
        });
        card.summary = format!(
            "The formula changed in {} cells ({}!{range}), filled down",
            items.len(),
            first.sheet
        );
        card.address = Some(range);
        card.cells = items.len();
        card.ids = items.iter().map(|i| i.id.clone()).collect();
        // One card for the panel's lists too, standing for every cell: its
        // id is the first, and the pane sends the decision for all of them.
        push(items[0], card, Category::Formula, None, &mut out);
    }

    let counted: usize = out.changes.len() + out.overlaps.len() + out.by_hand.len();
    out.headline = match counted {
        0 => "Nothing to bring in".to_owned(),
        1 => "1 update".to_owned(),
        n => format!("{n} updates"),
    };
    let mut wv = WorkbookView {
        headline: out.headline.clone(),
        ..WorkbookView::default()
    };
    for ((_, category, object), cards) in groups {
        let n: usize = cards.iter().map(|c| c.cells).sum();
        wv.groups.push(Group {
            object,
            category,
            title: object.map_or_else(|| title(category), object_title),
            summary: plural(n, "change", "changes"),
            cards,
            more: 0,
        });
    }
    wv.new_macro_code = p.master.macros.new_code();
    wv.signature = signature_line(p.theirs);
    let mut kinds: Vec<UnsupportedKind> = p.ours.unsupported.iter().map(|u| u.kind).collect();
    kinds.sort();
    kinds.dedup();
    wv.untracked = kinds
        .into_iter()
        .map(|k| format!("{}: {UNTRACKED}", k.label()))
        .collect();
    out.workbook = Some(Box::new(wv));
    out
}

/// Removed and added text, for a card's line.
fn segments(card: &Card) -> Vec<crate::diff::Segment> {
    use crate::diff::{Segment, SegmentKind};
    let text = |s: &CellSide| s.formula.clone().unwrap_or_else(|| s.shown.clone());
    let mut out = Vec::new();
    if let Some(b) = card.before.as_ref().map(text).filter(|t| !t.is_empty()) {
        out.push(Segment {
            kind: SegmentKind::Removed,
            text: b,
        });
    }
    if let Some(a) = card.after.as_ref().map(text).filter(|t| !t.is_empty()) {
        out.push(Segment {
            kind: SegmentKind::Added,
            text: a,
        });
    }
    out
}

/// The workbook view as the common "what changed" lists every panel and My
/// documents read, with the five-category view inside it for the Excel
/// pane. Formatting-only cards go in `formatting`, everything else in
/// `changes`; knock-on and refreshed groups are one summary card each.
#[must_use]
pub fn what_changed(d: &WorkbookDiff, a: &Workbook, b: &Workbook) -> crate::view::WhatChanged {
    use crate::view::{ChangeCard, WhatChanged};
    let v = view(d, a, b);
    let mut out = WhatChanged {
        headline: if v.groups.is_empty() && v.unmatched.is_empty() {
            "No changes".to_owned()
        } else {
            v.headline.clone()
        },
        ..WhatChanged::default()
    };
    for g in &v.groups {
        let summarised = matches!(g.category, Category::KnockOn | Category::Refreshed);
        if summarised {
            out.changes.push(ChangeCard {
                id: format!(
                    "{}-{}",
                    spec_category(g.category),
                    g.cards.first().map_or("", |c| c.id.as_str())
                ),
                summary: g.summary.clone(),
                place: None,
                text: Vec::new(),
                choices: Vec::new(),
                by_hand: None,
            });
            continue;
        }
        for card in &g.cards {
            let formatting = g.category == Category::Structure
                && card.before.is_some()
                && card.summary.starts_with("Its format");
            let c = ChangeCard {
                id: card.id.clone(),
                summary: card.summary.clone(),
                place: card.place.clone(),
                text: segments(card),
                choices: Vec::new(),
                by_hand: None,
            };
            if formatting {
                out.formatting.push(c);
            } else {
                out.changes.push(c);
            }
        }
    }
    for u in &v.unmatched {
        out.changes.push(ChangeCard {
            id: format!("unmatched-{}", u.sheet),
            summary: u.summary.clone(),
            place: Some(u.sheet.clone()),
            text: Vec::new(),
            choices: Vec::new(),
            by_hand: None,
        });
    }
    out.workbook = Some(Box::new(v));
    out
}

/// The text a person reads, one line per row (cells joined by tabs), and a
/// key of the structure (sheet names and widths), for recognising a file
/// that came back. Large sheets give their first rows only.
#[must_use]
pub fn text_and_structure(wb: &Workbook) -> (Vec<String>, u64) {
    const ROWS: usize = 5000;
    let mut h = crate::hash::Fnv::new();
    let mut lines = Vec::new();
    for (i, sheet) in wb.sheets.iter().enumerate() {
        h.str(&sheet.name);
        let text = wb.sheet_text(i);
        for row in sheet.rows.iter().take(ROWS) {
            let line: Vec<String> = row
                .cells
                .iter()
                .map(|c| value::either(wb, text, c).text())
                .filter(|t| !t.is_empty())
                .collect();
            if !line.is_empty() {
                lines.push(line.join("\t"));
            }
        }
        h.u64(u64::from(
            sheet.rows.last().map_or(0, |r| r.cells.len() as u32),
        ));
    }
    (lines, h.finish())
}

fn spec_category(c: Category) -> &'static str {
    match c {
        Category::Input => "input",
        Category::Formula => "formula",
        Category::KnockOn => "knockOn",
        Category::Refreshed => "refreshed",
        Category::Structure => "structure",
    }
}

fn spec_provenance(p: Provenance) -> &'static str {
    match p {
        Provenance::DirectEdit => "directEdit",
        Provenance::Downstream => "downstream",
        Provenance::Refresh => "refresh",
        Provenance::LikelyDownstream => "likelyDownstream",
        Provenance::Unknown => "unknown",
    }
}

/// The diff in the open change format (`spec/format/v0`).
#[must_use]
pub fn change_list(d: &WorkbookDiff, a: &Workbook, b: &Workbook) -> ChangeList {
    let mut changes = Vec::new();
    let text_of = |side: &CellSide| side.formula.clone().unwrap_or_else(|| side.shown.clone());
    for c in &d.cells {
        let mut location = Location::xlsx(&b.sheets[c.sheet_b].part, &c.sheet, c.sheet_b);
        location.address = Some(a1(c.at_b.0, c.at_b.1));
        if c.at_a != c.at_b {
            location.address_before = Some(a1(c.at_a.0, c.at_a.1));
        }
        location.target = Some("cell".to_owned());
        let (kind, before, after) = if c.category == Category::Structure {
            ("format", Some(text_of(&c.before)), Some(text_of(&c.after)))
        } else if c.before.shown.is_empty() && c.before.formula.is_none() {
            ("insert", None, Some(text_of(&c.after)))
        } else if c.after.shown.is_empty() && c.after.formula.is_none() {
            ("delete", Some(text_of(&c.before)), None)
        } else {
            ("modify", Some(text_of(&c.before)), Some(text_of(&c.after)))
        };
        changes.push(SpecChange {
            id: c.id.clone(),
            kind: kind.to_owned(),
            location,
            from_location: None,
            before,
            after,
            formatting_only: c.category == Category::Structure,
            confidence: 100,
            summary: Some(format!("{}: {}", place(c), cell_card(c).summary)),
            category: Some(spec_category(c.category).to_owned()),
            provenance: Some(spec_provenance(c.provenance).to_owned()),
            edited_by: c.edited_by.map(|e| spec_editor(e).to_owned()),
        });
    }
    for c in &d.calculated {
        let card = calc_card(a, b, d, c);
        let mut location = Location::xlsx(&b.sheets[c.sheet_b].part, &card.sheet, c.sheet_b);
        location.address = Some(a1(c.col, c.row));
        location.target = Some("cell".to_owned());
        changes.push(SpecChange {
            id: format!("c{:x}", {
                let mut h = crate::hash::Fnv::new();
                h.u64(c.sheet_b as u64)
                    .u64(u64::from(c.col))
                    .u64(u64::from(c.row));
                h.finish()
            }),
            kind: "modify".to_owned(),
            location,
            from_location: None,
            before: Some(
                card.before
                    .as_ref()
                    .map(|s| s.shown.clone())
                    .unwrap_or_default(),
            ),
            after: Some(
                card.after
                    .as_ref()
                    .map(|s| s.shown.clone())
                    .unwrap_or_default(),
            ),
            formatting_only: false,
            confidence: if c.provenance == Provenance::LikelyDownstream {
                70
            } else {
                100
            },
            summary: None,
            category: Some(spec_category(c.category).to_owned()),
            provenance: Some(spec_provenance(c.provenance).to_owned()),
            edited_by: None,
        });
    }
    for s in &d.structure {
        let sheet_index = b.sheet_index(&s.sheet).or_else(|| a.sheet_index(&s.sheet));
        let part = match (sheet_index, &s.kind) {
            (_, StructureKind::SheetRemoved) => a
                .sheet_index(&s.sheet)
                .map_or_else(|| a.main_part.clone(), |i| a.sheets[i].part.clone()),
            (Some(i), _) if !s.sheet.is_empty() => b
                .sheets
                .get(i)
                .map_or_else(|| b.main_part.clone(), |x| x.part.clone()),
            _ => b.main_part.clone(),
        };
        let mut location = Location::xlsx(&part, &s.sheet, sheet_index.unwrap_or(0));
        location.address = structure_address(s);
        let (kind, target) = match &s.kind {
            StructureKind::SheetAdded => ("insert", "sheet"),
            StructureKind::SheetRemoved => ("delete", "sheet"),
            StructureKind::RowsAdded { .. } => ("insert", "rows"),
            StructureKind::RowsRemoved { .. } => ("delete", "rows"),
            StructureKind::ColumnsAdded { .. } => ("insert", "columns"),
            StructureKind::ColumnsRemoved { .. } => ("delete", "columns"),
            StructureKind::RowsMoved { .. } | StructureKind::RowsSorted { .. } => {
                ("modify", "rows")
            }
            StructureKind::RowsHidden { .. }
            | StructureKind::RowsShown { .. }
            | StructureKind::RowFormat { .. } => ("format", "rows"),
            StructureKind::ColumnsHidden { .. }
            | StructureKind::ColumnsShown { .. }
            | StructureKind::ColumnFormat { .. } => ("format", "columns"),
            StructureKind::NameAdded { .. } => ("insert", "name"),
            StructureKind::NameRemoved { .. } => ("delete", "name"),
            StructureKind::NameChanged { .. } => ("modify", "name"),
            StructureKind::TableAdded { .. } => ("insert", "table"),
            StructureKind::TableRemoved { .. } => ("delete", "table"),
            StructureKind::TableChanged { .. } => ("modify", "table"),
            StructureKind::ConnectionChanged { .. } => ("modify", "connection"),
            StructureKind::WorkbookSettings | StructureKind::Calculation => ("modify", "workbook"),
            StructureKind::Untracked { .. } => ("modify", "untracked"),
            _ => ("modify", "sheet"),
        };
        location.target = Some(target.to_owned());
        let summary = structure_summary(s);
        let (before, after) = match kind {
            "insert" => (None, Some(summary.clone())),
            "delete" => (Some(summary.clone()), None),
            _ => (Some(String::new()), Some(summary.clone())),
        };
        changes.push(SpecChange {
            id: s.id.clone(),
            kind: kind.to_owned(),
            location,
            from_location: None,
            before,
            after,
            formatting_only: kind == "format",
            confidence: 100,
            summary: Some(summary),
            category: Some("structure".to_owned()),
            provenance: Some("unknown".to_owned()),
            edited_by: None,
        });
    }
    changes.extend(object_changes(d, a, b));
    ChangeList {
        spec_version: crate::SPEC_VERSION.to_owned(),
        format: "xlsx".to_owned(),
        changes,
    }
}

/// Macros, queries, pivot tables and slicers in the open change format:
/// `target` says which, and `object` names it (Phase 5B).
fn object_changes(d: &WorkbookDiff, a: &Workbook, b: &Workbook) -> Vec<SpecChange> {
    use super::pivot::PivotChangeKind;
    use super::query::QueryChangeKind;
    use crate::vba::diff::ModuleChangeKind;
    struct One<'s> {
        id: &'s str,
        kind: &'s str,
        part: String,
        sheet: String,
        sheet_index: usize,
        address: Option<String>,
        target: &'s str,
        object: String,
        summary: String,
        refreshed: bool,
    }
    let make = |o: One<'_>| {
        let mut location = Location::xlsx(&o.part, &o.sheet, o.sheet_index);
        location.address = o.address;
        location.target = Some(o.target.to_owned());
        location.object = Some(o.object);
        let (before, after) = match o.kind {
            "insert" => (None, Some(o.summary.clone())),
            "delete" => (Some(o.summary.clone()), None),
            _ => (Some(String::new()), Some(o.summary.clone())),
        };
        SpecChange {
            id: o.id.to_owned(),
            kind: o.kind.to_owned(),
            location,
            from_location: None,
            before,
            after,
            formatting_only: false,
            confidence: 100,
            summary: Some(o.summary),
            category: Some(
                if o.refreshed {
                    "refreshed"
                } else {
                    "structure"
                }
                .to_owned(),
            ),
            provenance: Some(if o.refreshed { "refresh" } else { "unknown" }.to_owned()),
            edited_by: None,
        }
    };
    let mut out = Vec::new();
    let vba_part = b
        .vba_part
        .clone()
        .or_else(|| a.vba_part.clone())
        .unwrap_or_else(|| "xl/vbaProject.bin".to_owned());
    for m in &d.macros.modules {
        let kind = match m.change {
            ModuleChangeKind::Added => "insert",
            ModuleChangeKind::Removed => "delete",
            _ => "modify",
        };
        out.push(make(One {
            id: &m.id,
            kind,
            part: vba_part.clone(),
            sheet: String::new(),
            sheet_index: 0,
            address: None,
            target: "macro",
            object: m.name.clone(),
            summary: m.summary.clone(),
            refreshed: false,
        }));
    }
    // References, the signature and the lock: the project's own settings.
    let mut project_lines: Vec<(String, &str, String)> = Vec::new();
    for r in &d.macros.references_added {
        project_lines.push((
            format!("Reference {r}"),
            "insert",
            format!("Added reference {r}"),
        ));
    }
    for r in &d.macros.references_removed {
        project_lines.push((
            format!("Reference {r}"),
            "delete",
            format!("Removed reference {r}"),
        ));
    }
    if d.macros.signature.is_some() {
        let summary = object_groups(d, b)
            .into_iter()
            .flat_map(|g| g.cards)
            .find(|c| c.id == "signature")
            .map_or_else(|| "The signature changed".to_owned(), |c| c.summary);
        project_lines.push(("Signature".to_owned(), "modify", summary));
    }
    if let Some(locked) = d.macros.locked {
        project_lines.push((
            "Lock".to_owned(),
            "modify",
            if locked {
                "The macros are now locked for viewing".to_owned()
            } else {
                "The macros are no longer locked for viewing".to_owned()
            },
        ));
    }
    for (object, kind, summary) in project_lines {
        let id = crate::hash::sha256_hex(
            format!("macro-project\u{1f}{object}\u{1f}{summary}").as_bytes(),
        )[..16]
            .to_owned();
        out.push(make(One {
            id: &id,
            kind,
            part: vba_part.clone(),
            sheet: String::new(),
            sheet_index: 0,
            address: None,
            target: "macro",
            object,
            summary,
            refreshed: false,
        }));
    }
    let query_part = b
        .queries
        .as_ref()
        .or(a.queries.as_ref())
        .map_or_else(|| "customXml/item1.xml".to_owned(), |q| q.part.clone());
    for q in &d.queries {
        let kind = match q.kind {
            QueryChangeKind::Added => "insert",
            QueryChangeKind::Removed => "delete",
            _ => "modify",
        };
        out.push(make(One {
            id: &q.id,
            kind,
            part: query_part.clone(),
            sheet: String::new(),
            sheet_index: 0,
            address: None,
            target: "query",
            object: q.name.clone(),
            summary: q.summary.clone(),
            refreshed: q.kind == QueryChangeKind::Refreshed,
        }));
    }
    for p in &d.pivots {
        let (wb, kind) = match p.kind {
            PivotChangeKind::Added => (b, "insert"),
            PivotChangeKind::Removed => (a, "delete"),
            _ => (b, "modify"),
        };
        let table = wb.pivots.tables.iter().find(|t| t.name == p.name);
        out.push(make(One {
            id: &p.id,
            kind,
            part: table.map_or_else(|| wb.main_part.clone(), |t| t.part.clone()),
            sheet: p.sheet.clone(),
            sheet_index: wb.sheet_index(&p.sheet).unwrap_or(0),
            address: p.area.map(|area| area_name(&area)),
            target: "pivot",
            object: p.name.clone(),
            summary: p.summary(),
            refreshed: p.kind == PivotChangeKind::Refreshed,
        }));
    }
    for s in &d.slicers {
        let part = b
            .pivots
            .slicers
            .iter()
            .chain(&a.pivots.slicers)
            .find(|x| x.caption == s.caption)
            .map_or_else(|| b.main_part.clone(), |x| x.part.clone());
        out.push(make(One {
            id: &s.id,
            kind: "modify",
            part,
            sheet: String::new(),
            sheet_index: 0,
            address: None,
            target: "slicer",
            object: s.caption.clone(),
            summary: s.summary.clone(),
            refreshed: false,
        }));
    }
    out
}

/// One cell as the side by side shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CellDetail {
    /// Where it is in its own version: "B4".
    pub address: String,
    /// What Excel shows, through the number format.
    pub shown: String,
    /// The formula, with `=`, when there is one.
    pub formula: Option<String>,
    /// What is stored: the value typed, or a formula's last result.
    pub value: String,
    /// "number", "text", "true or false", "error", "date" or "empty".
    pub kind: &'static str,
    /// The number format, when it isn't General.
    pub format: Option<String>,
}

/// A cell in the later version beside the same cell in the earlier one,
/// matched through the row and column alignment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SideBySide {
    pub sheet: String,
    /// `None` when the earlier version has no such cell (a new sheet, row or
    /// column).
    pub before: Option<CellDetail>,
    pub after: Option<CellDetail>,
}

fn detail(wb: &Workbook, sheet: usize, col: u32, row: u32) -> CellDetail {
    let text = wb.sheet_text(sheet);
    let cell = wb.sheets[sheet].cell(col, row);
    let (stored, shown, format) = match cell {
        Some(cell) => {
            let v = value::input(wb, text, cell)
                .or_else(|| value::cached(text, cell))
                .unwrap_or(value::Value::Blank);
            let code = wb.styles.format_code(cell.style);
            let shown = display(&v, code, wb.date1904);
            let format = (!code.eq_ignore_ascii_case("general")).then(|| code.to_owned());
            (v, shown, format)
        }
        None => (value::Value::Blank, String::new(), None),
    };
    CellDetail {
        address: a1(col, row),
        shown,
        formula: super::deps::formula_at(wb, sheet, row, col).map(|f| format!("={f}")),
        value: stored.text(),
        kind: match stored {
            value::Value::Blank => "empty",
            value::Value::Text(_) => "text",
            value::Value::Number(_) => "number",
            value::Value::Bool(_) => "true or false",
            value::Value::Error(_) => "error",
            value::Value::Date(_) => "date",
        },
        format,
    }
}

/// The side by side for `address` on `sheet` of `b`, beside where that cell
/// is in `a`. `None` when `b` has no such sheet, or `address` isn't a cell.
#[must_use]
pub fn side_by_side(a: &Workbook, b: &Workbook, sheet: &str, address: &str) -> Option<SideBySide> {
    let j = b.sheet_index(sheet)?;
    let (col, row) = super::refs::parse_cell(address)?;
    let before = super::align::pair_sheets(a, b)
        .both
        .iter()
        .find(|(_, jb)| *jb == j)
        .and_then(|(i, _)| {
            let al = super::align::align(a, *i, b, j);
            let (r, c) = (al.rows.b_to_a.get(&row)?, al.cols.b_to_a.get(&col)?);
            Some(detail(a, *i, *c, *r))
        });
    Some(SideBySide {
        sheet: b.sheets[j].name.clone(),
        before,
        after: Some(detail(b, j, col, row)),
    })
}

#[cfg(test)]
mod tests {
    use lineage_testkit::workbooks;

    use super::*;
    use crate::xlsx::diff::diff;

    fn open(wb: &lineage_testkit::xlsx::Workbook) -> Workbook {
        Workbook::open(&wb.to_bytes()).expect("opens")
    }

    #[test]
    fn a_price_change_reads_as_people_say_it() {
        let (a, b) = (
            open(&workbooks::model_master()),
            open(&workbooks::model_price_update()),
        );
        let v = view(&diff(&a, &b, &[]), &a, &b);
        assert_eq!(v.headline, "1 input, 83 knock-on changes");
        let inputs = &v.groups[0];
        assert_eq!(inputs.title, "Inputs");
        assert_eq!(inputs.cards[0].summary, "'$49' changed to '$55'");
        assert_eq!(
            inputs.cards[0].place.as_deref(),
            Some("Assumptions!B4 · Price per seat, Value")
        );
        let knock_on = &v.groups[1];
        assert!(
            knock_on
                .summary
                .starts_with("This changed 83 calculated cells, including Summary!"),
            "{}",
            knock_on.summary
        );
        assert_eq!(knock_on.cards.len(), EXAMPLES);
        assert_eq!(knock_on.more, 83 - EXAMPLES);
        assert_eq!(
            knock_on.cards[0].why.as_deref(),
            Some("Follows from a change above")
        );
    }

    #[test]
    fn a_sort_reads_as_one_line() {
        let (a, b) = (
            open(&workbooks::budget_master()),
            open(&workbooks::budget_sorted()),
        );
        let v = view(&diff(&a, &b, &[]), &a, &b);
        assert_eq!(v.groups.len(), 1);
        assert_eq!(
            v.groups[0].cards[0].summary,
            "Sorted the rows on Transactions by Amount, largest first"
        );
    }

    #[test]
    fn macros_are_labelled_as_kept_not_tracked() {
        let a = open(&workbooks::model_xlsm());
        let v = view(&diff(&a, &a, &[]), &a, &a);
        assert_eq!(v.headline, "No changes");
        assert_eq!(
            v.untracked,
            ["Macros: Preserved; semantic tracking not yet supported."]
        );
    }

    #[test]
    fn refreshed_data_says_nobody_typed_it() {
        let (a, b) = (
            open(&workbooks::connected_master()),
            open(&workbooks::connected_refreshed()),
        );
        let v = view(&diff(&a, &b, &[]), &a, &b);
        assert_eq!(v.groups.len(), 1);
        assert_eq!(v.groups[0].title, "Refreshed data");
        assert!(v.groups[0].summary.contains("Nobody typed them"));
    }

    #[test]
    fn a_cell_side_by_side_follows_its_row() {
        let a = open(&workbooks::budget_master());
        let b = open(&workbooks::budget_sorted());
        // Sorted largest first: January rent is row 2 in both, Lunch with
        // Priya moved.
        let sb = side_by_side(&a, &b, "Transactions", "D2").expect("a cell");
        let (before, after) = (sb.before.expect("before"), sb.after.expect("after"));
        assert_eq!(after.value, "1450");
        assert_eq!(after.kind, "number");
        assert_eq!(before.value, after.value);
        let lunch = (2..=17)
            .find(|r| {
                side_by_side(&a, &b, "Transactions", &format!("C{r}"))
                    .and_then(|s| s.after)
                    .is_some_and(|d| d.value == "Lunch with Priya")
            })
            .expect("lunch is there");
        let sb = side_by_side(&a, &b, "Transactions", &format!("C{lunch}")).expect("a cell");
        let before = sb.before.expect("matched");
        assert_eq!(before.value, "Lunch with Priya");
        assert_eq!(before.address, "C6", "where it was before the sort");
        // A formula: its text, its last result, and what it shows.
        let total = side_by_side(&a, &b, "Summary", "B8").expect("a cell");
        let after = total.after.expect("after");
        assert_eq!(after.formula.as_deref(), Some("=SUM(B2:B7)"));
        assert_eq!(after.kind, "number");
        assert_eq!(after.shown, "4,271.65", "through its number format");
        assert!(after.format.is_some());
        assert!(side_by_side(&a, &b, "Nowhere", "A1").is_none());
        assert!(side_by_side(&a, &b, "Summary", "not a cell").is_none());
    }
}
