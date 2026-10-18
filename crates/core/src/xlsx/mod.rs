// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Excel workbooks (.xlsx and .xlsm): opening a package into the workbook
//! model, and everything the engine does with workbooks.
//!
//! The saved file is the source of truth (decision 0020). Office.js reads in
//! the task pane are hints only; every comparison here works from bytes.

pub mod deps;
pub mod formula;
pub mod model;
mod read;
pub mod refs;
mod sheet;
mod stream;
mod strings;
pub mod styles;

pub use model::*;
pub use read::{CT_TEMPLATE, CT_TEMPLATE_MACRO, CT_WORKBOOK, CT_WORKBOOK_MACRO};
pub use sheet::MAX_CELLS;
pub use styles::Styles;

use crate::error::{Error, Result};
use crate::package::{Package, Relationships, resolve_target};
use crate::xml;

/// An opened workbook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Workbook {
    pub package: Package,
    /// The workbook part, usually `xl/workbook.xml`.
    pub main_part: String,
    pub main_rels: Relationships,
    /// `.xlsm`: the main part is macro-enabled.
    pub macro_enabled: bool,
    /// Tabs in order.
    pub sheets: Vec<Sheet>,
    pub names: Vec<DefinedName>,
    pub strings: Vec<SharedString>,
    pub styles: Styles,
    pub tables: Vec<TableDef>,
    /// Ranges that data connections fill: (sheet position, area).
    pub query_areas: Vec<(usize, Area)>,
    pub connections: Vec<ConnectionDef>,
    pub unsupported: Vec<Unsupported>,
    /// Dates count from 1904 (old Mac workbooks).
    pub date1904: bool,
    /// Workbook settings and protection, as a key.
    pub settings: u64,
    /// Calculation settings (`calcPr`), as a key.
    pub calc: u64,
}

impl Workbook {
    /// Opens a workbook.
    ///
    /// # Errors
    /// [`Error`] when the bytes are not a workbook this version can read.
    pub fn open(bytes: &[u8]) -> Result<Self> {
        Self::from_package(Package::read(bytes)?)
    }

    /// Builds the model of an already-read package.
    ///
    /// # Errors
    /// [`Error`] when the package is not a workbook this version can read.
    pub fn from_package(package: Package) -> Result<Self> {
        let (main_part, macro_enabled) = read::main_part(&package)?;
        let main_rels = read::rels(&package, &main_part)?;
        let wb_part = read::workbook_part(&package, &main_part, &main_rels)?;

        let strings = match main_rels.of_kind("sharedStrings").next() {
            Some(rel) => {
                let part = resolve_target(&main_part, &rel.target);
                match package.part(&part) {
                    Some(bytes) => strings::read(&part, xml::text(&part, bytes)?)?,
                    None => Vec::new(),
                }
            }
            None => Vec::new(),
        };
        let styles_part = main_rels
            .of_kind("styles")
            .next()
            .map(|rel| resolve_target(&main_part, &rel.target));
        let styles = match &styles_part {
            Some(part) => styles::read(part, package.part(part))?,
            None => Styles::default(),
        };

        let mut cells = 0usize;
        let mut sheets = Vec::with_capacity(wb_part.sheets.len());
        let mut tables = Vec::new();
        let mut query_areas = Vec::new();
        for (index, entry) in wb_part.sheets.iter().enumerate() {
            let mut sheet = Sheet {
                name: entry.name.clone(),
                sheet_id: entry.sheet_id,
                part: entry.part.clone(),
                state: entry.state,
                kind: entry.kind,
                ..Sheet::default()
            };
            if entry.kind != SheetKind::Worksheet {
                sheets.push(sheet);
                continue;
            }
            let bytes = package
                .part(&entry.part)
                .ok_or_else(|| Error::Damaged("a worksheet part is missing".to_owned()))?;
            let text = xml::text(&entry.part, bytes)?;
            let read = sheet::read(&entry.part, text, &mut cells)?;
            let sheet_rels = read::rels(&package, &entry.part)?;
            for rid in &read.table_rids {
                if let Some(rel) = sheet_rels.by_id(rid) {
                    let part = resolve_target(&entry.part, &rel.target);
                    if let Some(table) = read::table(&package, &part, index)? {
                        if table.query {
                            query_areas.push((index, table.area));
                        }
                        tables.push(table);
                    }
                }
            }
            for rel in sheet_rels.of_kind("queryTable") {
                let part = resolve_target(&entry.part, &rel.target);
                if let Some(area) = read::query_area(&wb_part.names, index, &part, &package) {
                    query_areas.push((index, area));
                }
            }
            for rid in read.drawing_rids.iter() {
                if let Some(rel) = sheet_rels.by_id(rid) {
                    sheet
                        .drawings
                        .push(resolve_target(&entry.part, &rel.target));
                }
            }
            for rel in sheet_rels
                .of_kind("comments")
                .chain(sheet_rels.of_kind("threadedComment"))
            {
                sheet
                    .comments
                    .push(resolve_target(&entry.part, &rel.target));
            }
            sheet.rows = read.rows;
            sheet.cols = read.cols;
            sheet.merged = read.merged;
            sheet.auto_filter = read.auto_filter;
            sheet.keys = read.keys;
            sheet.sheet_data = read.sheet_data;
            sheets.push(sheet);
        }

        let connections = read::connections(&package, &main_rels, &main_part)?;
        let unsupported = read::unsupported(&package)?;
        Ok(Self {
            package,
            main_part,
            main_rels,
            macro_enabled,
            sheets,
            names: wb_part.names,
            strings,
            styles,
            tables,
            query_areas,
            connections,
            unsupported,
            date1904: wb_part.date1904,
            settings: wb_part.settings,
            calc: wb_part.calc,
        })
    }

    /// The text of a sheet's part, which the cells' spans point into. Read
    /// it once per sheet: it is checked as UTF-8 on every call.
    #[must_use]
    pub fn sheet_text(&self, sheet: usize) -> &str {
        self.sheets
            .get(sheet)
            .and_then(|s| self.package.part(&s.part))
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .unwrap_or("")
    }

    /// The text of a shared string.
    #[must_use]
    pub fn shared(&self, index: u32) -> Option<&SharedString> {
        self.strings.get(index as usize)
    }

    /// A sheet's position by name (case-insensitive, as Excel matches).
    #[must_use]
    pub fn sheet_index(&self, name: &str) -> Option<usize> {
        self.sheets
            .iter()
            .position(|s| s.name.eq_ignore_ascii_case(name))
    }

    /// The extension a file of this workbook has.
    #[must_use]
    pub fn extension(&self) -> &'static str {
        if self.macro_enabled { ".xlsm" } else { ".xlsx" }
    }

    /// How many cells hold something, across every sheet.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.sheets.iter().map(Sheet::cell_count).sum()
    }
}

#[cfg(test)]
mod tests;
