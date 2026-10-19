// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Writing an apply plan's steps into workbook bytes (decision 0023): used
//! for closed files, the test harness's pilot runs and fidelity tests. In
//! the task pane, Excel itself carries the plan out.
//!
//! Splice only: a set cell replaces its `<c>` element, a new cell or row is
//! inserted where it belongs, and an inserted row renumbers the rows below
//! it and moves every reference to them, as Excel does: formulas on every
//! sheet, defined names, tables, merged cells, filters, conditional
//! formats and data validation. Nothing else is re-serialised. Shared
//! formulas a change reaches are written out cell by cell. The calculation
//! chain is dropped (Excel rebuilds it) and a full recalculation is asked
//! for on open, because cached results are left as they were.
//!
//! Refused rather than risked: deleting rows, and inserting rows on a sheet
//! a chart, a pivot table or a what-if data table reads.

use std::collections::{BTreeMap, HashMap};

use quick_xml::events::Event;
use quick_xml::reader::Reader;

use super::Workbook;
use super::apply::{Step, Write, shift_for_inserts};
use super::formula::tokenize;
use super::model::{Content, FormulaKind, Span};
use super::refs::{a1, area_name, is_number, parse_area};
use crate::error::{Error, Result};
use crate::package::{Package, Relationships, edit, rels_part_for, resolve_target};
use crate::xml::{self, Edits, escape_attr, escape_text};

/// Applies the steps to a workbook's bytes, in order.
///
/// # Errors
/// [`Error::CannotEdit`] for a step this writer refuses, or when the result
/// doesn't reopen.
pub fn apply_steps(bytes: &[u8], steps: &[Step]) -> Result<Vec<u8>> {
    let mut out = bytes.to_vec();
    let mut sets: Vec<(&str, u32, u32, &Write)> = Vec::new();
    let flush = |out: &mut Vec<u8>, sets: &mut Vec<(&str, u32, u32, &Write)>| -> Result<()> {
        if !sets.is_empty() {
            *out = set_cells(out, sets)?;
            sets.clear();
        }
        Ok(())
    };
    for step in steps {
        match step {
            Step::Set {
                sheet,
                col,
                row,
                value,
            } => sets.push((sheet, *col, *row, value)),
            Step::InsertRows { sheet, at, count } => {
                flush(&mut out, &mut sets)?;
                out = insert_rows(&out, sheet, *at, *count)?;
            }
            Step::DeleteRows { .. } => {
                return Err(Error::CannotEdit(
                    "deleting rows isn't written by Lineage yet".to_owned(),
                ));
            }
        }
    }
    flush(&mut out, &mut sets)?;
    Workbook::open(&out)
        .map_err(|e| Error::CannotEdit(format!("the result doesn't reopen: {e}")))?;
    Ok(out)
}

/// The namespace prefix the worksheet writes its elements with ("" or "x:").
fn prefix_of(text: &str, sheet_data: Span) -> String {
    let tag = sheet_data.get(text);
    let name = tag.trim_start_matches('<');
    name.find("sheetData")
        .map(|i| name[..i].to_owned())
        .unwrap_or_default()
}

fn cell_xml(prefix: &str, col: u32, row: u32, style: u32, w: &Write) -> Result<String> {
    let r = a1(col, row);
    let s = if style == 0 {
        String::new()
    } else {
        format!(r#" s="{style}""#)
    };
    let p = prefix;
    Ok(match w {
        Write::Clear => {
            if style == 0 {
                String::new()
            } else {
                format!(r#"<{p}c r="{r}"{s}/>"#)
            }
        }
        Write::Number(n) => {
            if !is_number(n) {
                return Err(Error::CannotEdit("a number that isn't one".to_owned()));
            }
            format!(r#"<{p}c r="{r}"{s}><{p}v>{n}</{p}v></{p}c>"#)
        }
        Write::Text(t) => format!(
            r#"<{p}c r="{r}"{s} t="inlineStr"><{p}is><{p}t xml:space="preserve">{}</{p}t></{p}is></{p}c>"#,
            escape_text(t)
        ),
        Write::Bool(b) => format!(
            r#"<{p}c r="{r}"{s} t="b"><{p}v>{}</{p}v></{p}c>"#,
            u8::from(*b)
        ),
        Write::Error(e) => format!(
            r#"<{p}c r="{r}"{s} t="e"><{p}v>{}</{p}v></{p}c>"#,
            escape_text(e)
        ),
        Write::Formula(f) => format!(r#"<{p}c r="{r}"{s}><{p}f>{}</{p}f></{p}c>"#, escape_text(f)),
    })
}

/// The `<f>` element for a formula written out in full, keeping what
/// matters of the old one (array ranges, always-recalculate).
fn f_xml(
    prefix: &str,
    text: &str,
    kind: &FormulaKind,
    always: bool,
    array_ref: Option<String>,
) -> String {
    let p = prefix;
    let ca = if always { r#" ca="1""# } else { "" };
    match kind {
        FormulaKind::Array { .. } => format!(
            r#"<{p}f t="array" ref="{}"{ca}>{}</{p}f>"#,
            array_ref.unwrap_or_default(),
            escape_text(text)
        ),
        _ => format!(r#"<{p}f{ca}>{}</{p}f>"#, escape_text(text)),
    }
}

/// Every shared formula group of a sheet: si → (master row, column, text).
fn masters(wb: &Workbook, sheet: usize) -> HashMap<u32, (u32, u32, String)> {
    let text = wb.sheet_text(sheet);
    let mut out = HashMap::new();
    for row in &wb.sheets[sheet].rows {
        for cell in &row.cells {
            if let Content::Formula(f) = &cell.content
                && let FormulaKind::Shared { si, .. } = f.kind
                && let Some(t) = &f.text
            {
                out.entry(si)
                    .or_insert_with(|| (row.r, cell.col, t.get(text).to_owned()));
            }
        }
    }
    out
}

/// A cell's formula as it reads in its own cell.
fn own_formula(
    masters: &HashMap<u32, (u32, u32, String)>,
    text: &str,
    f: &super::model::Formula,
    row: u32,
    col: u32,
) -> Option<String> {
    match (&f.text, &f.kind) {
        (Some(t), FormulaKind::Shared { si, .. }) => match masters.get(si) {
            Some((r0, c0, _)) if (*r0, *c0) == (row, col) => Some(t.get(text).to_owned()),
            _ => Some(t.get(text).to_owned()),
        },
        (Some(t), _) => Some(t.get(text).to_owned()),
        (None, FormulaKind::Shared { si, .. }) => masters.get(si).map(|(r0, c0, m)| {
            tokenize(m)
                .shifted(
                    i64::from(row) - i64::from(*r0),
                    i64::from(col) - i64::from(*c0),
                )
                .render()
        }),
        (None, _) => None,
    }
}

fn blocked(wb: &Workbook, sheet_name: &str) -> Option<&'static str> {
    let quoted = format!("'{}'!", sheet_name.replace('\'', "''"));
    let plain = format!("{sheet_name}!");
    for name in wb.package.part_names() {
        let lower = name.to_ascii_lowercase();
        let Some(bytes) = wb.package.part(name) else {
            continue;
        };
        let text = String::from_utf8_lossy(bytes);
        if lower.starts_with("xl/charts/chart") && (text.contains(&quoted) || text.contains(&plain))
        {
            return Some("a chart reads this sheet");
        }
        if lower.starts_with("xl/pivotcache/")
            && text.contains(&format!(r#"sheet="{}""#, escape_attr(sheet_name)))
        {
            return Some("a pivot table reads this sheet");
        }
    }
    None
}

/// The value range of attribute `name` in a start tag, relative to the tag.
fn attr_range(tag: &str, name: &str) -> Option<std::ops::Range<usize>> {
    // Preceded by white space, so `r` never matches the end of `xr:r`.
    for ws in [' ', '\n', '\t', '\r'] {
        let needle = format!("{ws}{name}=\"");
        if let Some(i) = tag.find(&needle) {
            let start = i + needle.len();
            let end = start + tag[start..].find('"')?;
            return Some(start..end);
        }
    }
    None
}

/// The end of a start tag beginning at `start` (just past `>`).
fn tag_end(text: &str, start: usize) -> usize {
    text[start..]
        .find('>')
        .map_or(text.len(), |i| start + i + 1)
}

/// Moves references in areas ("A5:B9 C3") as Excel does when rows are
/// inserted at `at`.
fn shift_sqref(value: &str, at: u32, count: u32) -> String {
    value
        .split_whitespace()
        .map(|part| match parse_area(part) {
            Some(mut a) if a.row1 >= 1 && !(a.row1 == 1 && a.row2 == super::refs::MAX_ROW) => {
                if a.row1 >= at {
                    a.row1 += count;
                }
                if a.row2 >= at {
                    a.row2 += count;
                }
                area_name(&a)
            }
            _ => part.to_owned(),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Rewrites attributes and formula texts in a region of a part, with a
/// non-namespace reader (the region need not be well formed at its ends).
fn rewrite_region(
    text: &str,
    region: std::ops::Range<usize>,
    edits: &mut Edits,
    attr: &dyn Fn(&str, &str, &str) -> Option<String>,
    formula: &dyn Fn(&str) -> Option<String>,
) -> Result<()> {
    let slice = &text[region.clone()];
    let mut reader = Reader::from_str(slice);
    let config = reader.config_mut();
    config.check_end_names = false;
    config.trim_text(false);
    let mut in_formula: Option<(usize, usize)> = None;
    loop {
        let at = usize::try_from(reader.buffer_position()).unwrap_or(0);
        let event = match reader.read_event() {
            Ok(e) => e,
            Err(_) => break,
        };
        let after = usize::try_from(reader.buffer_position()).unwrap_or(0);
        match event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let local = e.local_name();
                let local: &str = local.as_ref();
                let tag = &slice[at..after];
                for a in e.attributes().flatten() {
                    let key = a.key.0;
                    let Ok(value) = a.normalized_value(quick_xml::XmlVersion::Implicit1_0) else {
                        continue;
                    };
                    if let Some(new) = attr(local, key, &value)
                        && new != value
                        && let Some(range) = attr_range(tag, key)
                    {
                        let base = region.start + at;
                        edits.replace(
                            base + range.start..base + range.end,
                            escape_attr(&new).into_owned(),
                        );
                    }
                }
                if matches!(event, Event::Start(_))
                    && matches!(
                        local,
                        "formula" | "formula1" | "formula2" | "calculatedColumnFormula"
                    )
                {
                    in_formula = Some((after, after));
                }
            }
            Event::End(_) => {
                if let Some((start, _)) = in_formula.take() {
                    let raw = &slice[start..at];
                    let plain = raw
                        .replace("&lt;", "<")
                        .replace("&gt;", ">")
                        .replace("&quot;", "\"")
                        .replace("&apos;", "'")
                        .replace("&amp;", "&");
                    if let Some(new) = formula(&plain)
                        && new != plain
                    {
                        edits.replace(
                            region.start + start..region.start + at,
                            escape_text(&new).into_owned(),
                        );
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(())
}

/// Inserts `count` rows at row `at` of `sheet_name`.
fn insert_rows(bytes: &[u8], sheet_name: &str, at: u32, count: u32) -> Result<Vec<u8>> {
    let wb = Workbook::open(bytes)?;
    let target = wb
        .sheet_index(sheet_name)
        .ok_or_else(|| Error::CannotEdit(format!("no sheet {sheet_name}")))?;
    if let Some(why) = blocked(&wb, &wb.sheets[target].name) {
        return Err(Error::CannotEdit(format!(
            "rows can't be inserted safely: {why}"
        )));
    }
    let target_name = wb.sheets[target].name.clone();
    let mut inserts = BTreeMap::new();
    inserts.insert(target_name.clone(), vec![(at, count)]);
    let mut package = wb.package.clone();

    for (i, sheet) in wb.sheets.iter().enumerate() {
        if sheet.kind != super::model::SheetKind::Worksheet {
            continue;
        }
        let text = wb.sheet_text(i);
        let prefix = prefix_of(text, sheet.sheet_data);
        let mut edits = Edits::new();
        let groups = masters(&wb, i);
        // Shared groups to write out: on the target sheet (their cells move)
        // or reading it.
        let refers = |t: &str| {
            tokenize(t).references().any(|r| {
                r.sheet
                    .as_ref()
                    .is_some_and(|p| p.first.eq_ignore_ascii_case(&target_name))
            })
        };
        let expand: Vec<u32> = groups
            .iter()
            .filter(|(_, (_, _, t))| i == target || refers(t))
            .map(|(si, _)| *si)
            .collect();
        for row in &sheet.rows {
            if i == target && row.r >= at {
                let new_r = row.r + count;
                let tag_end = tag_end(text, row.span.start as usize);
                let tag = &text[row.span.start as usize..tag_end];
                match attr_range(tag, "r") {
                    Some(range) => edits.replace(
                        row.span.start as usize + range.start..row.span.start as usize + range.end,
                        new_r.to_string(),
                    ),
                    None => edits.insert(
                        row.span.start as usize + 1 + prefix.len() + 3,
                        format!(r#" r="{new_r}""#),
                    ),
                }
            }
            for cell in &row.cells {
                if i == target && row.r >= at {
                    let start = cell.element.start as usize;
                    let tag = &text[start..tag_end(text, start)];
                    let new_at = a1(cell.col, row.r + count);
                    match attr_range(tag, "r") {
                        Some(range) => {
                            edits.replace(start + range.start..start + range.end, new_at)
                        }
                        None => {
                            edits.insert(start + 1 + prefix.len() + 1, format!(r#" r="{new_at}""#))
                        }
                    }
                }
                let Content::Formula(f) = &cell.content else {
                    continue;
                };
                if matches!(f.kind, FormulaKind::DataTable) {
                    return Err(Error::CannotEdit(
                        "a what-if data table is on this sheet".to_owned(),
                    ));
                }
                let Some(own) = own_formula(&groups, text, f, row.r, cell.col) else {
                    continue;
                };
                let shared =
                    matches!(f.kind, FormulaKind::Shared { si, .. } if expand.contains(&si));
                let moved = shift_for_inserts(&tokenize(&own), &sheet.name, &inserts);
                if moved == own && !shared {
                    continue;
                }
                let array_ref = match &f.kind {
                    FormulaKind::Array { range: Some(a) } => Some(shift_sqref(
                        &area_name(a),
                        if i == target { at } else { u32::MAX },
                        count,
                    )),
                    _ => None,
                };
                let kind = if shared {
                    FormulaKind::Normal
                } else {
                    f.kind.clone()
                };
                edits.replace(
                    f.element.start as usize..(f.element.start + f.element.len) as usize,
                    f_xml(&prefix, &moved, &kind, f.always, array_ref),
                );
            }
        }
        // Outside the cells.
        let sd = sheet.sheet_data;
        let attr = |elem: &str, name: &str, value: &str| -> Option<String> {
            if i != target {
                return None;
            }
            match (elem, name) {
                (
                    "dimension" | "mergeCell" | "autoFilter" | "hyperlink" | "protectedRange"
                    | "ignoredError",
                    "ref" | "sqref",
                )
                | ("conditionalFormatting" | "dataValidation", "sqref") => {
                    Some(shift_sqref(value, at, count))
                }
                _ => None,
            }
        };
        let formula = |f: &str| -> Option<String> {
            Some(shift_for_inserts(&tokenize(f), &sheet.name, &inserts))
        };
        rewrite_region(text, 0..sd.start as usize, &mut edits, &attr, &formula)?;
        rewrite_region(
            text,
            (sd.start + sd.len) as usize..text.len(),
            &mut edits,
            &attr,
            &formula,
        )?;
        if !edits.is_empty() {
            package.set_part(&sheet.part, edits.apply(text)?.into_bytes());
        }
    }

    // Tables on the sheet.
    for t in wb.tables.iter().filter(|t| t.sheet == target) {
        let Some(bytes) = package.part(&t.part).map(<[u8]>::to_vec) else {
            continue;
        };
        let text = xml::text(&t.part, &bytes)?.to_owned();
        let mut edits = Edits::new();
        let attr = |elem: &str, name: &str, value: &str| -> Option<String> {
            matches!((elem, name), ("table" | "autoFilter" | "sortState", "ref"))
                .then(|| shift_sqref(value, at, count))
        };
        let formula = |f: &str| -> Option<String> {
            Some(shift_for_inserts(&tokenize(f), &target_name, &inserts))
        };
        rewrite_region(&text, 0..text.len(), &mut edits, &attr, &formula)?;
        if !edits.is_empty() {
            package.set_part(&t.part, edits.apply(&text)?.into_bytes());
        }
    }

    // Defined names, and a full recalculation on open.
    write_workbook_part(&mut package, &wb, |name_text, local| {
        let own = local
            .and_then(|l| wb.sheets.get(l))
            .map_or(target_name.as_str(), |s| s.name.as_str());
        shift_for_inserts(&tokenize(name_text), own, &inserts)
    })?;
    drop_calc_chain(&mut package, &wb)?;
    package.to_bytes()
}

/// Rewrites defined names through `name`, and asks Excel to recalculate
/// everything on open.
fn write_workbook_part(
    package: &mut Package,
    wb: &Workbook,
    name: impl Fn(&str, Option<usize>) -> String,
) -> Result<()> {
    let Some(bytes) = package.part(&wb.main_part).map(<[u8]>::to_vec) else {
        return Ok(());
    };
    let text = xml::text(&wb.main_part, &bytes)?;
    let doc = xml::parse(&wb.main_part, text)?;
    let mut edits = Edits::new();
    let root = doc.root_element();
    let main = super::stream::MAIN;
    if let Some(list) = xml::child(root, main, "definedNames") {
        for n in list.children().filter(|n| xml::is(*n, main, "definedName")) {
            let Some(child) = n.first_child().filter(roxmltree::Node::is_text) else {
                continue;
            };
            let old = child.text().unwrap_or("");
            let local = n.attribute("localSheetId").and_then(|v| v.parse().ok());
            let new = name(old, local);
            if new != old {
                edits.replace(child.range(), escape_text(&new).into_owned());
            }
        }
    }
    match xml::child(root, main, "calcPr") {
        Some(calc) => {
            if let Some(a) = calc.attributes().find(|a| a.name() == "fullCalcOnLoad") {
                if !matches!(a.value(), "1" | "true") {
                    edits.replace(a.range_value(), "1");
                }
            } else {
                let end = xml::start_tag_end(text, calc);
                let close = if text[..end].ends_with("/>") {
                    end - 2
                } else {
                    end - 1
                };
                edits.insert(close, r#" fullCalcOnLoad="1""#);
            }
        }
        None => {
            // After definedNames, or after sheets.
            let after =
                xml::child(root, main, "definedNames").or_else(|| xml::child(root, main, "sheets"));
            if let Some(node) = after {
                let p = xml::prefix_of(text, root);
                let p = if p.is_empty() {
                    String::new()
                } else {
                    format!("{p}:")
                };
                edits.insert(
                    node.range().end,
                    format!(r#"<{p}calcPr fullCalcOnLoad="1"/>"#),
                );
            }
        }
    }
    if !edits.is_empty() {
        package.set_part(&wb.main_part, edits.apply(text)?.into_bytes());
    }
    Ok(())
}

/// Removes the calculation chain: stale after any change, and Excel
/// rebuilds it.
fn drop_calc_chain(package: &mut Package, wb: &Workbook) -> Result<()> {
    let rels_part = rels_part_for(&wb.main_part);
    let Some(bytes) = package.part(&rels_part).map(<[u8]>::to_vec) else {
        return Ok(());
    };
    let rels = Relationships::parse(&rels_part, &bytes)?;
    let ids: Vec<String> = rels.of_kind("calcChain").map(|r| r.id.clone()).collect();
    for rel in rels.of_kind("calcChain") {
        let part = resolve_target(&wb.main_part, &rel.target);
        package.remove_part(&part);
        edit::remove_override(package, &part)?;
    }
    if !ids.is_empty() {
        edit::remove_relationships(package, &wb.main_part, &ids)?;
    }
    Ok(())
}

/// Sets cells, keeping each cell's format.
fn set_cells(bytes: &[u8], sets: &[(&str, u32, u32, &Write)]) -> Result<Vec<u8>> {
    let wb = Workbook::open(bytes)?;
    let mut package = wb.package.clone();
    let mut by_sheet: BTreeMap<usize, Vec<(u32, u32, &Write)>> = BTreeMap::new();
    for (sheet, col, row, w) in sets {
        let i = wb
            .sheet_index(sheet)
            .ok_or_else(|| Error::CannotEdit(format!("no sheet {sheet}")))?;
        by_sheet.entry(i).or_default().push((*row, *col, *w));
    }
    for (i, mut list) in by_sheet {
        list.sort_by_key(|(r, c, _)| (*r, *c));
        list.dedup_by_key(|(r, c, _)| (*r, *c));
        let sheet = &wb.sheets[i];
        let text = wb.sheet_text(i);
        let prefix = prefix_of(text, sheet.sheet_data);
        let groups = masters(&wb, i);
        let mut edits = Edits::new();
        // Shared groups a set cell belongs to are written out in full.
        let targets: std::collections::HashSet<(u32, u32)> =
            list.iter().map(|(r, c, _)| (*r, *c)).collect();
        let mut touched: Vec<u32> = Vec::new();
        for row in &sheet.rows {
            for cell in &row.cells {
                if let Content::Formula(f) = &cell.content
                    && let FormulaKind::Shared { si, .. } = f.kind
                    && targets.contains(&(row.r, cell.col))
                    && !touched.contains(&si)
                {
                    touched.push(si);
                }
            }
        }
        for row in &sheet.rows {
            for cell in &row.cells {
                if targets.contains(&(row.r, cell.col)) {
                    continue;
                }
                if let Content::Formula(f) = &cell.content
                    && let FormulaKind::Shared { si, .. } = f.kind
                    && touched.contains(&si)
                    && let Some(own) = own_formula(&groups, text, f, row.r, cell.col)
                {
                    edits.replace(
                        f.element.start as usize..(f.element.start + f.element.len) as usize,
                        f_xml(&prefix, &own, &FormulaKind::Normal, f.always, None),
                    );
                }
            }
        }
        // New rows go before the first row below them.
        let mut new_rows: BTreeMap<u32, Vec<String>> = BTreeMap::new();
        for (r, c, w) in &list {
            match sheet.row(*r) {
                Some(row) => {
                    match row.cells.binary_search_by_key(c, |x| x.col) {
                        Ok(k) => {
                            let cell = &row.cells[k];
                            let xml = cell_xml(&prefix, *c, *r, cell.style, w)?;
                            edits.replace(
                                cell.element.start as usize
                                    ..(cell.element.start + cell.element.len) as usize,
                                xml,
                            );
                        }
                        Err(k) => {
                            let xml = cell_xml(&prefix, *c, *r, 0, w)?;
                            if xml.is_empty() {
                                continue;
                            }
                            let span_start = row.span.start as usize;
                            let span_end = (row.span.start + row.span.len) as usize;
                            if let Some(next) = row.cells.get(k) {
                                edits.insert(next.element.start as usize, xml);
                            } else if text[..span_end].ends_with("/>") {
                                // `<row .../>`: open it up.
                                let open = &text[span_start..span_end - 2];
                                edits.replace(
                                    span_start..span_end,
                                    format!("{open}>{xml}</{prefix}row>"),
                                );
                            } else {
                                let close = text[..span_end].rfind("</").unwrap_or(span_end);
                                edits.insert(close, xml);
                            }
                        }
                    }
                }
                None => {
                    let xml = cell_xml(&prefix, *c, *r, 0, w)?;
                    if !xml.is_empty() {
                        new_rows.entry(*r).or_default().push(xml);
                    }
                }
            }
        }
        let sd = sheet.sheet_data;
        let sd_end = (sd.start + sd.len) as usize;
        for (r, cells) in new_rows {
            let xml = format!(r#"<{prefix}row r="{r}">{}</{prefix}row>"#, cells.concat());
            match sheet.rows.iter().find(|x| x.r > r) {
                Some(next) => edits.insert(next.span.start as usize, xml),
                None if text[..sd_end].ends_with("/>") => {
                    edits.replace(
                        sd.start as usize..sd_end,
                        format!("<{prefix}sheetData>{xml}</{prefix}sheetData>"),
                    );
                }
                None => {
                    let close = text[..sd_end].rfind("</").unwrap_or(sd_end);
                    edits.insert(close, xml);
                }
            }
        }
        if !edits.is_empty() {
            package.set_part(&sheet.part, edits.apply(text)?.into_bytes());
        }
    }
    write_workbook_part(&mut package, &wb, |t, _| t.to_owned())?;
    drop_calc_chain(&mut package, &wb)?;
    package.to_bytes()
}

#[cfg(test)]
mod tests;
