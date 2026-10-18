// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Reading the workbook part, its relationships, tables, data connections,
//! and the parts the engine keeps without tracking.

use super::model::{
    Area, ConnectionDef, DefinedName, SheetKind, SheetState, TableDef, Unsupported, UnsupportedKind,
};
use super::refs::parse_area;
use super::stream::{MAIN, MAIN_STRICT};
use super::styles::node_key;
use crate::error::{Error, Result};
use crate::hash::Fnv;
use crate::package::{ContentTypes, Package, Relationships, rels_part_for, resolve_target};
use crate::xml::{self, ns};

pub const REL_WORKSHEET: &str = "worksheet";
pub const REL_CHARTSHEET: &str = "chartsheet";
pub const REL_DIALOGSHEET: &str = "dialogsheet";
pub const REL_MACROSHEET: &str = "xlMacrosheet";

/// The content types of a workbook's main part.
pub const CT_WORKBOOK: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
pub const CT_WORKBOOK_MACRO: &str = "application/vnd.ms-excel.sheet.macroEnabled.main+xml";
pub const CT_TEMPLATE: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml";
pub const CT_TEMPLATE_MACRO: &str = "application/vnd.ms-excel.template.macroEnabled.main+xml";
pub const CT_BINARY: &str = "application/vnd.ms-excel.sheet.binary.macroEnabled.main";

pub(super) fn rels(package: &Package, part: &str) -> Result<Relationships> {
    let rels_part = rels_part_for(part);
    match package.part(&rels_part) {
        Some(bytes) => Relationships::parse(&rels_part, bytes),
        None => Ok(Relationships::default()),
    }
}

/// The main part and whether it is macro-enabled.
pub(super) fn main_part(package: &Package) -> Result<(String, bool)> {
    let types = package
        .part(ContentTypes::PART)
        .ok_or_else(|| Error::NotAnOfficeFile("no content types".to_owned()))
        .and_then(ContentTypes::parse)?;
    let root = rels(package, "")?;
    let part = root
        .of_kind("officeDocument")
        .next()
        .map(|rel| resolve_target("", &rel.target))
        .ok_or_else(|| Error::NotAnOfficeFile("no main part".to_owned()))?;
    match types.of(&part).unwrap_or_default() {
        CT_WORKBOOK | CT_TEMPLATE => Ok((part, false)),
        CT_WORKBOOK_MACRO | CT_TEMPLATE_MACRO => Ok((part, true)),
        CT_BINARY => Err(Error::Unsupported("binary workbooks (.xlsb)".to_owned())),
        _ => Err(Error::NotAWorkbook("not a workbook".to_owned())),
    }
}

/// One `<sheet>` of the workbook part.
#[derive(Clone, Debug)]
pub(super) struct SheetEntry {
    pub name: String,
    pub sheet_id: u32,
    pub state: SheetState,
    pub part: String,
    pub kind: SheetKind,
}

/// What the workbook part says.
#[derive(Debug, Default)]
pub(super) struct WorkbookPart {
    pub sheets: Vec<SheetEntry>,
    pub names: Vec<DefinedName>,
    pub date1904: bool,
    pub settings: u64,
    pub calc: u64,
}

pub(super) fn workbook_part(
    package: &Package,
    main: &str,
    main_rels: &Relationships,
) -> Result<WorkbookPart> {
    let bytes = package
        .part(main)
        .ok_or_else(|| Error::NotAWorkbook("the workbook part is missing".to_owned()))?;
    let text = xml::text(main, bytes)?;
    let doc = xml::parse(main, text)?;
    let root = doc.root_element();
    if xml::is(root, MAIN_STRICT, "workbook") {
        return Err(Error::Unsupported("Strict Open XML".to_owned()));
    }
    if !xml::is(root, MAIN, "workbook") {
        return Err(Error::NotAWorkbook("no workbook element".to_owned()));
    }
    let mut out = WorkbookPart::default();
    let mut settings = Fnv::new();
    for child in root.children().filter(roxmltree::Node::is_element) {
        let in_main = child.tag_name().namespace() == Some(MAIN);
        match (in_main, child.tag_name().name()) {
            (true, "workbookPr") => {
                out.date1904 = matches!(child.attribute("date1904"), Some("1" | "true"));
                let mut h = Fnv::new();
                for (name, value) in child.attributes().map(|a| (a.name(), a.value())) {
                    // Renewed by Excel on save.
                    if name != "defaultThemeVersion" {
                        h.str(name).str(value);
                    }
                }
                settings.u64(h.finish());
            }
            (true, "sheets") => {
                for sheet in child.children().filter(|n| xml::is(*n, MAIN, "sheet")) {
                    let name = sheet
                        .attribute("name")
                        .ok_or_else(|| Error::Damaged("a sheet without a name".to_owned()))?;
                    let rid = sheet
                        .attribute((ns::R, "id"))
                        .ok_or_else(|| Error::Damaged("a sheet without a part".to_owned()))?;
                    let rel = main_rels
                        .by_id(rid)
                        .ok_or_else(|| Error::Damaged("a sheet's part is missing".to_owned()))?;
                    let kind_tail = rel.rel_type.rsplit('/').next().unwrap_or("");
                    let kind = match kind_tail {
                        REL_WORKSHEET => SheetKind::Worksheet,
                        REL_CHARTSHEET => SheetKind::Chart,
                        REL_DIALOGSHEET | REL_MACROSHEET => SheetKind::Other,
                        _ => return Err(Error::Damaged("a sheet of an unknown kind".to_owned())),
                    };
                    let state = match sheet.attribute("state") {
                        Some("hidden") => SheetState::Hidden,
                        Some("veryHidden") => SheetState::VeryHidden,
                        _ => SheetState::Visible,
                    };
                    if out.sheets.iter().any(|s| s.name.eq_ignore_ascii_case(name)) {
                        return Err(Error::Damaged("two sheets share a name".to_owned()));
                    }
                    out.sheets.push(SheetEntry {
                        name: name.to_owned(),
                        sheet_id: sheet
                            .attribute("sheetId")
                            .and_then(|v| v.parse().ok())
                            .unwrap_or(0),
                        state,
                        part: resolve_target(main, &rel.target),
                        kind,
                    });
                }
            }
            (true, "definedNames") => {
                for n in child
                    .children()
                    .filter(|n| xml::is(*n, MAIN, "definedName"))
                {
                    let Some(name) = n.attribute("name") else {
                        continue;
                    };
                    out.names.push(DefinedName {
                        name: name.to_owned(),
                        local: n.attribute("localSheetId").and_then(|v| v.parse().ok()),
                        hidden: matches!(n.attribute("hidden"), Some("1" | "true")),
                        refers_to: n.text().unwrap_or("").to_owned(),
                    });
                }
            }
            (true, "calcPr") => {
                let mut h = Fnv::new();
                for a in child.attributes() {
                    // Excel stamps its own build into calcId on every save.
                    if a.name() != "calcId" {
                        h.str(a.name()).str(a.value());
                    }
                }
                out.calc = h.finish();
            }
            (true, "workbookProtection" | "fileSharing") => node_key(&mut settings, child),
            // Views, versions and revision marks change on every save.
            _ => {}
        }
    }
    out.settings = settings.finish();
    if out.sheets.is_empty() {
        return Err(Error::NotAWorkbook("no sheets".to_owned()));
    }
    Ok(out)
}

/// Reads a table part.
pub(super) fn table(package: &Package, part: &str, sheet: usize) -> Result<Option<TableDef>> {
    let Some(bytes) = package.part(part) else {
        return Ok(None);
    };
    let text = xml::text(part, bytes)?;
    let doc = xml::parse(part, text)?;
    let root = doc.root_element();
    if !xml::is(root, MAIN, "table") {
        return Err(Error::BadXml {
            part: part.to_owned(),
            detail: "not a table".to_owned(),
        });
    }
    let Some(area) = root.attribute("ref").and_then(parse_area) else {
        return Err(Error::Damaged("a table without a range".to_owned()));
    };
    let columns = xml::child(root, MAIN, "tableColumns")
        .map(|list| {
            list.children()
                .filter(|n| xml::is(*n, MAIN, "tableColumn"))
                .map(|n| n.attribute("name").unwrap_or("").to_owned())
                .collect()
        })
        .unwrap_or_default();
    let num = |name: &str, default: u32| {
        root.attribute(name)
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let mut key = Fnv::new();
    for a in root.attributes() {
        if !matches!(a.name(), "id" | "ref" | "uid") {
            key.str(a.name()).str(a.value());
        }
    }
    for child in root.children().filter(roxmltree::Node::is_element) {
        if child.tag_name().name() != "autoFilter" {
            node_key(&mut key, child);
        }
    }
    Ok(Some(TableDef {
        sheet,
        part: part.to_owned(),
        name: root
            .attribute("displayName")
            .or_else(|| root.attribute("name"))
            .unwrap_or("")
            .to_owned(),
        area,
        columns,
        header_rows: num("headerRowCount", 1),
        totals_rows: num("totalsRowCount", 0),
        query: root.attribute("tableType") == Some("queryTable"),
        key: key.finish(),
    }))
}

/// Reads `xl/connections.xml`, if there is one.
pub(super) fn connections(
    package: &Package,
    main_rels: &Relationships,
    main: &str,
) -> Result<Vec<ConnectionDef>> {
    let Some(rel) = main_rels.of_kind("connections").next() else {
        return Ok(Vec::new());
    };
    let part = resolve_target(main, &rel.target);
    let Some(bytes) = package.part(&part) else {
        return Ok(Vec::new());
    };
    let text = xml::text(&part, bytes)?;
    let doc = xml::parse(&part, text)?;
    Ok(doc
        .root_element()
        .children()
        .filter(|n| xml::is(*n, MAIN, "connection"))
        .map(|n| {
            let mut h = Fnv::new();
            node_key(&mut h, n);
            ConnectionDef {
                id: n.attribute("id").and_then(|v| v.parse().ok()).unwrap_or(0),
                name: n.attribute("name").unwrap_or("").to_owned(),
                kind: n
                    .attribute("type")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                key: h.finish(),
            }
        })
        .collect())
}

/// The range a sheet-level query table fills: the sheet-scoped defined name
/// of the same name ("ExternalData_1").
pub(super) fn query_area(
    names: &[DefinedName],
    sheet: usize,
    query_part: &str,
    package: &Package,
) -> Option<Area> {
    let bytes = package.part(query_part)?;
    let text = xml::text(query_part, bytes).ok()?;
    let doc = xml::parse(query_part, text).ok()?;
    let name = doc.root_element().attribute("name")?;
    let defined = names
        .iter()
        .find(|n| n.name == name && n.local == Some(sheet))?;
    let target = defined
        .refers_to
        .rsplit_once('!')
        .map_or(defined.refers_to.as_str(), |(_, a)| a);
    parse_area(target)
}

/// Every part the engine keeps but doesn't track, by content type.
pub(super) fn unsupported(package: &Package) -> Result<Vec<Unsupported>> {
    let types = package
        .part(ContentTypes::PART)
        .map(ContentTypes::parse)
        .transpose()?
        .unwrap_or_default();
    let mut out = Vec::new();
    for name in package.part_names() {
        let content_type = types.of(name).unwrap_or_default();
        let kind = if content_type == "application/vnd.ms-office.vbaProject"
            || name.to_ascii_lowercase().ends_with("vbaproject.bin")
        {
            Some(UnsupportedKind::Macros)
        } else if content_type.contains("pivotTable")
            || content_type.contains("pivotCacheDefinition")
        {
            Some(UnsupportedKind::PivotTables)
        } else if content_type
            == "application/vnd.openxmlformats-officedocument.drawingml.chart+xml"
        {
            Some(UnsupportedKind::Charts)
        } else if content_type.contains("externalLink") {
            Some(UnsupportedKind::ExternalLinks)
        } else if content_type.contains("slicer") || content_type.contains("timeline") {
            Some(UnsupportedKind::Slicers)
        } else if content_type.starts_with("image/") {
            Some(UnsupportedKind::Pictures)
        } else if content_type.contains("rdrichvalue") || name.starts_with("xl/richData/") {
            Some(UnsupportedKind::DataTypes)
        } else if content_type.contains("chartsheet") {
            Some(UnsupportedKind::ChartSheets)
        } else if content_type.contains("dialogsheet") || content_type.contains("macrosheet") {
            Some(UnsupportedKind::MacroSheets)
        } else if name.starts_with("customXml/item")
            && package
                .part(name)
                .is_some_and(|b| contains(b, b"DataMashup") || contains(b, &utf16(b"DataMashup")))
        {
            Some(UnsupportedKind::PowerQuery)
        } else {
            None
        };
        if let Some(kind) = kind {
            out.push(Unsupported {
                kind,
                part: name.to_owned(),
            });
        }
    }
    Ok(out)
}

/// Power Query's item is often saved as UTF-16.
fn utf16(ascii: &[u8]) -> Vec<u8> {
    ascii.iter().flat_map(|b| [*b, 0]).collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
