// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Pivot tables, their caches and slicers for synthetic workbooks
//! (ECMA-376 Part 1 §18.10, and the x14 slicer extension).
//!
//! The builder renders each pivot's output cells into its sheet from the
//! cache records, with exact decimal sums, so the cells agree with the
//! cache the way Excel's do. Only the shapes the fixtures need: one row
//! field, any number of values, at most one report filter, compact or
//! tabular form.

use std::fmt::Write;

use crate::workbooks::Dec;
use crate::xlsx::{Sheet, Style, Value, a1, num, parse_a1, text};

const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";
const NS_MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub const NS_X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";
const NS_MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// A field of a pivot cache.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheField {
    pub name: String,
    /// Numbers (summed); otherwise text items a pivot can put on rows.
    pub numeric: bool,
    /// A calculated field's formula. Calculated fields have no records.
    pub formula: Option<String>,
}

impl CacheField {
    #[must_use]
    pub fn text(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            numeric: false,
            formula: None,
        }
    }

    #[must_use]
    pub fn number(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            numeric: true,
            formula: None,
        }
    }

    #[must_use]
    pub fn calculated(name: &str, formula: &str) -> Self {
        Self {
            name: name.to_owned(),
            numeric: true,
            formula: Some(formula.to_owned()),
        }
    }
}

/// A pivot cache: a copy of a source range's data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PivotCache {
    pub sheet: String,
    pub range: String,
    pub fields: Vec<CacheField>,
    /// One row per record, one value per field that isn't calculated.
    pub records: Vec<Vec<String>>,
    /// `refreshedDate`, a serial date.
    pub refreshed: String,
}

impl PivotCache {
    fn database(&self) -> Vec<usize> {
        (0..self.fields.len())
            .filter(|&k| self.fields[k].formula.is_none())
            .collect()
    }

    /// The column of `field` in a record.
    fn column(&self, field: usize) -> Option<usize> {
        self.database().iter().position(|&k| k == field)
    }

    /// A text field's items, in order of first appearance.
    #[must_use]
    pub fn items(&self, field: usize) -> Vec<String> {
        let Some(col) = self.column(field) else {
            return Vec::new();
        };
        let mut out: Vec<String> = Vec::new();
        for r in &self.records {
            if !out.contains(&r[col]) {
                out.push(r[col].clone());
            }
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueSpec {
    /// "Sum of Revenue".
    pub name: String,
    pub field: usize,
    /// "sum" or "count".
    pub function: String,
}

impl ValueSpec {
    #[must_use]
    pub fn sum(name: &str, field: usize) -> Self {
        Self {
            name: name.to_owned(),
            field,
            function: "sum".to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Form {
    #[default]
    Compact,
    Tabular,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pivot {
    pub name: String,
    /// The sheet it sits on (position in the workbook).
    pub sheet: usize,
    pub cache: usize,
    /// Its top-left cell (the header row), below any report filter.
    pub at: String,
    pub rows: usize,
    pub values: Vec<ValueSpec>,
    /// A report filter: the field, and the one item shown (`None` for all).
    pub filter: Option<(usize, Option<String>)>,
    /// Items hidden in a field.
    pub hidden: Vec<(usize, Vec<String>)>,
    pub style: String,
    pub form: Form,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slicer {
    /// The slicer cache's name, "Slicer_Region".
    pub name: String,
    pub caption: String,
    pub field: usize,
    /// The pivots it filters (positions in `Workbook::pivots`), all on the
    /// same cache.
    pub pivots: Vec<usize>,
    /// The sheet the slicer's shape sits on.
    pub sheet: usize,
    /// The items it shows; `None` shows all.
    pub selected: Option<Vec<String>>,
}

/// The items hidden in `field` of `pivot`: its own, and those its slicers
/// leave out.
fn hidden_items(
    pivot: &Pivot,
    index: usize,
    field: usize,
    caches: &[PivotCache],
    slicers: &[Slicer],
) -> Vec<String> {
    let cache = &caches[pivot.cache];
    let mut out: Vec<String> = pivot
        .hidden
        .iter()
        .filter(|(f, _)| *f == field)
        .flat_map(|(_, items)| items.clone())
        .collect();
    for s in slicers {
        if s.field == field
            && s.pivots.contains(&index)
            && let Some(selected) = &s.selected
        {
            for item in cache.items(field) {
                if !selected.contains(&item) && !out.contains(&item) {
                    out.push(item);
                }
            }
        }
    }
    out
}

/// What a pivot shows: (row item, one total per value), then the grand
/// totals.
fn compute(
    pivot: &Pivot,
    index: usize,
    caches: &[PivotCache],
    slicers: &[Slicer],
) -> (Vec<(String, Vec<Dec>)>, Vec<Dec>) {
    let cache = &caches[pivot.cache];
    let keep = |r: &Vec<String>| -> bool {
        if let Some((f, Some(item))) = &pivot.filter
            && cache.column(*f).is_some_and(|c| &r[c] != item)
        {
            return false;
        }
        for f in 0..cache.fields.len() {
            let hidden = hidden_items(pivot, index, f, caches, slicers);
            if let Some(c) = cache.column(f)
                && hidden.contains(&r[c])
            {
                return false;
            }
        }
        true
    };
    let records: Vec<&Vec<String>> = cache.records.iter().filter(|r| keep(r)).collect();
    let total = |rs: &[&Vec<String>], v: &ValueSpec| -> Dec {
        if v.function == "count" {
            return Dec::int(i128::try_from(rs.len()).unwrap_or(0));
        }
        let Some(c) = cache.column(v.field) else {
            return Dec::int(0);
        };
        rs.iter()
            .fold(Dec::int(0), |acc, r| acc.plus(Dec::parse(&r[c])))
    };
    let row_col = cache.column(pivot.rows);
    let hidden = hidden_items(pivot, index, pivot.rows, caches, slicers);
    let mut rows = Vec::new();
    for item in cache.items(pivot.rows) {
        if hidden.contains(&item) {
            continue;
        }
        let rs: Vec<&Vec<String>> = records
            .iter()
            .copied()
            .filter(|r| row_col.is_some_and(|c| r[c] == item))
            .collect();
        if rs.is_empty() {
            continue;
        }
        rows.push((item, pivot.values.iter().map(|v| total(&rs, v)).collect()));
    }
    let grand = pivot.values.iter().map(|v| total(&records, v)).collect();
    (rows, grand)
}

/// The pivot's area: its header row to its grand total row.
#[must_use]
pub fn location(pivot: &Pivot, index: usize, caches: &[PivotCache], slicers: &[Slicer]) -> String {
    let (rows, _) = compute(pivot, index, caches, slicers);
    let (col, row) = parse_a1(&pivot.at);
    let width = u32::try_from(pivot.values.len()).unwrap_or(0);
    let height = u32::try_from(rows.len()).unwrap_or(0) + 1;
    format!("{}:{}", a1(col, row), a1(col + width, row + height))
}

/// Writes every pivot's output cells into its sheet.
pub fn render(sheets: &mut [Sheet], pivots: &[Pivot], caches: &[PivotCache], slicers: &[Slicer]) {
    for (index, p) in pivots.iter().enumerate() {
        let cache = &caches[p.cache];
        let (rows, grand) = compute(p, index, caches, slicers);
        let (col, row) = parse_a1(&p.at);
        let sheet = &mut sheets[p.sheet];
        let put = |sheet: &mut Sheet, c: u32, r: u32, v: Value, s: Style| {
            sheet.set(&a1(c, r), v, s);
        };
        if let Some((f, item)) = &p.filter {
            put(
                sheet,
                col,
                row - 2,
                text(&cache.fields[*f].name),
                Style::General,
            );
            put(
                sheet,
                col + 1,
                row - 2,
                text(item.as_deref().unwrap_or("(All)")),
                Style::General,
            );
        }
        let header = match p.form {
            Form::Compact => "Row Labels".to_owned(),
            Form::Tabular => cache.fields[p.rows].name.clone(),
        };
        put(sheet, col, row, text(&header), Style::Bold);
        for (k, v) in p.values.iter().enumerate() {
            let c = col + 1 + u32::try_from(k).unwrap_or(0);
            put(sheet, c, row, text(&v.name), Style::Bold);
        }
        let mut r = row + 1;
        for (item, totals) in &rows {
            put(sheet, col, r, text(item), Style::General);
            for (k, t) in totals.iter().enumerate() {
                let c = col + 1 + u32::try_from(k).unwrap_or(0);
                put(sheet, c, r, num(t.lexeme()), Style::General);
            }
            r += 1;
        }
        put(sheet, col, r, text("Grand Total"), Style::Bold);
        for (k, t) in grand.iter().enumerate() {
            let c = col + 1 + u32::try_from(k).unwrap_or(0);
            put(sheet, c, r, num(t.lexeme()), Style::Bold);
        }
    }
}

/// `pivotCacheDefinitionN.xml`, with `pivot_cache_id` for its slicers.
#[must_use]
pub fn cache_xml(cache: &PivotCache, pivot_cache_id: usize) -> String {
    let mut out = format!(
        r#"{DECL}<pivotCacheDefinition xmlns="{NS_MAIN}" xmlns:r="{NS_R}" r:id="rId1" refreshedBy="Lineage fixture generator" refreshedDate="{}" createdVersion="8" refreshedVersion="8" minRefreshableVersion="3" recordCount="{}"><cacheSource type="worksheet"><worksheetSource ref="{}" sheet="{}"/></cacheSource><cacheFields count="{}">"#,
        cache.refreshed,
        cache.records.len(),
        cache.range,
        esc(&cache.sheet),
        cache.fields.len()
    );
    for (k, f) in cache.fields.iter().enumerate() {
        if let Some(formula) = &f.formula {
            let _ = write!(
                out,
                r#"<cacheField name="{}" numFmtId="0" formula="{}" databaseField="0"/>"#,
                esc(&f.name),
                esc(formula)
            );
        } else if f.numeric {
            let _ = write!(
                out,
                r#"<cacheField name="{}" numFmtId="0"><sharedItems containsSemiMixedTypes="0" containsString="0" containsNumber="1"/></cacheField>"#,
                esc(&f.name)
            );
        } else {
            let items = cache.items(k);
            let _ = write!(
                out,
                r#"<cacheField name="{}" numFmtId="0"><sharedItems count="{}">"#,
                esc(&f.name),
                items.len()
            );
            for i in items {
                let _ = write!(out, r#"<s v="{}"/>"#, esc(&i));
            }
            out.push_str("</sharedItems></cacheField>");
        }
    }
    let _ = write!(
        out,
        r#"</cacheFields><extLst><ext uri="{{725AE2AE-9491-48be-B2B4-4EB974FC3084}}" xmlns:x14="{NS_X14}"><x14:pivotCacheDefinition pivotCacheId="{pivot_cache_id}"/></ext></extLst></pivotCacheDefinition>"#
    );
    out
}

/// `pivotCacheRecordsN.xml`.
#[must_use]
pub fn records_xml(cache: &PivotCache) -> String {
    let mut out = format!(
        r#"{DECL}<pivotCacheRecords xmlns="{NS_MAIN}" xmlns:r="{NS_R}" count="{}">"#,
        cache.records.len()
    );
    let db = cache.database();
    let items: Vec<Vec<String>> = db.iter().map(|&k| cache.items(k)).collect();
    for r in &cache.records {
        out.push_str("<r>");
        for (c, &k) in db.iter().enumerate() {
            if cache.fields[k].numeric {
                let _ = write!(out, r#"<n v="{}"/>"#, esc(&r[c]));
            } else {
                let x = items[c].iter().position(|i| *i == r[c]).unwrap_or(0);
                let _ = write!(out, r#"<x v="{x}"/>"#);
            }
        }
        out.push_str("</r>");
    }
    out.push_str("</pivotCacheRecords>");
    out
}

/// `pivotTableN.xml`.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn table_xml(
    pivot: &Pivot,
    index: usize,
    cache_id: usize,
    caches: &[PivotCache],
    slicers: &[Slicer],
) -> String {
    let cache = &caches[pivot.cache];
    let (rows, _) = compute(pivot, index, caches, slicers);
    let many = pivot.values.len() > 1;
    let form = match pivot.form {
        Form::Compact => r#" outline="1" outlineData="1""#,
        Form::Tabular => r#" compact="0" compactData="0" gridDropZones="1""#,
    };
    let page_rows = if pivot.filter.is_some() {
        r#" rowPageCount="1" colPageCount="1""#
    } else {
        ""
    };
    let mut out = format!(
        r#"{DECL}<pivotTableDefinition xmlns="{NS_MAIN}" name="{}" cacheId="{cache_id}" applyNumberFormats="0" applyBorderFormats="0" applyFontFormats="0" applyPatternFormats="0" applyAlignmentFormats="0" applyWidthHeightFormats="1" dataCaption="Values" updatedVersion="8" minRefreshableVersion="3" useAutoFormatting="1" itemPrintTitles="1" createdVersion="8" indent="0"{form} multipleFieldNames="0"><location ref="{}" firstHeaderRow="{}" firstDataRow="1" firstDataCol="1"{page_rows}/><pivotFields count="{}">"#,
        esc(&pivot.name),
        location(pivot, index, caches, slicers),
        if many { 0 } else { 1 },
        cache.fields.len()
    );
    let tabular = if pivot.form == Form::Tabular {
        r#" compact="0" outline="0""#
    } else {
        ""
    };
    for (k, f) in cache.fields.iter().enumerate() {
        let is_data = pivot.values.iter().any(|v| v.field == k);
        let data = if is_data { r#" dataField="1""# } else { "" };
        let axis = if k == pivot.rows {
            Some("axisRow")
        } else if pivot.filter.as_ref().is_some_and(|(p, _)| *p == k) {
            Some("axisPage")
        } else {
            None
        };
        if f.formula.is_some() {
            let _ = write!(
                out,
                r#"<pivotField{data} dragToRow="0" dragToCol="0" dragToPage="0" showAll="0" defaultSubtotal="0"/>"#
            );
            continue;
        }
        let hidden = hidden_items(pivot, index, k, caches, slicers);
        // A field a slicer filters lists its items even off the axes.
        let axis = axis.or_else(|| (!hidden.is_empty()).then_some(""));
        match axis {
            Some(axis) => {
                let items = cache.items(k);
                let axis = if axis.is_empty() {
                    String::new()
                } else {
                    format!(r#" axis="{axis}""#)
                };
                let _ = write!(
                    out,
                    r#"<pivotField{axis}{data}{tabular} showAll="0"><items count="{}">"#,
                    items.len() + 1
                );
                for (x, item) in items.iter().enumerate() {
                    let h = if hidden.contains(item) {
                        r#" h="1""#
                    } else {
                        ""
                    };
                    let _ = write!(out, r#"<item{h} x="{x}"/>"#);
                }
                out.push_str(r#"<item t="default"/></items></pivotField>"#);
            }
            None => {
                let _ = write!(out, r#"<pivotField{data}{tabular} showAll="0"/>"#);
            }
        }
    }
    let items = cache.items(pivot.rows);
    let _ = write!(
        out,
        r#"</pivotFields><rowFields count="1"><field x="{}"/></rowFields><rowItems count="{}">"#,
        pivot.rows,
        rows.len() + 1
    );
    for (item, _) in &rows {
        let x = items.iter().position(|i| i == item).unwrap_or(0);
        if x == 0 {
            out.push_str("<i><x/></i>");
        } else {
            let _ = write!(out, r#"<i><x v="{x}"/></i>"#);
        }
    }
    out.push_str(r#"<i t="grand"><x/></i></rowItems>"#);
    if many {
        out.push_str(r#"<colFields count="1"><field x="-2"/></colFields>"#);
        let _ = write!(out, r#"<colItems count="{}">"#, pivot.values.len());
        for k in 0..pivot.values.len() {
            if k == 0 {
                out.push_str("<i><x/></i>");
            } else {
                let _ = write!(out, r#"<i i="{k}"><x v="{k}"/></i>"#);
            }
        }
        out.push_str("</colItems>");
    } else {
        out.push_str(r#"<colItems count="1"><i/></colItems>"#);
    }
    if let Some((f, item)) = &pivot.filter {
        let item = item
            .as_ref()
            .and_then(|i| cache.items(*f).iter().position(|x| x == i))
            .map(|x| format!(r#" item="{x}""#))
            .unwrap_or_default();
        let _ = write!(
            out,
            r#"<pageFields count="1"><pageField fld="{f}"{item} hier="-1"/></pageFields>"#
        );
    }
    let _ = write!(out, r#"<dataFields count="{}">"#, pivot.values.len());
    for v in &pivot.values {
        let subtotal = if v.function == "sum" {
            String::new()
        } else {
            format!(r#" subtotal="{}""#, v.function)
        };
        let _ = write!(
            out,
            r#"<dataField name="{}" fld="{}"{subtotal} baseField="0" baseItem="0"/>"#,
            esc(&v.name),
            v.field
        );
    }
    let _ = write!(
        out,
        r#"</dataFields><pivotTableStyleInfo name="{}" showRowHeaders="1" showColHeaders="1" showRowStripes="0" showColStripes="0" showLastColumn="1"/></pivotTableDefinition>"#,
        esc(&pivot.style)
    );
    out
}

/// `slicerCacheN.xml`.
#[must_use]
pub fn slicer_cache_xml(
    slicer: &Slicer,
    pivots: &[Pivot],
    caches: &[PivotCache],
    tab_ids: &[u32],
) -> String {
    let first = &pivots[slicer.pivots[0]];
    let cache = &caches[first.cache];
    let field = &cache.fields[slicer.field];
    let mut out = format!(
        r#"{DECL}<slicerCacheDefinition xmlns="{NS_X14}" xmlns:mc="{NS_MC}" mc:Ignorable="x" xmlns:x="{NS_MAIN}" name="{}" sourceName="{}"><pivotTables>"#,
        esc(&slicer.name),
        esc(&field.name)
    );
    for &p in &slicer.pivots {
        let _ = write!(
            out,
            r#"<pivotTable tabId="{}" name="{}"/>"#,
            tab_ids[pivots[p].sheet],
            esc(&pivots[p].name)
        );
    }
    let items = cache.items(slicer.field);
    let _ = write!(
        out,
        r#"</pivotTables><data><tabular pivotCacheId="{}"><items count="{}">"#,
        first.cache + 1,
        items.len()
    );
    for (x, item) in items.iter().enumerate() {
        let shown = slicer.selected.as_ref().is_none_or(|s| s.contains(item));
        let s = if shown { r#" s="1""# } else { "" };
        let _ = write!(out, r#"<i x="{x}"{s}/>"#);
    }
    out.push_str("</items></tabular></data></slicerCacheDefinition>");
    out
}

/// `slicerN.xml`: the slicers on one sheet.
#[must_use]
pub fn slicers_xml(slicers: &[&Slicer]) -> String {
    let mut out = format!(
        r#"{DECL}<slicers xmlns="{NS_X14}" xmlns:mc="{NS_MC}" mc:Ignorable="x" xmlns:x="{NS_MAIN}">"#
    );
    for s in slicers {
        let _ = write!(
            out,
            r#"<slicer name="{}" cache="{}" caption="{}" rowHeight="241300"/>"#,
            esc(&s.caption),
            esc(&s.name),
            esc(&s.caption)
        );
    }
    out.push_str("</slicers>");
    out
}

/// The drawing that shows a sheet's slicers, as Excel writes one.
#[must_use]
pub fn slicer_drawing_xml(slicers: &[&Slicer]) -> String {
    let mut out = format!(
        r#"{DECL}<xdr:wsDr xmlns:xdr="http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">"#
    );
    for (n, s) in slicers.iter().enumerate() {
        let from_row = 2 + n * 12;
        let anchor = |inner: &str| {
            format!(
                r#"<xdr:twoCellAnchor editAs="oneCell"><xdr:from><xdr:col>6</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{from_row}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>9</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>{}</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>{inner}<xdr:clientData/></xdr:twoCellAnchor>"#,
                from_row + 10
            )
        };
        let choice = anchor(&format!(
            r#"<xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="{}" name="{}"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="http://schemas.microsoft.com/office/drawing/2010/slicer"><sle:slicer xmlns:sle="http://schemas.microsoft.com/office/drawing/2010/slicer" name="{}"/></a:graphicData></a:graphic></xdr:graphicFrame>"#,
            n + 2,
            esc(&s.caption),
            esc(&s.caption)
        ));
        let fallback = anchor(
            r#"<xdr:sp macro="" textlink=""><xdr:nvSpPr><xdr:cNvPr id="0" name=""/><xdr:cNvSpPr><a:spLocks noTextEdit="1"/></xdr:cNvSpPr></xdr:nvSpPr><xdr:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="1828800" cy="1905000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></xdr:spPr><xdr:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US" sz="1100"/><a:t>This shape represents a slicer.</a:t></a:r></a:p></xdr:txBody></xdr:sp>"#,
        );
        let _ = write!(
            out,
            r#"<mc:AlternateContent xmlns:mc="{NS_MC}"><mc:Choice xmlns:sle15="http://schemas.microsoft.com/office/drawing/2012/slicer" Requires="sle15">{choice}</mc:Choice><mc:Fallback>{fallback}</mc:Fallback></mc:AlternateContent>"#
        );
    }
    out.push_str("</xdr:wsDr>");
    out
}
