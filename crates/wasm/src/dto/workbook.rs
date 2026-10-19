// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What crosses the boundary for workbooks: "What changed" in the five
//! categories (decision 0022), the apply plan the Excel pane carries out
//! (decision 0023), a cell side by side, and the event hints the pane
//! passes in.

use lineage::workbooks::WorkbookPlan;
use lineage_core::xlsx::apply::{Expect, Skipped, Step, Write};
use lineage_core::xlsx::diff::{Category, CellSide, Hint};
use lineage_core::xlsx::refs::{a1, parse_area};
use lineage_core::xlsx::view::{Card, CellDetail, Group, SideBySide, WorkbookView};
use serde::{Deserialize, Serialize};

use super::{ChoiceDto, choices};

// ---------------------------------------------------------------- inputs

/// A range Excel reported as edited in this session, not by Lineage
/// (`onChanged` with a `triggerSource` other than this add-in).
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct HintDto {
    pub sheet: String,
    /// "B4" or "B4:C9", without the sheet.
    pub address: String,
}

impl HintDto {
    /// The engine's hint, or `None` when the address isn't a cell or range.
    #[must_use]
    pub fn to_hint(&self) -> Option<Hint> {
        if self.sheet.is_empty() || self.sheet.len() > 255 {
            return None;
        }
        Some(Hint {
            sheet: self.sheet.clone(),
            area: parse_area(&self.address)?,
        })
    }
}

/// One cell for the side by side.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CellArgs {
    pub sheet: String,
    pub address: String,
}

// ---------------------------------------------------------------- outputs

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum CategoryDto {
    Input,
    Formula,
    KnockOn,
    Refreshed,
    Structure,
}

impl From<Category> for CategoryDto {
    fn from(c: Category) -> Self {
        match c {
            Category::Input => Self::Input,
            Category::Formula => Self::Formula,
            Category::KnockOn => Self::KnockOn,
            Category::Refreshed => Self::Refreshed,
            Category::Structure => Self::Structure,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CellSideDto {
    pub shown: String,
    pub formula: Option<String>,
}

impl From<&CellSide> for CellSideDto {
    fn from(s: &CellSide) -> Self {
        Self {
            shown: s.shown.clone(),
            formula: s.formula.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookCardDto {
    pub id: String,
    pub summary: String,
    pub place: Option<String>,
    pub sheet: String,
    /// Where "Go to cell" jumps, in the later version.
    pub address: Option<String>,
    pub before: Option<CellSideDto>,
    pub after: Option<CellSideDto>,
    pub cells: usize,
    pub why: Option<String>,
    /// For updates and reviews: the item ids the card's choice decides.
    pub ids: Vec<String>,
    pub choices: Vec<ChoiceDto>,
    pub by_hand: Option<String>,
    pub mine: Option<CellSideDto>,
    pub conflict: Option<String>,
}

impl From<&Card> for WorkbookCardDto {
    fn from(c: &Card) -> Self {
        Self {
            id: c.id.clone(),
            summary: c.summary.clone(),
            place: c.place.clone(),
            sheet: c.sheet.clone(),
            address: c.address.clone(),
            before: c.before.as_ref().map(CellSideDto::from),
            after: c.after.as_ref().map(CellSideDto::from),
            cells: c.cells,
            why: c.why.clone(),
            ids: c.ids.clone(),
            choices: choices(&c.choices),
            by_hand: c.by_hand.clone(),
            mine: c.mine.as_ref().map(CellSideDto::from),
            conflict: c.conflict.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookGroupDto {
    pub category: CategoryDto,
    pub title: String,
    pub summary: String,
    pub cards: Vec<WorkbookCardDto>,
    pub more: usize,
}

impl From<&Group> for WorkbookGroupDto {
    fn from(g: &Group) -> Self {
        Self {
            category: g.category.into(),
            title: g.title.to_owned(),
            summary: g.summary.clone(),
            cards: g.cards.iter().map(WorkbookCardDto::from).collect(),
            more: g.more,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnmatchedDto {
    pub sheet: String,
    pub summary: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookViewDto {
    pub headline: String,
    pub groups: Vec<WorkbookGroupDto>,
    pub unmatched: Vec<UnmatchedDto>,
    pub untracked: Vec<String>,
    pub notes: Vec<String>,
}

impl From<&WorkbookView> for WorkbookViewDto {
    fn from(v: &WorkbookView) -> Self {
        Self {
            headline: v.headline.clone(),
            groups: v.groups.iter().map(WorkbookGroupDto::from).collect(),
            unmatched: v
                .unmatched
                .iter()
                .map(|u| UnmatchedDto {
                    sheet: u.sheet.clone(),
                    summary: u.summary.clone(),
                })
                .collect(),
            untracked: v.untracked.clone(),
            notes: v.notes.clone(),
        }
    }
}

/// What one step writes into a cell.
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum WriteDto {
    Clear,
    /// A number by its lexeme, written as Excel would read it from a file.
    Number {
        text: String,
    },
    /// Text exactly, never turned into a number or a formula.
    Text {
        text: String,
    },
    Bool {
        value: bool,
    },
    Error {
        text: String,
    },
    /// En-US, with the `=`.
    Formula {
        text: String,
    },
}

impl From<&Write> for WriteDto {
    fn from(w: &Write) -> Self {
        match w {
            Write::Clear => Self::Clear,
            Write::Number(t) => Self::Number { text: t.clone() },
            Write::Text(t) => Self::Text { text: t.clone() },
            Write::Bool(b) => Self::Bool { value: *b },
            Write::Error(t) => Self::Error { text: t.clone() },
            Write::Formula(t) => Self::Formula {
                text: format!("={t}"),
            },
        }
    }
}

/// One step for the Excel pane, at the addresses the workbook has when the
/// step runs (rows are inserted from the bottom up first).
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StepDto {
    /// Insert whole rows: `rows` is "5:7" for three rows at 5.
    InsertRows {
        sheet: String,
        at: u32,
        count: u32,
        rows: String,
    },
    DeleteRows {
        sheet: String,
        at: u32,
        count: u32,
        rows: String,
    },
    Set {
        sheet: String,
        address: String,
        value: WriteDto,
    },
}

fn rows(at: u32, count: u32) -> String {
    format!("{at}:{}", at + count.saturating_sub(1))
}

impl From<&Step> for StepDto {
    fn from(s: &Step) -> Self {
        match s {
            Step::InsertRows { sheet, at, count } => Self::InsertRows {
                sheet: sheet.clone(),
                at: *at,
                count: *count,
                rows: rows(*at, *count),
            },
            Step::DeleteRows { sheet, at, count } => Self::DeleteRows {
                sheet: sheet.clone(),
                at: *at,
                count: *count,
                rows: rows(*at, *count),
            },
            Step::Set {
                sheet,
                col,
                row,
                value,
            } => Self::Set {
                sheet: sheet.clone(),
                address: a1(*col, *row),
                value: value.into(),
            },
        }
    }
}

/// What a cell must hold before the plan runs.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpectDto {
    pub sheet: String,
    pub address: String,
    pub value: WriteDto,
}

impl From<&Expect> for ExpectDto {
    fn from(e: &Expect) -> Self {
        Self {
            sheet: e.sheet.clone(),
            address: a1(e.col, e.row),
            value: (&e.value).into(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedDto {
    pub id: String,
    pub reason: String,
}

impl From<&Skipped> for SkippedDto {
    fn from(s: &Skipped) -> Self {
        Self {
            id: s.id.clone(),
            reason: s.reason.clone(),
        }
    }
}

/// The plan the Excel pane carries out, and the way back.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkbookPlanDto {
    pub steps: Vec<StepDto>,
    pub undo: Vec<StepDto>,
    pub expect: Vec<ExpectDto>,
    pub skipped: Vec<SkippedDto>,
    pub brought: usize,
    pub deferred: usize,
    pub text: String,
}

impl From<&WorkbookPlan> for WorkbookPlanDto {
    fn from(p: &WorkbookPlan) -> Self {
        Self {
            steps: p.steps.iter().map(StepDto::from).collect(),
            undo: p.undo.iter().map(StepDto::from).collect(),
            expect: p.expect.iter().map(ExpectDto::from).collect(),
            skipped: p.skipped.iter().map(SkippedDto::from).collect(),
            brought: p.brought,
            deferred: p.deferred,
            text: p.text.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CellDetailDto {
    pub address: String,
    pub shown: String,
    pub formula: Option<String>,
    pub value: String,
    pub kind: String,
    pub format: Option<String>,
}

impl From<&CellDetail> for CellDetailDto {
    fn from(d: &CellDetail) -> Self {
        Self {
            address: d.address.clone(),
            shown: d.shown.clone(),
            formula: d.formula.clone(),
            value: d.value.clone(),
            kind: d.kind.to_owned(),
            format: d.format.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SideBySideDto {
    pub sheet: String,
    pub before: Option<CellDetailDto>,
    pub after: Option<CellDetailDto>,
}

impl From<&SideBySide> for SideBySideDto {
    fn from(s: &SideBySide) -> Self {
        Self {
            sheet: s.sheet.clone(),
            before: s.before.as_ref().map(CellDetailDto::from),
            after: s.after.as_ref().map(CellDetailDto::from),
        }
    }
}
