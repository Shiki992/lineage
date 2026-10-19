// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Clean copies of workbooks for sending (CLAUDE.md: "copies that leave are
//! clean"; decision 0026).
//!
//! Left out: notes and threaded comments (with their people and the VML
//! drawings that only showed notes), revision logs, the thumbnail, add-in
//! data, custom XML that isn't a Power Query, personal metadata, and every
//! custom property but the two Lineage ids (or none).
//!
//! Kept, and counted so Send a copy can say so: hidden sheets, rows and
//! columns, because formulas may read them and removing them would change
//! the numbers (the maintainer's choice); macros; and Power Query queries,
//! which the workbook's data needs.

use std::collections::BTreeSet;

use super::Workbook;
use super::model::{SheetState, UnsupportedKind};
use crate::clean::{
    CleanReport, empty_properties, only_ids, prune_content_types, prune_relationships, source_of,
};
use crate::error::{Error, Result};
use crate::package::props::LineageIds;
use crate::package::{Relationships, rels_part_for, resolve_target};
use crate::xml::{self, Edits};

fn removable(name: &str, package: &crate::Package) -> bool {
    let lower = name.to_ascii_lowercase();
    (lower.starts_with("xl/comments") && lower.ends_with(".xml"))
        || lower.starts_with("xl/threadedcomments/")
        || lower.starts_with("xl/persons/")
        || lower.starts_with("xl/revisions/")
        || lower.starts_with("docprops/thumbnail")
        || lower.contains("webextensions/")
        || (lower.starts_with("customxml/")
            && !package.part(name).is_some_and(|b| {
                let mashup: Vec<u8> = b"DataMashup".iter().flat_map(|c| [*c, 0]).collect();
                b.windows(10).any(|w| w == b"DataMashup")
                    || b.windows(20).any(|w| w == mashup.as_slice())
            })
            && !customxml_props_of_mashup(name, package))
}

/// `customXml/itemProps1.xml` and `customXml/_rels/item1.xml.rels` belong to
/// their item: kept when the item is a Power Query.
fn customxml_props_of_mashup(name: &str, package: &crate::Package) -> bool {
    let lower = name.to_ascii_lowercase();
    let number: String = lower.chars().filter(char::is_ascii_digit).collect();
    if number.is_empty() {
        return false;
    }
    let item = format!("customXml/item{number}.xml");
    package.part(&item).is_some_and(|b| {
        let mashup: Vec<u8> = b"DataMashup".iter().flat_map(|c| [*c, 0]).collect();
        b.windows(10).any(|w| w == b"DataMashup") || b.windows(20).any(|w| w == mashup.as_slice())
    })
}

/// The value of the root element's `refreshedBy` attribute, as a byte range,
/// when it isn't empty.
fn refreshed_by(text: &str) -> Option<std::ops::Range<usize>> {
    let start = text.find("<pivotCacheDefinition")?;
    let end = start + text[start..].find('>')?;
    let tag = &text[start..end];
    let at = tag
        .find(" refreshedBy=\"")
        .map(|i| i + " refreshedBy=\"".len())?;
    let close = tag[at..].find('"')?;
    (close > 0).then(|| start + at..start + at + close)
}

/// Makes a clean copy of a workbook for sending. With `ids`, the copy
/// carries the two Lineage ids as its only custom properties; without, none.
///
/// # Errors
/// [`Error`] when the workbook can't be read or cleaned safely.
pub fn clean_copy(bytes: &[u8], ids: Option<&LineageIds>) -> Result<(Vec<u8>, CleanReport)> {
    let wb = Workbook::open(bytes)?;
    let mut report = CleanReport {
        hidden_sheets: wb
            .sheets
            .iter()
            .filter(|s| s.state != SheetState::Visible)
            .count(),
        hidden_rows: wb
            .sheets
            .iter()
            .map(|s| s.rows.iter().filter(|r| r.hidden).count())
            .sum(),
        hidden_columns: wb
            .sheets
            .iter()
            .map(|s| {
                s.cols
                    .iter()
                    .filter(|c| c.hidden)
                    .map(|c| (c.max - c.min + 1) as usize)
                    .sum::<usize>()
            })
            .sum(),
        // Read (decision 0028) or kept untracked, macros go with the copy.
        macros: wb.vba.is_some()
            || wb
                .unsupported
                .iter()
                .any(|u| u.kind == UnsupportedKind::Macros),
        ..CleanReport::default()
    };
    let mut package = wb.package.clone();

    // Notes are counted, then their parts go, with the VML drawings that
    // only showed them and the sheets' links to those drawings.
    let mut vml_gone: Vec<String> = Vec::new();
    for sheet in &wb.sheets {
        let rels_part = rels_part_for(&sheet.part);
        let Some(bytes) = package.part(&rels_part).map(<[u8]>::to_vec) else {
            continue;
        };
        let rels = Relationships::parse(&rels_part, &bytes)?;
        let has_notes = rels.of_kind("comments").next().is_some();
        for c in rels
            .of_kind("comments")
            .chain(rels.of_kind("threadedComment"))
        {
            let part = resolve_target(&sheet.part, &c.target);
            if let Some(b) = package.part(&part)
                && let Ok(text) = xml::text(&part, b)
                && let Ok(doc) = xml::parse(&part, text)
            {
                report.comments += doc
                    .descendants()
                    .filter(|n| {
                        n.is_element()
                            && matches!(n.tag_name().name(), "comment" | "threadedComment")
                    })
                    .count();
            }
        }
        if !has_notes {
            continue;
        }
        let vml: Vec<(String, String)> = rels
            .of_kind("vmlDrawing")
            .map(|r| (r.id.clone(), resolve_target(&sheet.part, &r.target)))
            .collect();
        for (rid, part) in vml {
            // Only a drawing that holds notes and nothing else (no form
            // controls) goes.
            let only_notes = package.part(&part).is_some_and(|b| {
                let t = String::from_utf8_lossy(b);
                !t.contains("ObjectType=\"Button\"")
                    && !t.contains("ObjectType=\"Checkbox\"")
                    && !t.contains("ObjectType=\"Drop\"")
                    && t.contains("ObjectType=\"Note\"")
            });
            if !only_notes {
                continue;
            }
            vml_gone.push(part.clone());
            // The sheet's <legacyDrawing r:id="…"/>.
            if let Some(b) = package.part(&sheet.part).map(<[u8]>::to_vec) {
                let text = xml::text(&sheet.part, &b)?;
                if let Some(start) = text
                    .find("legacyDrawing ")
                    .and_then(|i| text[..i].rfind('<'))
                {
                    let end = text[start..].find("/>").map(|e| start + e + 2);
                    if let Some(end) = end
                        && text[start..end].contains(&format!("\"{rid}\""))
                    {
                        let mut edits = Edits::new();
                        edits.delete(start..end);
                        package.set_part(&sheet.part, edits.apply(text)?.into_bytes());
                    }
                }
            }
        }
    }

    let mut removed_names: Vec<String> = package
        .part_names()
        .filter(|n| removable(n, &package))
        .map(str::to_owned)
        .collect();
    removed_names.extend(vml_gone);
    removed_names.sort();
    removed_names.dedup();
    let removed: BTreeSet<String> = removed_names
        .iter()
        .map(|n| n.to_ascii_lowercase())
        .collect();
    for name in &removed_names {
        package.remove_part(name);
    }
    let orphan_rels: Vec<String> = package
        .part_names()
        .filter(|n| n.ends_with(".rels") && removed.contains(&source_of(n).to_ascii_lowercase()))
        .map(str::to_owned)
        .collect();
    for name in &orphan_rels {
        package.remove_part(name);
    }
    removed_names.extend(orphan_rels);

    let mut removed_set = removed;
    only_ids(&mut package, ids, &mut removed_set, &mut removed_names)?;
    prune_relationships(&mut package, &removed_set)?;
    prune_content_types(&mut package, &removed_set)?;
    empty_properties(&mut package);
    // A pivot cache names who last refreshed it (decision 0029).
    for cache in &wb.pivots.caches {
        let Some(bytes) = package.part(&cache.part).map(<[u8]>::to_vec) else {
            continue;
        };
        let text = xml::text(&cache.part, &bytes)?;
        if let Some(range) = refreshed_by(text) {
            let mut edits = Edits::new();
            edits.replace(range, String::new());
            package.set_part(&cache.part, edits.apply(text)?.into_bytes());
        }
    }

    report.parts_removed = removed_names;
    let out = package.to_bytes()?;
    // The copy must still be a workbook that opens, with every relationship
    // pointing at a part.
    let check = Workbook::open(&out)
        .map_err(|e| Error::CannotEdit(format!("the clean copy doesn't reopen: {e}")))?;
    for name in check.package.part_names().filter(|n| n.ends_with(".rels")) {
        let rels = Relationships::parse(name, check.package.part(name).unwrap_or_default())?;
        let source = source_of(name);
        for r in rels.items.iter().filter(|r| !r.external) {
            let target = resolve_target(&source, &r.target);
            if !check.package.contains(&target) {
                return Err(Error::CannotEdit(format!(
                    "{name} points at a missing part"
                )));
            }
        }
    }
    Ok((out, report))
}

#[cfg(test)]
mod tests {
    use lineage_testkit::workbooks;

    use super::*;
    use crate::package::props::{custom_properties, read_ids};

    fn lineage_ids() -> LineageIds {
        LineageIds {
            family: format!("f{:031}", 1),
            checkpoint: format!("s{:031}", 2),
        }
    }

    #[test]
    fn notes_and_metadata_leave_hidden_sheets_stay() {
        let mut kit = workbooks::model_master();
        kit.custom = vec![("Owner".to_owned(), "Rowan Ellis".to_owned())];
        let (out, report) = clean_copy(&kit.to_bytes(), Some(&lineage_ids())).expect("cleans");
        let clean = Workbook::open(&out).expect("opens");
        assert_eq!(report.comments, 1);
        assert!(
            !clean
                .package
                .part_names()
                .any(|n| n.starts_with("xl/comments"))
        );
        assert!(!clean.package.part_names().any(|n| n.ends_with(".vml")));
        let sheet = String::from_utf8_lossy(
            clean
                .package
                .part("xl/worksheets/sheet1.xml")
                .expect("sheet"),
        );
        assert!(!sheet.contains("legacyDrawing"));
        assert_eq!(
            read_ids(&clean.package).expect("reads"),
            Some(lineage_ids())
        );
        assert_eq!(custom_properties(&clean.package).expect("reads").len(), 2);
        // The numbers read the same.
        crate::xlsx::apply::put_back(&Workbook::open(&kit.to_bytes()).expect("opens"), &clean)
            .expect("same cells");

        let (out, report) =
            clean_copy(&workbooks::awkward_cells().to_bytes(), None).expect("cleans");
        assert_eq!(report.hidden_sheets, 2, "kept, and counted");
        assert_eq!(Workbook::open(&out).expect("opens").sheets.len(), 3);
        let (_, report) =
            clean_copy(&workbooks::tracker_filtered().to_bytes(), None).expect("cleans");
        assert_eq!(report.hidden_rows, 2);
    }

    #[test]
    fn the_full_model_keeps_its_macros_queries_and_pivots() {
        let kit = lineage_testkit::advanced::model_full();
        let (out, report) = clean_copy(&kit.to_bytes(), None).expect("cleans");
        assert!(report.macros, "readable macros are counted too");
        let clean = Workbook::open(&out).expect("opens");
        assert!(clean.vba.is_some());
        assert_eq!(clean.queries.as_ref().map(|q| q.queries.len()), Some(2));
        assert_eq!(clean.pivots.tables.len(), 2);
        let cache = String::from_utf8_lossy(
            clean
                .package
                .part("xl/pivotCache/pivotCacheDefinition1.xml")
                .expect("cache"),
        )
        .into_owned();
        assert!(cache.contains(r#"refreshedBy="""#), "{cache}");
        // Nothing else about the pivots changed.
        let original = Workbook::open(&kit.to_bytes()).expect("opens");
        let d = super::super::diff::diff(&original, &clean, &[]);
        assert!(d.pivots.is_empty() && d.macros.is_empty() && d.queries.is_empty());
    }

    #[test]
    fn macros_go_with_the_copy() {
        let (out, report) = clean_copy(&workbooks::model_xlsm().to_bytes(), None).expect("cleans");
        assert!(report.macros);
        let clean = Workbook::open(&out).expect("opens");
        assert!(clean.macro_enabled);
        assert_eq!(
            clean.package.part("xl/vbaProject.bin"),
            Workbook::open(&workbooks::model_xlsm().to_bytes())
                .expect("opens")
                .package
                .part("xl/vbaProject.bin")
        );
    }
}
