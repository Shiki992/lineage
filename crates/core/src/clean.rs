// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Clean copies for sending (CLAUDE.md: "copies that leave are clean").
//!
//! A clean copy has every tracked change accepted, and no comments, revision
//! ids (rsids), hidden text, personal metadata, custom properties, thumbnail,
//! custom XML data, add-in task panes or building blocks. The only thing Lineage
//! leaves is the two ids, so the file can be recognised if it comes back, and the
//! person can turn even that off (decision 0007).

use std::collections::BTreeSet;

use roxmltree::Node;

use crate::docx::props::{
    CHECKPOINT_ID, FAMILY_ID, LineageIds, add_custom_part, custom_part, custom_xml,
};
use crate::docx::{Docx, View};
use crate::error::{Error, Result};
use crate::package::{ContentTypes, Package, resolve_target};
use crate::xml::{self, Edits, ns};

/// What cleaning removed, so the panel can say so.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CleanReport {
    /// Tracked changes accepted.
    pub revisions: usize,
    /// Comment ranges and references removed (the comments themselves go with
    /// their part).
    pub comments: usize,
    /// Runs of hidden text removed.
    pub hidden_runs: usize,
    /// Parts removed, such as `word/comments.xml`.
    pub parts_removed: Vec<String>,
}

const REVISION_PROPS: &[&str] = &[
    "rPrChange",
    "pPrChange",
    "sectPrChange",
    "tblPrChange",
    "tblGridChange",
    "trPrChange",
    "tcPrChange",
    "tblPrExChange",
    "numberingChange",
    "cellIns",
    "cellMerge",
];

const REVISION_RANGES: &[&str] = &[
    "moveFromRangeStart",
    "moveFromRangeEnd",
    "moveToRangeStart",
    "moveToRangeEnd",
    "customXmlInsRangeStart",
    "customXmlInsRangeEnd",
    "customXmlDelRangeStart",
    "customXmlDelRangeEnd",
    "customXmlMoveFromRangeStart",
    "customXmlMoveFromRangeEnd",
    "customXmlMoveToRangeStart",
    "customXmlMoveToRangeEnd",
];

fn is_w(node: Node<'_, '_>, name: &str) -> bool {
    xml::is(node, ns::W, name)
}

fn is_mark(node: Node<'_, '_>) -> bool {
    // ins/del/moveFrom/moveTo as a marker on a paragraph mark or a row.
    node.parent().is_some_and(|p| {
        is_w(p, "rPr") && p.parent().is_some_and(|pp| is_w(pp, "pPr")) || is_w(p, "trPr")
    })
}

fn vanished(run: Node<'_, '_>) -> bool {
    xml::child(run, ns::W, "rPr")
        .and_then(|rpr| xml::child(rpr, ns::W, "vanish"))
        .is_some_and(|v| !matches!(v.attribute((ns::W, "val")), Some("0" | "false" | "off")))
}

/// Removes rsid attributes (with the space before each) from an element's start tag.
fn strip_rsids(edits: &mut Edits, text: &str, node: Node<'_, '_>) {
    for attr in node.attributes() {
        if attr.namespace() == Some(ns::W) && attr.name().starts_with("rsid") {
            let range = attr.range();
            let start = text[..range.start].trim_end().len();
            edits.delete(start..range.end);
        }
    }
}

struct Pass<'r> {
    text: &'r str,
    edits: Edits,
    report: &'r mut CleanReport,
}

impl Pass<'_> {
    fn visit(&mut self, node: Node<'_, '_>) {
        if !node.is_element() {
            return;
        }
        if node.tag_name().namespace() == Some(ns::W) {
            let name = node.tag_name().name();
            match name {
                "del" | "moveFrom" if !is_mark(node) => {
                    self.report.revisions += 1;
                    self.edits.delete(node.range());
                    return;
                }
                "ins" | "moveTo" if !is_mark(node) => {
                    self.report.revisions += 1;
                    let start_end = xml::start_tag_end(self.text, node);
                    match xml::end_tag_start(self.text, node) {
                        Some(end_start) => {
                            self.edits.delete(node.range().start..start_end);
                            self.edits.delete(end_start..node.range().end);
                        }
                        None => self.edits.delete(node.range()),
                    }
                    for child in node.children() {
                        self.visit(child);
                    }
                    return;
                }
                "ins" | "moveTo" => {
                    // An inserted paragraph mark or row: accepted, the marker goes.
                    self.report.revisions += 1;
                    self.edits.delete(node.range());
                    return;
                }
                "del" | "moveFrom" if node.parent().is_some_and(|p| is_w(p, "trPr")) => {
                    // A deleted row: handled at the row.
                    return;
                }
                "del" | "moveFrom" => {
                    // A deleted paragraph mark: the join pass handles it.
                    return;
                }
                "tr" => {
                    let deleted = xml::child(node, ns::W, "trPr").is_some_and(|trpr| {
                        trpr.children()
                            .any(|c| is_w(c, "del") || is_w(c, "moveFrom"))
                    });
                    if deleted {
                        self.report.revisions += 1;
                        self.edits.delete(node.range());
                        return;
                    }
                }
                "commentRangeStart" | "commentRangeEnd" => {
                    self.report.comments += 1;
                    self.edits.delete(node.range());
                    return;
                }
                "r" => {
                    if node.children().any(|c| is_w(c, "commentReference")) {
                        self.report.comments += 1;
                        self.edits.delete(node.range());
                        return;
                    }
                    if vanished(node) {
                        self.report.hidden_runs += 1;
                        self.edits.delete(node.range());
                        return;
                    }
                }
                _ if REVISION_PROPS.contains(&name) || REVISION_RANGES.contains(&name) => {
                    self.report.revisions += 1;
                    self.edits.delete(node.range());
                    return;
                }
                "cellDel" => {
                    // Rare; there is no safe way to accept it here.
                    self.edits.delete(node.range());
                    return;
                }
                _ => {}
            }
        }
        strip_rsids(&mut self.edits, self.text, node);
        for child in node.children() {
            self.visit(child);
        }
    }
}

/// Joins paragraphs whose mark is a tracked deletion with the next paragraph,
/// as accepting the deletion does. One join per pass, repeated until none are left.
fn join_deleted_marks(part: &str, mut text: String, report: &mut CleanReport) -> Result<String> {
    loop {
        let doc = xml::parse(part, &text)?;
        let found = doc.descendants().find(|n| {
            is_w(*n, "p")
                && xml::child(*n, ns::W, "pPr")
                    .and_then(|ppr| xml::child(ppr, ns::W, "rPr"))
                    .is_some_and(|rpr| {
                        rpr.children()
                            .any(|c| is_w(c, "del") || is_w(c, "moveFrom"))
                    })
        });
        let Some(p) = found else {
            return Ok(text);
        };
        report.revisions += 1;
        let mut edits = Edits::new();
        let next = p.next_siblings().skip(1).find(Node::is_element);
        match next.filter(|n| is_w(*n, "p")) {
            Some(q) => {
                // <p>{P.pPr}{P content}</p><q>{Q.pPr}{Q content}</q>
                // becomes <q>{Q.pPr}{P content}{Q content}</q>.
                let p_content_start = xml::child(p, ns::W, "pPr")
                    .map_or_else(|| xml::start_tag_end(&text, p), |n| n.range().end);
                let p_content_end = xml::end_tag_start(&text, p).unwrap_or(p_content_start);
                let q_content_start = xml::child(q, ns::W, "pPr")
                    .map_or_else(|| xml::start_tag_end(&text, q), |n| n.range().end);
                let q_head = text[q.range().start..q_content_start].to_owned();
                edits.replace(p.range().start..p_content_start, q_head);
                edits.delete(p_content_end..q_content_start);
            }
            None => {
                // Nothing to join with: the deletion of the mark cannot be accepted
                // in place, so the marker alone goes.
                let rpr = xml::child(p, ns::W, "pPr").and_then(|ppr| xml::child(ppr, ns::W, "rPr"));
                if let Some(marker) = rpr.and_then(|r| {
                    r.children()
                        .find(|c| is_w(*c, "del") || is_w(*c, "moveFrom"))
                }) {
                    edits.delete(marker.range());
                }
            }
        }
        text = edits.apply(&text)?;
    }
}

/// Removes revision markup, comment markup, rsids and hidden runs from one part.
fn clean_part(part: &str, text: &str, report: &mut CleanReport) -> Result<String> {
    let doc = xml::parse(part, text)?;
    let mut pass = Pass {
        text,
        edits: Edits::new(),
        report,
    };
    pass.visit(doc.root_element());
    let edits = pass.edits;
    let once = edits.apply(text)?;
    join_deleted_marks(part, once, report)
}

fn removable(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "word/comments.xml"
            | "word/commentsextended.xml"
            | "word/commentsids.xml"
            | "word/commentsextensible.xml"
            | "word/people.xml"
    ) || lower.starts_with("docprops/thumbnail")
        || lower.starts_with("customxml/")
        || lower.starts_with("word/webextensions/")
        || lower.starts_with("word/glossary/")
}

/// Removes relationships to removed parts (and the settings' attached template).
fn prune_relationships(package: &mut Package, removed: &BTreeSet<String>) -> Result<()> {
    let rels_parts: Vec<String> = package
        .part_names()
        .filter(|n| n.ends_with(".rels"))
        .map(str::to_owned)
        .collect();
    for rels_part in rels_parts {
        if removed.contains(&rels_part.to_ascii_lowercase()) {
            continue;
        }
        let source = source_of(&rels_part);
        let bytes = package.part(&rels_part).unwrap_or_default().to_vec();
        let text = xml::text(&rels_part, &bytes)?;
        let doc = xml::parse(&rels_part, text)?;
        let mut edits = Edits::new();
        for rel in doc
            .root_element()
            .children()
            .filter(|n| xml::is(*n, ns::PACKAGE_RELS, "Relationship"))
        {
            let target = rel.attribute("Target").unwrap_or_default();
            let external = rel.attribute("TargetMode") == Some("External");
            let kind = rel.attribute("Type").unwrap_or_default();
            let resolved = resolve_target(&source, target).to_ascii_lowercase();
            if (!external && removed.contains(&resolved)) || kind.ends_with("/attachedTemplate") {
                edits.delete(rel.range());
            }
        }
        if !edits.is_empty() {
            let new = edits.apply(text)?;
            package.set_part(&rels_part, new.into_bytes());
        }
    }
    Ok(())
}

/// The part a `.rels` part belongs to: `word/_rels/document.xml.rels` → `word/document.xml`.
fn source_of(rels_part: &str) -> String {
    let (dir, file) = rels_part.rsplit_once('/').unwrap_or(("", rels_part));
    let dir = dir.trim_end_matches("_rels").trim_end_matches('/');
    let file = file.trim_end_matches(".rels");
    if dir.is_empty() {
        file.to_owned()
    } else {
        format!("{dir}/{file}")
    }
}

fn prune_content_types(package: &mut Package, removed: &BTreeSet<String>) -> Result<()> {
    let bytes = package
        .part(ContentTypes::PART)
        .ok_or_else(|| Error::NotAWordDocument("no content types".to_owned()))?
        .to_vec();
    let text = xml::text(ContentTypes::PART, &bytes)?;
    let doc = xml::parse(ContentTypes::PART, text)?;
    let mut edits = Edits::new();
    for over in doc
        .root_element()
        .children()
        .filter(|n| xml::is(*n, ns::CONTENT_TYPES, "Override"))
    {
        let name = over
            .attribute("PartName")
            .unwrap_or_default()
            .trim_start_matches('/')
            .to_ascii_lowercase();
        if removed.contains(&name) {
            edits.delete(over.range());
        }
    }
    if !edits.is_empty() {
        let new = edits.apply(text)?;
        package.set_part(ContentTypes::PART, new.into_bytes());
    }
    Ok(())
}

/// Empties the core and extended properties: author, editors, company, template,
/// editing time and the title all go.
fn empty_properties(package: &mut Package) {
    if package.contains("docProps/core.xml") {
        package.set_part(
            "docProps/core.xml",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<cp:coreProperties xmlns:cp=\"{}\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\" xmlns:dcterms=\"http://purl.org/dc/terms/\" xmlns:dcmitype=\"http://purl.org/dc/dcmitype/\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"/>",
                ns::CORE_PROPS
            )
            .into_bytes(),
        );
    }
    if package.contains("docProps/app.xml") {
        package.set_part(
            "docProps/app.xml",
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Properties xmlns=\"{}\" xmlns:vt=\"{}\"/>",
                ns::EXTENDED_PROPS,
                ns::VT
            )
            .into_bytes(),
        );
    }
}

/// Removes document variables, rsids and the attached template from settings.
fn clean_settings(package: &mut Package, settings: &str) -> Result<()> {
    let Some(bytes) = package.part(settings).map(<[u8]>::to_vec) else {
        return Ok(());
    };
    let text = xml::text(settings, &bytes)?;
    let doc = xml::parse(settings, text)?;
    let mut edits = Edits::new();
    for child in doc.root_element().children().filter(Node::is_element) {
        if ["docVars", "rsids", "attachedTemplate"]
            .iter()
            .any(|n| is_w(child, n))
        {
            edits.delete(child.range());
        }
    }
    if !edits.is_empty() {
        let new = edits.apply(text)?;
        package.set_part(settings, new.into_bytes());
    }
    Ok(())
}

/// Makes a clean copy for sending. With `ids`, the copy carries the two Lineage
/// ids as its only custom properties; without, it carries none.
///
/// # Errors
/// [`Error`] when the file cannot be read, or a part cannot be cleaned safely.
pub fn clean_copy(bytes: &[u8], ids: Option<&LineageIds>) -> Result<(Vec<u8>, CleanReport)> {
    let docx = Docx::open_view(bytes, View::Accepted)?;
    let mut package = docx.package.clone();
    let mut report = CleanReport::default();

    let mut stories: Vec<&str> = docx.stories.iter().map(|s| s.part.as_str()).collect();
    stories.sort_unstable();
    stories.dedup();
    for part in stories {
        let text = docx.part_text(part)?;
        let cleaned = clean_part(part, text, &mut report)?;
        package.set_part(part, cleaned.into_bytes());
    }

    let removed: BTreeSet<String> = package
        .part_names()
        .filter(|n| removable(n))
        .map(str::to_ascii_lowercase)
        .collect();
    let mut removed_names: Vec<String> = package
        .part_names()
        .filter(|n| removable(n))
        .map(str::to_owned)
        .collect();
    for name in &removed_names {
        package.remove_part(name);
    }
    // Relationship parts of removed parts go too.
    let orphan_rels: Vec<String> = package
        .part_names()
        .filter(|n| n.ends_with(".rels") && removed.contains(&source_of(n).to_ascii_lowercase()))
        .map(str::to_owned)
        .collect();
    for name in &orphan_rels {
        package.remove_part(name);
    }
    removed_names.extend(orphan_rels);

    // Custom properties: only the two ids, or nothing.
    let custom = custom_part(&package);
    let mut removed_set = removed.clone();
    match (ids, &custom) {
        (Some(ids), Some(part)) => package.set_part(
            part,
            custom_xml(&[(FAMILY_ID, &ids.family), (CHECKPOINT_ID, &ids.checkpoint)]).into_bytes(),
        ),
        (Some(ids), None) => {
            add_custom_part(
                &mut package,
                &custom_xml(&[(FAMILY_ID, &ids.family), (CHECKPOINT_ID, &ids.checkpoint)]),
            )?;
        }
        (None, Some(part)) => {
            package.remove_part(part);
            removed_set.insert(part.to_ascii_lowercase());
            removed_names.push(part.clone());
        }
        (None, None) => {}
    }
    prune_relationships(&mut package, &removed_set)?;
    prune_content_types(&mut package, &removed_set)?;
    empty_properties(&mut package);
    if let Some(rel) = docx.main_rels.of_kind("settings").next() {
        clean_settings(&mut package, &resolve_target(&docx.main_part, &rel.target))?;
    }

    report.parts_removed = removed_names;
    let out = package.to_bytes()?;
    // The copy must still be a Word document the engine can read.
    Docx::open(&out)?;
    Ok((out, report))
}

#[cfg(test)]
mod tests;
