// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Reading slides, shapes and notes into the model, and the canonical keys.

use std::collections::BTreeMap;

use roxmltree::Node;

use super::model::{LayoutRef, Notes, Para, Placeholder, Shape, ShapeKind, Slide, Xfrm};
use crate::error::{Error, Result};
use crate::hash::Fnv;
use crate::package::{Package, Relationships, rels_part_for, resolve_target};
use crate::xml::{self, ns};

/// Attributes PowerPoint rewrites without anyone changing the text: spell-check
/// and grammar marks, the language it guessed, and smart-tag bookkeeping.
const NOISE_ATTRIBUTES: [&str; 8] = [
    "lang", "altLang", "dirty", "err", "noProof", "smtClean", "smtId", "bmk",
];

/// Elements left out of every key: the mark that holds an empty paragraph end's
/// formatting, which PowerPoint adds and drops as people type.
fn skipped_element(node: Node<'_, '_>) -> bool {
    xml::is(node, ns::A, "endParaRPr")
}

/// Text that counts: inside text-bearing elements, whitespace is content.
fn text_bearing(node: Node<'_, '_>) -> bool {
    matches!(
        node.tag_name().name(),
        "t" | "v" | "f" | "formatCode" | "text"
    )
}

/// Relationships of a part, or none.
pub(crate) fn read_rels(package: &Package, part: &str) -> Result<Relationships> {
    let rels_part = rels_part_for(part);
    match package.part(&rels_part) {
        Some(bytes) => Relationships::parse(&rels_part, bytes),
        None => Ok(Relationships::default()),
    }
}

/// Resolves relationship ids inside one part to what they point at, so keys
/// match across decks that number relationships differently.
pub(crate) struct Canon<'a> {
    package: &'a Package,
    part: &'a str,
    rels: &'a Relationships,
}

impl<'a> Canon<'a> {
    pub(crate) fn new(package: &'a Package, part: &'a str, rels: &'a Relationships) -> Self {
        Self {
            package,
            part,
            rels,
        }
    }

    /// The identity of what a relationship id points at: an external address, or
    /// a part's bytes together with the bytes of the parts it points at (a chart
    /// and its data workbook, a picture).
    pub(crate) fn rel_identity(&self, id: &str) -> u64 {
        let Some(rel) = self.rels.by_id(id) else {
            return Fnv::new().str("missing").str(id).finish();
        };
        if rel.external {
            return Fnv::new().str("external").str(&rel.target).finish();
        }
        let target = resolve_target(self.part, &rel.target);
        part_identity(self.package, &target)
    }

    /// Feeds a canonical form of `node` into `h`. With `skip_xfrm`, position and
    /// size are left out.
    pub(crate) fn hash(&self, node: Node<'_, '_>, h: &mut Fnv, skip_xfrm: bool) {
        if node.is_text() {
            let text = node.text().unwrap_or_default();
            let keep = node
                .parent()
                .is_some_and(|p| p.is_element() && text_bearing(p));
            if keep || !text.trim().is_empty() {
                h.str("#").str(text);
            }
            return;
        }
        if !node.is_element() || skipped_element(node) {
            return;
        }
        if skip_xfrm && (xml::is(node, ns::A, "xfrm") || xml::is(node, ns::P, "xfrm")) {
            return;
        }
        let tag = node.tag_name();
        h.str("<")
            .str(tag.namespace().unwrap_or(""))
            .str(tag.name());
        let is_run_props =
            tag.namespace() == Some(ns::A) && matches!(tag.name(), "rPr" | "defRPr" | "endParaRPr");
        let is_shape_props = xml::is(node, ns::P, "cNvPr");
        let mut attributes: Vec<(String, String)> = Vec::new();
        for attribute in node.attributes() {
            let name = attribute.name();
            let namespace = attribute.namespace().unwrap_or("");
            if namespace.is_empty() && is_run_props && NOISE_ATTRIBUTES.contains(&name) {
                continue;
            }
            if namespace.is_empty() && is_shape_props && name == "id" {
                continue;
            }
            let value = if namespace == ns::R {
                format!("{:016x}", self.rel_identity(attribute.value()))
            } else {
                attribute.value().to_owned()
            };
            attributes.push((format!("{namespace}|{name}"), value));
        }
        attributes.sort();
        for (name, value) in attributes {
            h.str(&name).str(&value);
        }
        for child in node.children() {
            self.hash(child, h, skip_xfrm);
        }
        h.str(">");
    }

    /// The part a relationship id points at, when it is inside the package.
    pub(crate) fn target_part(&self, id: &str) -> Option<String> {
        let rel = self.rels.by_id(id)?;
        if rel.external {
            return None;
        }
        Some(resolve_target(self.part, &rel.target))
    }

    pub(crate) fn key(&self, node: Node<'_, '_>, skip_xfrm: bool) -> u64 {
        let mut h = Fnv::new();
        self.hash(node, &mut h, skip_xfrm);
        h.finish()
    }
}

/// A part's bytes and the bytes of the parts it points at, one level down.
pub(crate) fn part_identity(package: &Package, part: &str) -> u64 {
    let mut h = Fnv::new();
    h.str("part");
    match package.part(part) {
        Some(bytes) => {
            h.bytes(bytes);
        }
        None => {
            h.str("missing");
        }
    }
    if let Ok(rels) = read_rels(package, part) {
        let mut targets: Vec<(String, u64)> = rels
            .items
            .iter()
            .map(|rel| {
                let identity = if rel.external {
                    Fnv::new().str(&rel.target).finish()
                } else {
                    let target = resolve_target(part, &rel.target);
                    package
                        .part(&target)
                        .map_or(0, |b| Fnv::new().bytes(b).finish())
                };
                (rel.rel_type.clone(), identity)
            })
            .collect();
        targets.sort();
        for (kind, identity) in targets {
            h.str(&kind).u64(identity);
        }
    }
    h.finish()
}

fn attr_i64(node: Option<Node<'_, '_>>, name: &str) -> i64 {
    node.and_then(|n| n.attribute(name))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
}

fn read_xfrm(xfrm: Node<'_, '_>) -> Xfrm {
    let off = xml::child(xfrm, ns::A, "off");
    let ext = xml::child(xfrm, ns::A, "ext");
    Xfrm {
        x: attr_i64(off, "x"),
        y: attr_i64(off, "y"),
        cx: attr_i64(ext, "cx"),
        cy: attr_i64(ext, "cy"),
    }
}

/// The paragraphs of a `p:txBody` / `a:txBody`.
pub(crate) fn paragraphs(body: Node<'_, '_>) -> Vec<Para> {
    body.children()
        .filter(|n| xml::is(*n, ns::A, "p"))
        .map(|p| {
            let level = xml::child(p, ns::A, "pPr")
                .and_then(|ppr| ppr.attribute("lvl"))
                .and_then(|l| l.parse::<u8>().ok())
                .unwrap_or(0)
                .min(8);
            let mut text = String::new();
            for child in p.children() {
                if xml::is(child, ns::A, "r") || xml::is(child, ns::A, "fld") {
                    if let Some(t) = xml::child(child, ns::A, "t") {
                        text.push_str(t.text().unwrap_or_default());
                    }
                } else if xml::is(child, ns::A, "br") {
                    text.push('\u{b}');
                }
            }
            Para { level, text }
        })
        .collect()
}

fn text_key(paras: &[Para]) -> u64 {
    let mut h = Fnv::new();
    for p in paras {
        h.u64(u64::from(p.level)).str(&p.text);
    }
    h.finish()
}

/// The non-visual properties (`p:cNvPr`) of a shape element, wherever its kind
/// keeps them.
fn c_nv_pr<'a, 'i>(node: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    node.descendants().find(|n| xml::is(*n, ns::P, "cNvPr"))
}

fn nv_pr<'a, 'i>(node: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    node.children()
        .find(|n| n.is_element() && n.tag_name().name().starts_with("nv"))
        .and_then(|nv| xml::child(nv, ns::P, "nvPr"))
}

fn placeholder(node: Node<'_, '_>) -> Option<Placeholder> {
    let ph = xml::child(nv_pr(node)?, ns::P, "ph")?;
    Some(Placeholder {
        kind: ph.attribute("type").map(str::to_owned),
        idx: ph.attribute("idx").and_then(|i| i.parse().ok()),
    })
}

/// Reads one child of a shape tree into a [`Shape`]. Returns `None` for the
/// tree's own properties.
pub(crate) fn shape(node: Node<'_, '_>, canon: &Canon<'_>) -> Option<Shape> {
    if !node.is_element() {
        return None;
    }
    let tag = node.tag_name();
    if tag.namespace() == Some(ns::P) && matches!(tag.name(), "nvGrpSpPr" | "grpSpPr" | "extLst") {
        return None;
    }
    let props = c_nv_pr(node);
    let id = props
        .and_then(|p| p.attribute("id"))
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let name = props
        .and_then(|p| p.attribute("name"))
        .unwrap_or_default()
        .to_owned();
    let mut members = Vec::new();
    let mut paras = Vec::new();
    let mut geometry = None;
    let mut object = Fnv::new();
    let kind = if xml::is(node, ns::P, "sp") {
        if let Some(xfrm) =
            xml::child(node, ns::P, "spPr").and_then(|s| xml::child(s, ns::A, "xfrm"))
        {
            geometry = Some(read_xfrm(xfrm));
        }
        if let Some(body) = xml::child(node, ns::P, "txBody") {
            paras = paragraphs(body);
        }
        let has_text = paras.iter().any(|p| !p.text.is_empty());
        if placeholder(node).is_some() || has_text {
            ShapeKind::Text
        } else {
            if let Some(sp_pr) = xml::child(node, ns::P, "spPr") {
                for geom in sp_pr
                    .children()
                    .filter(|c| xml::is(*c, ns::A, "prstGeom") || xml::is(*c, ns::A, "custGeom"))
                {
                    canon.hash(geom, &mut object, false);
                }
            }
            ShapeKind::Drawing
        }
    } else if xml::is(node, ns::P, "cxnSp") {
        if let Some(xfrm) =
            xml::child(node, ns::P, "spPr").and_then(|s| xml::child(s, ns::A, "xfrm"))
        {
            geometry = Some(read_xfrm(xfrm));
        }
        object.str("connector");
        ShapeKind::Drawing
    } else if xml::is(node, ns::P, "pic") {
        if let Some(xfrm) =
            xml::child(node, ns::P, "spPr").and_then(|s| xml::child(s, ns::A, "xfrm"))
        {
            geometry = Some(read_xfrm(xfrm));
        }
        if let Some(fill) = xml::child(node, ns::P, "blipFill") {
            canon.hash(fill, &mut object, false);
        }
        let media = nv_pr(node).is_some_and(|nv| {
            nv.descendants().any(|n| {
                n.is_element()
                    && matches!(
                        n.tag_name().name(),
                        "videoFile" | "audioFile" | "quickTimeFile" | "media"
                    )
            })
        });
        if media {
            if let Some(nv) = nv_pr(node) {
                canon.hash(nv, &mut object, false);
            }
            ShapeKind::Media
        } else {
            ShapeKind::Picture
        }
    } else if xml::is(node, ns::P, "graphicFrame") {
        if let Some(xfrm) = xml::child(node, ns::P, "xfrm") {
            geometry = Some(read_xfrm(xfrm));
        }
        let data =
            xml::child(node, ns::A, "graphic").and_then(|g| xml::child(g, ns::A, "graphicData"));
        if let Some(data) = data {
            canon.hash(data, &mut object, false);
        }
        let uri = data.and_then(|d| d.attribute("uri")).unwrap_or_default();
        if uri.ends_with("/table") {
            if let Some(tbl) = data.and_then(|d| xml::child(d, ns::A, "tbl")) {
                for row in tbl.children().filter(|r| xml::is(*r, ns::A, "tr")) {
                    let cells: Vec<String> = row
                        .children()
                        .filter(|c| xml::is(*c, ns::A, "tc"))
                        .map(|c| {
                            xml::child(c, ns::A, "txBody")
                                .map(|b| {
                                    paragraphs(b)
                                        .into_iter()
                                        .map(|p| p.text)
                                        .collect::<Vec<_>>()
                                        .join(" ")
                                })
                                .unwrap_or_default()
                        })
                        .collect();
                    paras.push(Para {
                        level: 0,
                        text: cells.join(" · "),
                    });
                }
            }
            ShapeKind::Table
        } else if uri.ends_with("/chart") {
            let chart = data
                .and_then(|d| d.children().find(|c| xml::is(*c, ns::C, "chart")))
                .and_then(|c| c.attribute((ns::R, "id")))
                .and_then(|id| canon.target_part(id));
            if let Some(chart) = chart {
                paras = chart_figures(canon.package, &chart);
            }
            ShapeKind::Chart
        } else if uri.ends_with("/diagram") {
            ShapeKind::Diagram
        } else if uri.ends_with("/ole") {
            ShapeKind::Ole
        } else {
            ShapeKind::Other
        }
    } else if xml::is(node, ns::P, "grpSp") {
        if let Some(xfrm) =
            xml::child(node, ns::P, "grpSpPr").and_then(|s| xml::child(s, ns::A, "xfrm"))
        {
            geometry = Some(read_xfrm(xfrm));
        }
        for child in node.children() {
            if let Some(member) = shape(child, canon) {
                members.push(member.id);
                members.extend(&member.members);
                object.u64(member.content);
                paras.extend(member.paras);
            }
        }
        ShapeKind::Group
    } else {
        canon.hash(node, &mut object, true);
        if let Some(body) = node.descendants().find(|n| xml::is(*n, ns::P, "txBody")) {
            paras = paragraphs(body);
        }
        ShapeKind::Other
    };
    let content = Fnv::new()
        .str(&format!("{kind:?}"))
        .u64(text_key(&paras))
        .u64(object.finish())
        .finish();
    Some(Shape {
        id,
        name,
        kind,
        placeholder: placeholder(node),
        geometry,
        paras,
        content,
        full: canon.key(node, false),
        span: node.range(),
        members,
    })
}

fn cached_values(node: Option<Node<'_, '_>>) -> Vec<(usize, String)> {
    let Some(node) = node else {
        return Vec::new();
    };
    let mut out: Vec<(usize, String)> = node
        .descendants()
        .filter(|n| xml::is(*n, ns::C, "pt"))
        .filter_map(|pt| {
            let idx = pt.attribute("idx")?.parse().ok()?;
            let value = xml::child(pt, ns::C, "v")?.text().unwrap_or_default();
            Some((idx, value.to_owned()))
        })
        .collect();
    out.sort_by_key(|(idx, _)| *idx);
    out
}

/// A chart's figures, from the values cached in the chart part: one line per
/// point, "Q4 2026: 1200", with the series name first when there are several.
/// What a person reads off the chart, so "1200 changed to 2000" can be shown.
fn chart_figures(package: &Package, part: &str) -> Vec<Para> {
    let Some(bytes) = package.part(part) else {
        return Vec::new();
    };
    let Ok(text) = xml::text(part, bytes) else {
        return Vec::new();
    };
    let Ok(doc) = xml::parse(part, text) else {
        return Vec::new();
    };
    let series: Vec<Node<'_, '_>> = doc
        .descendants()
        .filter(|n| xml::is(*n, ns::C, "ser"))
        .collect();
    let mut out = Vec::new();
    for ser in &series {
        let name = xml::child(*ser, ns::C, "tx")
            .and_then(|tx| tx.descendants().find(|n| xml::is(*n, ns::C, "v")))
            .and_then(|v| v.text())
            .unwrap_or_default();
        let categories = cached_values(
            ser.children()
                .find(|c| xml::is(*c, ns::C, "cat") || xml::is(*c, ns::C, "xVal")),
        );
        let values = cached_values(
            ser.children()
                .find(|c| xml::is(*c, ns::C, "val") || xml::is(*c, ns::C, "yVal")),
        );
        for (idx, value) in values {
            let category = categories
                .iter()
                .find(|(i, _)| *i == idx)
                .map_or_else(|| format!("Point {}", idx + 1), |(_, c)| c.clone());
            let text = if series.len() > 1 && !name.is_empty() {
                format!("{name}, {category}: {value}")
            } else {
                format!("{category}: {value}")
            };
            out.push(Para { level: 0, text });
        }
    }
    out
}

/// Layouts already read, by part.
pub(crate) type LayoutCache = BTreeMap<String, LayoutRef>;

fn layout(package: &Package, part: &str, cache: &mut LayoutCache) -> Result<LayoutRef> {
    if let Some(found) = cache.get(part) {
        return Ok(found.clone());
    }
    let bytes = package
        .part(part)
        .ok_or_else(|| Error::Damaged(format!("{part} is missing")))?;
    let text = xml::text(part, bytes)?;
    let doc = xml::parse(part, text)?;
    let root = doc.root_element();
    let name = xml::child(root, ns::P, "cSld")
        .and_then(|c| c.attribute("name"))
        .unwrap_or_default()
        .to_owned();
    let layout = LayoutRef {
        part: part.to_owned(),
        name,
        kind: root.attribute("type").unwrap_or("cust").to_owned(),
    };
    cache.insert(part.to_owned(), layout.clone());
    Ok(layout)
}

fn notes(package: &Package, part: &str) -> Result<Option<Notes>> {
    let Some(bytes) = package.part(part) else {
        return Ok(None);
    };
    let text = xml::text(part, bytes)?;
    let doc = xml::parse(part, text)?;
    let rels = read_rels(package, part)?;
    let canon = Canon::new(package, part, &rels);
    let body = doc.descendants().find(|n| {
        xml::is(*n, ns::P, "sp")
            && placeholder(*n).is_some_and(|ph| ph.kind.as_deref() == Some("body"))
    });
    let tx_body = body.and_then(|b| xml::child(b, ns::P, "txBody"));
    let paras = tx_body.map(paragraphs).unwrap_or_default();
    let key = tx_body.map_or(0, |b| canon.key(b, false));
    Ok(Some(Notes {
        part: part.to_owned(),
        paras,
        key,
        body_span: tx_body.map(|b| b.range()),
    }))
}

/// Reads one slide part.
pub(crate) fn slide(
    package: &Package,
    part: &str,
    rel_id: &str,
    sld_id: u32,
    layouts: &mut LayoutCache,
) -> Result<Slide> {
    let bytes = package
        .part(part)
        .ok_or_else(|| Error::Damaged(format!("{part} is missing")))?;
    let text = xml::text(part, bytes)?;
    let doc = xml::parse(part, text)?;
    let root = doc.root_element();
    if !xml::is(root, ns::P, "sld") {
        return Err(Error::NotADeck(format!("{part} is not a slide")));
    }
    let rels = read_rels(package, part)?;
    let canon = Canon::new(package, part, &rels);
    let c_sld = xml::child(root, ns::P, "cSld")
        .ok_or_else(|| Error::NotADeck(format!("{part} has no slide content")))?;
    let tree = xml::child(c_sld, ns::P, "spTree")
        .ok_or_else(|| Error::NotADeck(format!("{part} has no shape tree")))?;
    let shapes: Vec<Shape> = tree.children().filter_map(|n| shape(n, &canon)).collect();
    let creation_id = c_sld
        .descendants()
        .find(|n| xml::is(*n, ns::P14, "creationId"))
        .and_then(|n| n.attribute("val"))
        .and_then(|v| v.parse().ok());
    let hidden = root
        .attribute("show")
        .is_some_and(|v| v == "0" || v == "false");

    let layout = match rels.of_kind("slideLayout").next() {
        Some(rel) => Some(layout(
            package,
            &resolve_target(part, &rel.target),
            layouts,
        )?),
        None => None,
    };
    let notes = match rels.of_kind("notesSlide").next() {
        Some(rel) => notes(package, &resolve_target(part, &rel.target))?,
        None => None,
    };

    let mut settings = Fnv::new();
    settings.str(if hidden { "hidden" } else { "shown" });
    if let Some(layout) = &layout {
        settings.str(&layout.name).str(&layout.kind);
    }
    settings.str(c_sld.attribute("name").unwrap_or_default());
    if let Some(bg) = xml::child(c_sld, ns::P, "bg") {
        canon.hash(bg, &mut settings, false);
    }
    let mut timing = 0;
    let mut animated = Vec::new();
    for child in root.children().filter(Node::is_element) {
        if xml::is(child, ns::P, "cSld") {
            continue;
        }
        if xml::is(child, ns::P, "timing") {
            timing = canon.key(child, false);
            for target in child.descendants().filter(|n| xml::is(*n, ns::P, "spTgt")) {
                if let Some(id) = target.attribute("spid").and_then(|v| v.parse().ok())
                    && !animated.contains(&id)
                {
                    animated.push(id);
                }
            }
            continue;
        }
        canon.hash(child, &mut settings, false);
    }
    let settings = settings.finish();
    let mut full = Fnv::new();
    full.u64(settings).u64(timing);
    for s in &shapes {
        full.u64(s.full);
    }
    full.u64(notes.as_ref().map_or(0, |n| n.key));
    let tree_end = xml::end_tag_start(text, tree).unwrap_or(tree.range().end);
    Ok(Slide {
        part: part.to_owned(),
        rel_id: rel_id.to_owned(),
        sld_id,
        creation_id,
        hidden,
        layout,
        shapes,
        notes,
        settings,
        timing,
        animated,
        full: full.finish(),
        tree_span: tree.range(),
        tree_end,
    })
}
