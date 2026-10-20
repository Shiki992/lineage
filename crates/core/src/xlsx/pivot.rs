// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Pivot tables, their caches and slicers (Phase 5B, decision 0029).
//!
//! A pivot table's *definition* (its source, rows, columns, values,
//! filters, calculated fields and layout) is structure, compared as a whole
//! object. Its *cache* is a copy of the source data that Excel refreshes:
//! when only the cache and the pivot's output cells move, that is refreshed
//! data, never someone's edit. The keys here keep the two apart: what a
//! refresh renews (dates, record counts, shared items, the records, the
//! rendered row and column items, the output's size) is left out of the
//! definition key.
//!
//! Parts follow ECMA-376 Part 1 §18.10 ([PivotTableDefinition],
//! [PivotCacheDefinition]); slicers are the x14 extension.
//!
//! [PivotTableDefinition]: https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.pivottabledefinition
//! [PivotCacheDefinition]: https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.spreadsheet.pivotcachedefinition

use roxmltree::Node;

use super::model::Area;
use super::refs::parse_area;
use super::stream::MAIN;
use crate::error::{Error, Result};
use crate::hash::Fnv;
use crate::package::{Package, Relationships, rels_part_for, resolve_target};
use crate::xml;

/// The x14 namespace, which slicers use.
pub const X14: &str = "http://schemas.microsoft.com/office/spreadsheetml/2009/9/main";

/// Where a cache's data comes from.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Source {
    /// A range on a sheet: "Sales", "A1:F120".
    Range { sheet: String, range: String },
    /// A defined name or a table.
    Name(String),
    /// A data connection, by id.
    Connection(u32),
    /// Several ranges consolidated.
    Consolidation,
    /// Something else Lineage doesn't name (scenarios, a data model).
    Other(String),
}

impl Source {
    /// "Sales!A1:F120", "the table Orders", "data connection 2".
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Range { sheet, range } => {
                let plain = sheet.chars().all(|c| c.is_alphanumeric() || c == '_')
                    && !sheet.starts_with(|c: char| c.is_ascii_digit());
                if plain {
                    format!("{sheet}!{range}")
                } else {
                    format!("'{}'!{range}", sheet.replace('\'', "''"))
                }
            }
            Self::Name(n) => n.clone(),
            Self::Connection(id) => format!("data connection {id}"),
            Self::Consolidation => "several ranges".to_owned(),
            Self::Other(kind) => kind.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheField {
    pub name: String,
    /// A calculated field's formula.
    pub formula: Option<String>,
    /// The shared items' text, for naming filtered or hidden items.
    pub items: Vec<String>,
}

/// What a refresh renews in a cache.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Refresh {
    /// `refreshedDate` (a serial date) or `refreshedDateIso`.
    pub date: Option<String>,
    pub record_count: Option<u64>,
    /// A key of the shared items and the records part.
    pub data: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PivotCache {
    pub part: String,
    pub source: Source,
    pub fields: Vec<CacheField>,
    /// Calculated items' formulas.
    pub calculated_items: Vec<String>,
    pub refresh: Refresh,
    /// Built on the data model or an OLAP source: read in part.
    pub olap: bool,
    /// Everything a refresh doesn't renew.
    pub key: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ValueField {
    /// "Sum of Revenue".
    pub name: String,
    pub field: String,
    /// "sum", "count", "average", …
    pub function: String,
    /// "percentOfTotal", …
    pub show_as: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FilterField {
    pub field: String,
    /// The one item it shows, or `None` for all.
    pub item: Option<String>,
}

/// The pivot's fields as the PivotTable Fields pane lists them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Fields {
    pub rows: Vec<String>,
    pub columns: Vec<String>,
    pub values: Vec<ValueField>,
    pub filters: Vec<FilterField>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Layout {
    /// "compact", "outline" or "tabular".
    pub form: &'static str,
    pub row_totals: bool,
    pub column_totals: bool,
    pub style: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PivotTable {
    pub name: String,
    pub part: String,
    /// The sheet it sits on (position in the workbook).
    pub sheet: usize,
    /// Where its cells are, report filters included.
    pub area: Area,
    pub cache: usize,
    pub fields: Fields,
    /// Items hidden in a field: (field, items).
    pub hidden: Vec<(String, Vec<String>)>,
    /// Label and value filters, in words.
    pub filters: Vec<String>,
    pub layout: Layout,
    /// Everything else in the definition, without what a refresh renews.
    pub key: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slicer {
    /// The slicer cache's name ("Slicer_Region").
    pub name: String,
    /// The caption on the slicer ("Region").
    pub caption: String,
    pub field: String,
    /// Names of the pivot tables it filters.
    pub pivots: Vec<String>,
    /// The items it shows; empty when it shows all.
    pub selected: Vec<String>,
    pub part: String,
}

/// Everything about pivots in a workbook.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pivots {
    pub caches: Vec<PivotCache>,
    pub tables: Vec<PivotTable>,
    pub slicers: Vec<Slicer>,
    /// Parts Lineage couldn't read, which stay "Preserved; semantic
    /// tracking not yet supported".
    pub unreadable: Vec<String>,
}

fn attr_u32(n: Node<'_, '_>, name: &str) -> Option<u32> {
    n.attribute(name).and_then(|v| v.parse().ok())
}

fn attr_i64(n: Node<'_, '_>, name: &str) -> Option<i64> {
    n.attribute(name).and_then(|v| v.parse().ok())
}

fn flag(n: Node<'_, '_>, name: &str, default: bool) -> bool {
    match n.attribute(name) {
        Some("1" | "true") => true,
        Some("0" | "false") => false,
        _ => default,
    }
}

fn child<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| xml::is(*c, MAIN, name))
}

fn kids<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(move |c| xml::is(*c, MAIN, name))
}

/// A key of an element and everything under it, attributes in a fixed
/// order, leaving out the attributes and children named.
fn key_of(h: &mut Fnv, n: Node<'_, '_>, skip_attrs: &[&str], skip_children: &[&str]) {
    h.str(n.tag_name().name());
    // Attributes in another namespace (markup compatibility, Excel's
    // revision ids, relationship ids) say nothing about the pivot, and
    // Excel adds and renews them on every save; so is `multipleFieldNames`,
    // which Excel drops when it is the default (seen at stop 1).
    let mut attrs: Vec<(&str, &str)> = n
        .attributes()
        .filter(|a| a.namespace().is_none())
        .filter(|a| !skip_attrs.contains(&a.name()) && a.name() != "multipleFieldNames")
        .map(|a| (a.name(), a.value()))
        .collect();
    attrs.sort_unstable();
    for (k, v) in attrs {
        h.str(k).str(v);
    }
    for c in n.children().filter(Node::is_element) {
        if skip_children.contains(&c.tag_name().name()) {
            continue;
        }
        key_of(h, c, skip_attrs, skip_children);
    }
    h.str("/");
}

fn item_text(n: Node<'_, '_>) -> String {
    match n.tag_name().name() {
        "m" => "(blank)".to_owned(),
        _ => n.attribute("v").unwrap_or("").to_owned(),
    }
}

fn read_cache(package: &Package, part: &str) -> Result<PivotCache> {
    let bytes = package
        .part(part)
        .ok_or_else(|| Error::Damaged("a pivot cache is missing".to_owned()))?;
    let text = xml::text(part, bytes)?;
    let doc = xml::parse(part, text)?;
    let root = doc.root_element();
    if !xml::is(root, MAIN, "pivotCacheDefinition") {
        return Err(Error::BadXml {
            part: part.to_owned(),
            detail: "not a pivot cache".to_owned(),
        });
    }
    let mut olap = false;
    let source = match child(root, "cacheSource") {
        Some(src) => match src.attribute("type") {
            Some("worksheet") | None => match child(src, "worksheetSource") {
                Some(ws) => match (
                    ws.attribute("name"),
                    ws.attribute("sheet"),
                    ws.attribute("ref"),
                ) {
                    (Some(name), _, _) => Source::Name(name.to_owned()),
                    (None, Some(sheet), Some(range)) => Source::Range {
                        sheet: sheet.to_owned(),
                        range: range.to_owned(),
                    },
                    _ => Source::Other("a range".to_owned()),
                },
                None => Source::Other("a range".to_owned()),
            },
            Some("external") => {
                olap = child(root, "cacheHierarchies").is_some();
                attr_u32(src, "connectionId").map_or(
                    Source::Other("an external source".to_owned()),
                    Source::Connection,
                )
            }
            Some("consolidation") => Source::Consolidation,
            Some(other) => Source::Other(other.to_owned()),
        },
        None => Source::Other("unknown".to_owned()),
    };
    let mut fields = Vec::new();
    let mut data = Fnv::new();
    if let Some(cf) = child(root, "cacheFields") {
        for f in kids(cf, "cacheField") {
            let items: Vec<String> = child(f, "sharedItems")
                .map(|s| {
                    s.children()
                        .filter(Node::is_element)
                        .map(item_text)
                        .collect()
                })
                .unwrap_or_default();
            for i in &items {
                data.str(i);
            }
            data.str("|");
            fields.push(CacheField {
                name: f.attribute("name").unwrap_or("").to_owned(),
                formula: f.attribute("formula").map(str::to_owned),
                items,
            });
        }
    }
    let calculated_items = child(root, "calculatedItems")
        .map(|c| {
            kids(c, "calculatedItem")
                .filter_map(|i| i.attribute("formula").map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    // The records: their bytes, as a key.
    let rels = rels_of(package, part)?;
    if let Some(rel) = rels.of_kind("pivotCacheRecords").next() {
        let records = resolve_target(part, &rel.target);
        if let Some(b) = package.part(&records) {
            data.bytes(b);
        }
    }
    let refresh = Refresh {
        date: root
            .attribute("refreshedDateIso")
            .or_else(|| root.attribute("refreshedDate"))
            .map(str::to_owned),
        record_count: root.attribute("recordCount").and_then(|v| v.parse().ok()),
        data: data.finish(),
    };
    let mut key = Fnv::new();
    key_of(
        &mut key,
        root,
        &[
            "refreshedBy",
            "refreshedDate",
            "refreshedDateIso",
            "refreshedVersion",
            "createdVersion",
            "minRefreshableVersion",
            "recordCount",
            "upgradeOnRefresh",
            "id",
            "uid",
            "count",
            "containsBlank",
            "containsMixedTypes",
            "containsSemiMixedTypes",
            "containsString",
            "containsNumber",
            "containsInteger",
            "containsDate",
            "containsNonDate",
            "minValue",
            "maxValue",
            "minDate",
            "maxDate",
            "longText",
        ],
        &["sharedItems", "extLst", "fieldGroup"],
    );
    Ok(PivotCache {
        part: part.to_owned(),
        source,
        fields,
        calculated_items,
        refresh,
        olap,
        key: key.finish(),
    })
}

fn rels_of(package: &Package, part: &str) -> Result<Relationships> {
    let rels_part = rels_part_for(part);
    match package.part(&rels_part) {
        Some(b) => Relationships::parse(&rels_part, b),
        None => Ok(Relationships::default()),
    }
}

fn field_name(cache: &PivotCache, index: i64) -> String {
    if index == -2 {
        return "Values".to_owned();
    }
    usize::try_from(index)
        .ok()
        .and_then(|i| cache.fields.get(i))
        .map_or_else(|| format!("field {}", index + 1), |f| f.name.clone())
}

#[allow(clippy::too_many_lines)]
fn read_table(
    package: &Package,
    part: &str,
    sheet: usize,
    cache: usize,
    caches: &[PivotCache],
) -> Result<PivotTable> {
    let bytes = package
        .part(part)
        .ok_or_else(|| Error::Damaged("a pivot table is missing".to_owned()))?;
    let text = xml::text(part, bytes)?;
    let doc = xml::parse(part, text)?;
    let root = doc.root_element();
    if !xml::is(root, MAIN, "pivotTableDefinition") {
        return Err(Error::BadXml {
            part: part.to_owned(),
            detail: "not a pivot table".to_owned(),
        });
    }
    let c = &caches[cache];
    let name = root.attribute("name").unwrap_or("").to_owned();
    let location = child(root, "location").ok_or_else(|| Error::BadXml {
        part: part.to_owned(),
        detail: "a pivot table without a location".to_owned(),
    })?;
    let mut area = location
        .attribute("ref")
        .and_then(parse_area)
        .ok_or_else(|| Error::BadXml {
            part: part.to_owned(),
            detail: "a pivot location".to_owned(),
        })?;
    // Report filters sit above the table, with a blank row between.
    let page_rows = attr_u32(location, "rowPageCount").unwrap_or(0);
    if page_rows > 0 {
        area.row1 = area.row1.saturating_sub(page_rows + 1).max(1);
    }

    // Each pivot field's hidden items, by name.
    let mut hidden = Vec::new();
    let mut field_items: Vec<Vec<Option<String>>> = Vec::new();
    let mut pf_key = Fnv::new();
    if let Some(pfs) = child(root, "pivotFields") {
        for (k, pf) in kids(pfs, "pivotField").enumerate() {
            let cf = c.fields.get(k);
            let mut items = Vec::new();
            let mut hid = Vec::new();
            if let Some(list) = child(pf, "items") {
                for it in kids(list, "item") {
                    let text = attr_u32(it, "x").and_then(|x| {
                        cf.and_then(|f| f.items.get(usize::try_from(x).ok()?).cloned())
                    });
                    if flag(it, "h", false) {
                        hid.push(text.clone().unwrap_or_else(|| "(an item)".to_owned()));
                    }
                    items.push(text);
                }
            }
            hid.sort();
            if !hid.is_empty() {
                hidden.push((field_name(c, k as i64), hid));
            }
            field_items.push(items);
            // The field's settings, without its item list (a refresh adds
            // and removes items).
            key_of(&mut pf_key, pf, &[], &["items", "extLst"]);
        }
    }
    let axis = |name: &'static str| -> Vec<String> {
        child(root, name)
            .map(|f| {
                kids(f, "field")
                    .filter_map(|x| attr_i64(x, "x"))
                    .map(|x| field_name(c, x))
                    .collect()
            })
            .unwrap_or_default()
    };
    let rows = axis("rowFields");
    let columns = axis("colFields");
    let values: Vec<ValueField> = child(root, "dataFields")
        .map(|d| {
            kids(d, "dataField")
                .map(|f| ValueField {
                    name: f.attribute("name").unwrap_or("").to_owned(),
                    field: field_name(c, attr_i64(f, "fld").unwrap_or(-1)),
                    function: f.attribute("subtotal").unwrap_or("sum").to_owned(),
                    show_as: f
                        .attribute("showDataAs")
                        .filter(|s| *s != "normal")
                        .map(str::to_owned),
                })
                .collect()
        })
        .unwrap_or_default();
    let filters_fields: Vec<FilterField> = child(root, "pageFields")
        .map(|p| {
            kids(p, "pageField")
                .map(|f| {
                    let fld = attr_i64(f, "fld").unwrap_or(-1);
                    let item = attr_u32(f, "item").and_then(|i| {
                        let items = field_items.get(usize::try_from(fld).ok()?)?;
                        items.get(usize::try_from(i).ok()?).cloned().flatten()
                    });
                    FilterField {
                        field: field_name(c, fld),
                        item,
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let filters: Vec<String> = child(root, "filters")
        .map(|fs| {
            kids(fs, "filter")
                .map(|f| {
                    let field = field_name(c, attr_i64(f, "fld").unwrap_or(-1));
                    let kind = f.attribute("type").unwrap_or("filter");
                    match f.attribute("stringValue1") {
                        Some(v) => format!("{field}: {kind} {v}"),
                        None => format!("{field}: {kind}"),
                    }
                })
                .collect()
        })
        .unwrap_or_default();
    let compact = flag(root, "compact", true);
    let outline = flag(root, "outline", false);
    let form = if compact {
        "compact"
    } else if outline {
        "outline"
    } else {
        "tabular"
    };
    let layout = Layout {
        form,
        row_totals: flag(root, "rowGrandTotals", true),
        column_totals: flag(root, "colGrandTotals", true),
        style: child(root, "pivotTableStyleInfo")
            .and_then(|s| s.attribute("name"))
            .map(str::to_owned),
    };
    let mut key = Fnv::new();
    key.u64(pf_key.finish());
    key_of(
        &mut key,
        root,
        &[
            "updatedVersion",
            "createdVersion",
            "minRefreshableVersion",
            "cacheId",
            "count",
            "ref",
            "firstHeaderRow",
            "firstDataRow",
            "firstDataCol",
            "uid",
        ],
        &["rowItems", "colItems", "pivotFields", "extLst", "location"],
    );
    Ok(PivotTable {
        name,
        part: part.to_owned(),
        sheet,
        area,
        cache,
        fields: Fields {
            rows,
            columns,
            values,
            filters: filters_fields,
        },
        hidden,
        filters,
        layout,
        key: key.finish(),
    })
}

fn read_slicer_cache(package: &Package, part: &str, pivots: &Pivots) -> Result<Slicer> {
    let bytes = package
        .part(part)
        .ok_or_else(|| Error::Damaged("a slicer cache is missing".to_owned()))?;
    let text = xml::text(part, bytes)?;
    let doc = xml::parse(part, text)?;
    let root = doc.root_element();
    if !xml::is(root, X14, "slicerCacheDefinition") {
        return Err(Error::BadXml {
            part: part.to_owned(),
            detail: "not a slicer cache".to_owned(),
        });
    }
    fn x14<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> Option<Node<'a, 'i>> {
        n.children().find(move |c| xml::is(*c, X14, name))
    }
    let field = root.attribute("sourceName").unwrap_or("").to_owned();
    let names: Vec<String> = x14(root, "pivotTables")
        .map(|p| {
            p.children()
                .filter(|c| xml::is(*c, X14, "pivotTable"))
                .filter_map(|c| c.attribute("name").map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    // The field's items, from the cache of the first pivot it filters.
    let items: Vec<String> = names
        .first()
        .and_then(|n| pivots.tables.iter().find(|t| &t.name == n))
        .and_then(|t| pivots.caches.get(t.cache))
        .and_then(|c| c.fields.iter().find(|f| f.name == field))
        .map(|f| f.items.clone())
        .unwrap_or_default();
    let mut selected = Vec::new();
    let mut all = true;
    if let Some(tab) = x14(root, "data").and_then(|d| x14(d, "tabular"))
        && let Some(list) = x14(tab, "items")
    {
        for i in list.children().filter(|c| xml::is(*c, X14, "i")) {
            let x = attr_u32(i, "x").and_then(|x| usize::try_from(x).ok());
            if flag(i, "s", false) {
                selected.push(
                    x.and_then(|x| items.get(x).cloned())
                        .unwrap_or_else(|| "(an item)".to_owned()),
                );
            } else if !flag(i, "nd", false) {
                all = false;
            }
        }
    }
    if all {
        selected.clear();
    }
    Ok(Slicer {
        name: root.attribute("name").unwrap_or("").to_owned(),
        caption: field.clone(),
        field,
        pivots: names,
        selected,
        part: part.to_owned(),
    })
}

/// Reads every pivot table (from the sheets' relationships), its cache and
/// the slicers. A part that can't be read goes to `unreadable`, and the
/// rest are still read.
///
/// `sheets` are the worksheet parts in tab order.
#[must_use]
pub fn read(
    package: &Package,
    main_part: &str,
    main_rels: &Relationships,
    sheets: &[String],
) -> Pivots {
    let mut out = Pivots::default();
    for (index, sheet_part) in sheets.iter().enumerate() {
        let Ok(rels) = rels_of(package, sheet_part) else {
            continue;
        };
        for rel in rels.of_kind("pivotTable") {
            let part = resolve_target(sheet_part, &rel.target);
            let cache_part = rels_of(package, &part).ok().and_then(|r| {
                r.of_kind("pivotCacheDefinition")
                    .next()
                    .map(|c| resolve_target(&part, &c.target))
            });
            let Some(cache_part) = cache_part else {
                out.unreadable.push(part);
                continue;
            };
            let cache = match out.caches.iter().position(|c| c.part == cache_part) {
                Some(c) => c,
                None => match read_cache(package, &cache_part) {
                    Ok(c) => {
                        out.caches.push(c);
                        out.caches.len() - 1
                    }
                    Err(_) => {
                        out.unreadable.push(cache_part);
                        out.unreadable.push(part);
                        continue;
                    }
                },
            };
            match read_table(package, &part, index, cache, &out.caches) {
                Ok(t) => out.tables.push(t),
                Err(_) => out.unreadable.push(part),
            }
        }
        // Slicer captions are on the sheet's slicer parts.
    }
    let captions = slicer_captions(package, sheets);
    for rel in main_rels.of_kind("slicerCache") {
        let part = resolve_target(main_part, &rel.target);
        match read_slicer_cache(package, &part, &out) {
            Ok(mut s) => {
                if let Some(c) = captions.iter().find(|c| c.0 == s.name) {
                    s.caption.clone_from(&c.1);
                }
                // Slicers on tables (no pivot) stay untracked.
                if s.pivots.is_empty() {
                    out.unreadable.push(part);
                } else {
                    out.slicers.push(s);
                }
            }
            Err(_) => out.unreadable.push(part),
        }
    }
    out
}

/// The parts a successful read covers, so they leave the untracked list:
/// the pivot tables, their caches and records, and the slicer caches and
/// sheet slicer parts whose every slicer is tracked.
#[must_use]
pub fn covered_parts(package: &Package, sheets: &[String], pivots: &Pivots) -> Vec<String> {
    let mut out: Vec<String> = pivots.tables.iter().map(|t| t.part.clone()).collect();
    for c in &pivots.caches {
        out.push(c.part.clone());
        if let Ok(rels) = rels_of(package, &c.part) {
            for r in rels.of_kind("pivotCacheRecords") {
                out.push(resolve_target(&c.part, &r.target));
            }
        }
    }
    out.extend(pivots.slicers.iter().map(|s| s.part.clone()));
    let tracked: Vec<&str> = pivots.slicers.iter().map(|s| s.name.as_str()).collect();
    for (part, caches) in slicer_parts(package, sheets) {
        if !caches.is_empty() && caches.iter().all(|c| tracked.contains(&c.0.as_str())) {
            out.push(part);
        }
    }
    out
}

/// (slicer cache name, caption) from the sheets' slicer parts.
fn slicer_captions(package: &Package, sheets: &[String]) -> Vec<(String, String)> {
    slicer_parts(package, sheets)
        .into_iter()
        .flat_map(|(_, list)| list)
        .collect()
}

/// Each sheet slicer part, with (cache name, caption) of its slicers.
fn slicer_parts(package: &Package, sheets: &[String]) -> Vec<(String, Vec<(String, String)>)> {
    let mut out = Vec::new();
    for sheet_part in sheets {
        let Ok(rels) = rels_of(package, sheet_part) else {
            continue;
        };
        for rel in rels.of_kind("slicer") {
            let part = resolve_target(sheet_part, &rel.target);
            let Some(bytes) = package.part(&part) else {
                continue;
            };
            let Ok(text) = xml::text(&part, bytes) else {
                continue;
            };
            let Ok(doc) = xml::parse(&part, text) else {
                continue;
            };
            let mut list = Vec::new();
            for s in doc
                .root_element()
                .children()
                .filter(|c| xml::is(*c, X14, "slicer"))
            {
                if let Some(cache) = s.attribute("cache") {
                    let caption = s.attribute("caption").unwrap_or(cache);
                    list.push((cache.to_owned(), caption.to_owned()));
                }
            }
            out.push((part, list));
        }
    }
    out
}

// --- What changed ------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PivotChangeKind {
    Added,
    Removed,
    /// Its definition changed (perhaps refreshed too).
    Changed,
    /// Only refreshed: Excel renewed its data. Never an edit.
    Refreshed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PivotChange {
    pub id: String,
    pub name: String,
    /// The sheet it is on, in the later version (the earlier for a removed
    /// one).
    pub sheet: String,
    pub kind: PivotChangeKind,
    /// One line per thing that changed: "Rows: Plan (was Region)".
    pub details: Vec<String>,
    pub before: Option<Fields>,
    pub after: Option<Fields>,
    /// Its data was refreshed (its cache's records or date moved).
    pub refreshed: bool,
    /// Its output cells that changed; set by the workbook diff.
    pub cells: usize,
    /// Where it is in the later version, for Go to.
    pub area: Option<Area>,
}

impl PivotChange {
    /// "Pivot table RevenueByRegion: Rows: Plan (was Region)".
    #[must_use]
    pub fn summary(&self) -> String {
        match self.kind {
            PivotChangeKind::Added => format!("Added pivot table {} on {}", self.name, self.sheet),
            PivotChangeKind::Removed => format!("Removed pivot table {}", self.name),
            PivotChangeKind::Refreshed => format!(
                "Pivot table {} was refreshed{}",
                self.name,
                match self.cells {
                    0 => String::new(),
                    1 => ": 1 value changed".to_owned(),
                    n => format!(": {n} values changed"),
                }
            ),
            PivotChangeKind::Changed => match self.details.as_slice() {
                [one] => format!("Pivot table {}: {one}", self.name),
                _ => format!("Pivot table {} changed", self.name),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlicerChange {
    pub id: String,
    pub caption: String,
    /// "Slicer Region: shows North, West (was all)".
    pub summary: String,
}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        "none".to_owned()
    } else {
        items.join(", ")
    }
}

fn values(v: &[ValueField]) -> Vec<String> {
    v.iter()
        .map(|f| match &f.show_as {
            Some(s) => format!("{} ({s})", f.name),
            None => f.name.clone(),
        })
        .collect()
}

fn shows(item: Option<&String>) -> String {
    item.map_or_else(|| "all".to_owned(), Clone::clone)
}

fn calculated(c: &PivotCache) -> Vec<(String, String)> {
    c.fields
        .iter()
        .filter_map(|f| f.formula.as_ref().map(|x| (f.name.clone(), x.clone())))
        .collect()
}

/// What changed in one pivot's definition, in words.
pub(crate) fn details(
    ta: &PivotTable,
    ca: &PivotCache,
    tb: &PivotTable,
    cb: &PivotCache,
) -> Vec<String> {
    let mut out = Vec::new();
    if ca.source != cb.source {
        out.push(format!(
            "Source: {} (was {})",
            cb.source.label(),
            ca.source.label()
        ));
    }
    let (fa, fb) = (&ta.fields, &tb.fields);
    if fa.rows != fb.rows {
        out.push(format!("Rows: {} (was {})", list(&fb.rows), list(&fa.rows)));
    }
    if fa.columns != fb.columns {
        out.push(format!(
            "Columns: {} (was {})",
            list(&fb.columns),
            list(&fa.columns)
        ));
    }
    if fa.values != fb.values {
        out.push(format!(
            "Values: {} (was {})",
            list(&values(&fb.values)),
            list(&values(&fa.values))
        ));
    }
    for f in &fb.filters {
        match fa.filters.iter().find(|g| g.field == f.field) {
            None => out.push(format!(
                "Filter {} added, showing {}",
                f.field,
                shows(f.item.as_ref())
            )),
            Some(g) if g.item != f.item => out.push(format!(
                "Filter {}: shows {} (was {})",
                f.field,
                shows(f.item.as_ref()),
                shows(g.item.as_ref())
            )),
            Some(_) => {}
        }
    }
    for g in &fa.filters {
        if !fb.filters.iter().any(|f| f.field == g.field) {
            out.push(format!("Filter {} removed", g.field));
        }
    }
    let fields: Vec<&String> = {
        let mut v: Vec<&String> = ta.hidden.iter().chain(&tb.hidden).map(|(f, _)| f).collect();
        v.sort();
        v.dedup();
        v
    };
    for f in fields {
        let ha = ta
            .hidden
            .iter()
            .find(|(g, _)| g == f)
            .map(|(_, i)| i.clone())
            .unwrap_or_default();
        let hb = tb
            .hidden
            .iter()
            .find(|(g, _)| g == f)
            .map(|(_, i)| i.clone())
            .unwrap_or_default();
        if ha != hb {
            let now = if hb.is_empty() {
                "shows every item".to_owned()
            } else {
                format!("hides {}", hb.join(", "))
            };
            let was = if ha.is_empty() {
                "showed every item".to_owned()
            } else {
                format!("hid {}", ha.join(", "))
            };
            out.push(format!("{f}: {now} ({was})"));
        }
    }
    if ta.filters != tb.filters {
        out.push(format!(
            "Label and value filters: {} (was {})",
            list(&tb.filters),
            list(&ta.filters)
        ));
    }
    let (xa, xb) = (calculated(ca), calculated(cb));
    for (name, formula) in &xb {
        match xa.iter().find(|(n, _)| n == name) {
            None => out.push(format!("Added calculated field {name}: ={formula}")),
            Some((_, was)) if was != formula => {
                out.push(format!("Calculated field {name}: ={formula} (was ={was})"))
            }
            Some(_) => {}
        }
    }
    for (name, _) in &xa {
        if !xb.iter().any(|(n, _)| n == name) {
            out.push(format!("Removed calculated field {name}"));
        }
    }
    if ca.calculated_items != cb.calculated_items {
        out.push("Calculated items changed".to_owned());
    }
    let (la, lb) = (&ta.layout, &tb.layout);
    if la.form != lb.form {
        out.push(format!("Layout: {} form (was {} form)", lb.form, la.form));
    }
    if la.row_totals != lb.row_totals || la.column_totals != lb.column_totals {
        out.push("Grand totals changed".to_owned());
    }
    if la.style != lb.style {
        out.push(format!(
            "Style: {} (was {})",
            lb.style.as_deref().unwrap_or("none"),
            la.style.as_deref().unwrap_or("none")
        ));
    }
    if out.is_empty() && (ta.key != tb.key || ca.key != cb.key) {
        out.push("Other settings changed".to_owned());
    }
    out
}

/// What changed in the pivot tables and slicers from `a` to `b`.
/// `sheet_b` maps a sheet position in `a` to its position in `b`, when the
/// sheet is in both.
#[must_use]
pub fn diff(
    a: &Pivots,
    b: &Pivots,
    sheet_names_a: &[String],
    sheet_names_b: &[String],
    sheet_b: &dyn Fn(usize) -> Option<usize>,
) -> (Vec<PivotChange>, Vec<SlicerChange>) {
    let mut out = Vec::new();
    let mut matched = vec![false; b.tables.len()];
    let id =
        |parts: &[&str]| crate::hash::sha256_hex(parts.join("\u{1f}").as_bytes())[..16].to_owned();
    for ta in &a.tables {
        let jb = sheet_b(ta.sheet);
        let found = b.tables.iter().enumerate().position(|(k, tb)| {
            !matched[k] && tb.name.eq_ignore_ascii_case(&ta.name) && Some(tb.sheet) == jb
        });
        let found = found.or_else(|| {
            b.tables
                .iter()
                .enumerate()
                .position(|(k, tb)| !matched[k] && tb.name.eq_ignore_ascii_case(&ta.name))
        });
        let sheet_a = sheet_names_a.get(ta.sheet).cloned().unwrap_or_default();
        let Some(k) = found else {
            out.push(PivotChange {
                id: id(&["pivot", &sheet_a, &ta.name, "removed"]),
                name: ta.name.clone(),
                sheet: sheet_a,
                kind: PivotChangeKind::Removed,
                details: Vec::new(),
                before: Some(ta.fields.clone()),
                after: None,
                refreshed: false,
                cells: 0,
                area: None,
            });
            continue;
        };
        matched[k] = true;
        let tb = &b.tables[k];
        let (ca, cb) = (&a.caches[ta.cache], &b.caches[tb.cache]);
        let details = details(ta, ca, tb, cb);
        let refreshed = ca.refresh != cb.refresh;
        let sheet = sheet_names_b.get(tb.sheet).cloned().unwrap_or_default();
        let kind = if !details.is_empty() {
            PivotChangeKind::Changed
        } else if refreshed {
            PivotChangeKind::Refreshed
        } else {
            continue;
        };
        let mut details = details;
        if kind == PivotChangeKind::Changed && refreshed {
            details.push("Its data was refreshed".to_owned());
        }
        out.push(PivotChange {
            id: id(&[
                "pivot",
                &sheet,
                &tb.name,
                &format!("{kind:?}"),
                &details.join("|"),
            ]),
            name: tb.name.clone(),
            sheet,
            kind,
            details,
            before: Some(ta.fields.clone()),
            after: Some(tb.fields.clone()),
            refreshed,
            cells: 0,
            area: Some(tb.area),
        });
    }
    for (k, tb) in b.tables.iter().enumerate() {
        if matched[k] {
            continue;
        }
        let sheet = sheet_names_b.get(tb.sheet).cloned().unwrap_or_default();
        out.push(PivotChange {
            id: id(&["pivot", &sheet, &tb.name, "added"]),
            name: tb.name.clone(),
            sheet,
            kind: PivotChangeKind::Added,
            details: Vec::new(),
            before: None,
            after: Some(tb.fields.clone()),
            refreshed: false,
            cells: 0,
            area: Some(tb.area),
        });
    }
    let mut slicers = Vec::new();
    for sb in &b.slicers {
        match a.slicers.iter().find(|s| s.name == sb.name) {
            None => slicers.push(SlicerChange {
                id: id(&["slicer", &sb.name, "added"]),
                caption: sb.caption.clone(),
                summary: format!("Added slicer {} on {}", sb.caption, sb.pivots.join(", ")),
            }),
            Some(sa) if sa.selected != sb.selected || sa.pivots != sb.pivots => {
                let now = if sb.selected.is_empty() {
                    "all".to_owned()
                } else {
                    sb.selected.join(", ")
                };
                let was = if sa.selected.is_empty() {
                    "all".to_owned()
                } else {
                    sa.selected.join(", ")
                };
                let summary = if sa.selected == sb.selected {
                    format!(
                        "Slicer {}: now filters {} (was {})",
                        sb.caption,
                        sb.pivots.join(", "),
                        sa.pivots.join(", ")
                    )
                } else {
                    format!("Slicer {}: shows {now} (was {was})", sb.caption)
                };
                slicers.push(SlicerChange {
                    id: id(&["slicer", &sb.name, &summary]),
                    caption: sb.caption.clone(),
                    summary,
                });
            }
            Some(_) => {}
        }
    }
    for sa in &a.slicers {
        if !b.slicers.iter().any(|s| s.name == sa.name) {
            slicers.push(SlicerChange {
                id: id(&["slicer", &sa.name, "removed"]),
                caption: sa.caption.clone(),
                summary: format!("Removed slicer {}", sa.caption),
            });
        }
    }
    (out, slicers)
}

#[cfg(test)]
mod tests;
