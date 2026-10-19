// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The open change format, `spec/format/v0`: serde types and conversion from the
//! engine's changes. The JSON Schema in `spec/format/v0/` is the contract; tests
//! validate everything this module writes against it.

use serde::{Deserialize, Serialize};

use super::{CellStep, Change, ChangeKind, Path};

/// A change list: `{ specVersion, format, changes }`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeList {
    pub spec_version: String,
    pub format: String,
    pub changes: Vec<SpecChange>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SpecChange {
    pub id: String,
    pub kind: String,
    pub location: Location,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_location: Option<Location>,
    pub before: Option<String>,
    pub after: Option<String>,
    pub formatting_only: bool,
    pub confidence: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// xlsx: input, formula, knockOn, refreshed or structure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// xlsx: directEdit, downstream, refresh, likelyDownstream or unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Location {
    pub format: String,
    pub part: String,
    /// docx: the block's index in the part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub char_range: Option<CharRange>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<Box<CellLocation>>,
    /// pptx: the slide's position in show order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slide_index: Option<usize>,
    /// pptx: the slide's `p:sldId`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slide_id: Option<u32>,
    /// pptx: the shape's `p:cNvPr` id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape_id: Option<u32>,
    /// pptx and xlsx: what changed when it is not one shape or one cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// xlsx: the sheet's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet: Option<String>,
    /// xlsx: the sheet's position among the tabs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sheet_index: Option<usize>,
    /// xlsx: the cell or range in A1 style, in the version the change is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    /// xlsx: where the cell was in the earlier version, when rows or
    /// columns moved it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address_before: Option<String>,
    /// xlsx: the macro module, query, pivot table or slicer, by name, for
    /// those targets.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
}

impl Location {
    /// A deck location.
    #[must_use]
    pub fn pptx(part: &str, slide_index: usize) -> Self {
        Self {
            format: "pptx".to_owned(),
            part: part.to_owned(),
            block_index: None,
            block_anchor: None,
            char_range: None,
            cell: None,
            slide_index: Some(slide_index),
            ..Self::default()
        }
    }

    /// A workbook location.
    #[must_use]
    pub fn xlsx(part: &str, sheet: &str, sheet_index: usize) -> Self {
        Self {
            format: "xlsx".to_owned(),
            part: part.to_owned(),
            sheet: Some(sheet.to_owned()),
            sheet_index: Some(sheet_index),
            ..Self::default()
        }
    }
}

/// A place inside a table: a row, and optionally a cell and a block in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CellLocation {
    pub row: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<Box<CellLocation>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CharRange {
    pub start: usize,
    pub end: usize,
}

fn cell_location(steps: &[CellStep]) -> Option<Box<CellLocation>> {
    let (first, rest) = steps.split_first()?;
    Some(Box::new(CellLocation {
        row: first.row,
        column: first.column,
        block_index: first.block,
        cell: cell_location(rest),
    }))
}

fn location(part: &str, path: &Path, change: &Change, with_range: bool) -> Location {
    Location {
        format: "docx".to_owned(),
        part: part.to_owned(),
        block_index: Some(path.block),
        block_anchor: change.anchor.clone(),
        char_range: if with_range {
            change
                .char_range
                .map(|(start, end)| CharRange { start, end })
        } else {
            None
        },
        cell: cell_location(&path.cells),
        ..Location::default()
    }
}

/// The change list for `changes`. `summary` gives each change its plain-English
/// summary, if any.
#[must_use]
pub fn change_list(changes: &[Change], summary: impl Fn(&Change) -> Option<String>) -> ChangeList {
    let changes = changes
        .iter()
        .filter_map(|change| {
            let (path, from) = match change.kind {
                ChangeKind::Delete => (change.before.as_ref()?, None),
                ChangeKind::Move => (change.after.as_ref()?, change.before.as_ref()),
                _ => (change.after.as_ref()?, None),
            };
            let with_range = matches!(change.kind, ChangeKind::Modify | ChangeKind::Format);
            Some(SpecChange {
                id: change.id.clone(),
                kind: change.kind.as_str().to_owned(),
                location: location(&change.part, path, change, with_range),
                from_location: from.map(|p| location(&change.part, p, change, false)),
                before: change.before_text.clone(),
                after: change.after_text.clone(),
                formatting_only: change.formatting_only,
                confidence: change.confidence,
                summary: summary(change).filter(|s| !s.is_empty()),
                category: None,
                provenance: None,
            })
        })
        .collect();
    ChangeList {
        spec_version: crate::SPEC_VERSION.to_owned(),
        format: "docx".to_owned(),
        changes,
    }
}

impl ChangeList {
    /// The change list as JSON.
    ///
    /// # Panics
    /// Never: the types serialise infallibly.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("change lists always serialise")
    }
}
