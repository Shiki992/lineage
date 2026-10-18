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
    Calculated, Category, CellChange, CellSide, Provenance, StructureChange, StructureKind,
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

/// How many examples a summarised group shows.
pub const EXAMPLES: usize = 5;

/// One card: a cell, a group of cells, or a structure change.
#[derive(Clone, Debug, PartialEq, Eq)]
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub category: Category,
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
}

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
        why: why(c.category, c.provenance),
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
    }
}

/// The address of a calculated cell, with its sheet.
fn calc_address(b: &Workbook, c: &Calculated) -> String {
    let name = b.sheets.get(c.sheet_b).map_or("", |s| s.name.as_str());
    let card = CellChange {
        id: String::new(),
        category: c.category,
        provenance: c.provenance,
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
            category: Category::Formula,
            title: title(Category::Formula),
            summary: format!("{} changed", plural(formulas.len(), "formula", "formulas")),
            cards: cards.into_iter().map(|(_, c)| c).collect(),
            more: 0,
        });
    }

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
                });
            }
        }
        let n = structure.len() + format_cells.len();
        groups.push(Group {
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

    let counted: Vec<String> = groups
        .iter()
        .map(|g| {
            let n = g.cards.iter().map(|c| c.cells).sum::<usize>() + g.more;
            let (one, many) = match g.category {
                Category::Input => ("input", "inputs"),
                Category::Formula => ("formula", "formulas"),
                Category::KnockOn => ("knock-on change", "knock-on changes"),
                Category::Refreshed => ("refreshed value", "refreshed values"),
                Category::Structure => {
                    ("structure or format change", "structure or format changes")
                }
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
    }
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
        });
    }
    ChangeList {
        spec_version: crate::SPEC_VERSION.to_owned(),
        format: "xlsx".to_owned(),
        changes,
    }
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
}
