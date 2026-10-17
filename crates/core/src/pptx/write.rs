// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Writes decided changes into a deck.
//!
//! PowerPoint has no tracked changes, so brought-in changes are plain edits; the
//! version's checkpoint before them is the way back (decision 0018). Like the
//! Word writer, it never re-serialises XML it didn't change: a shape is replaced
//! by splicing the master's shape bytes over the version's, with relationship
//! ids and shape ids renumbered for this deck. Pictures, charts (with their data
//! workbooks) and notes are copied as parts, reusing any the version already has
//! with the same bytes. The slide order and sections are rebuilt once.

use std::collections::{BTreeMap, BTreeSet};

use roxmltree::Node;

use super::Deck;
use super::diff::{Change, Gap, Loc, Unit};
use super::merge::{Item, Plan};
use super::read::{part_identity, read_rels};
use crate::diff::ChangeKind;
use crate::error::{Error, Result};
use crate::merge::{Decision, OverlapKind, Status};
use crate::package::edit::{
    NewRelationship, add_default, add_override, add_relationships_with_ids, free_part_name,
    remove_override, remove_relationships,
};
use crate::package::{
    ContentTypes, Package, Relationships, part_dir, relative_target, resolve_target,
};
use crate::xml::{self, Edits, ns};

fn cannot(detail: &str) -> Error {
    Error::CannotEdit(detail.to_owned())
}

const REL_SLIDE: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide";
const REL_NOTES_SLIDE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide";

/// One entry of the slide order.
#[derive(Clone, Debug)]
struct Entry {
    id: u32,
    xml: String,
}

/// Relationships to add to a part that already exists, with ids chosen here.
#[derive(Default)]
struct Pending {
    existing: Relationships,
    added: Vec<(String, NewRelationship)>,
}

impl Pending {
    fn id_for(&mut self, rel: NewRelationship) -> String {
        if let Some(found) = self.existing.items.iter().find(|r| {
            r.rel_type == rel.rel_type && r.target == rel.target && r.external == rel.external
        }) {
            return found.id.clone();
        }
        if let Some((id, _)) = self.added.iter().find(|(_, r)| *r == rel) {
            return id.clone();
        }
        let mut n = self.existing.items.len() + self.added.len() + 1;
        loop {
            let id = format!("rId{n}");
            if self.existing.by_id(&id).is_none() && !self.added.iter().any(|(i, _)| *i == id) {
                self.added.push((id.clone(), rel));
                return id;
            }
            n += 1;
        }
    }
}

struct Writer<'p, 'd> {
    plan: &'p Plan<'d>,
    package: Package,
    edits: BTreeMap<String, Edits>,
    pending: BTreeMap<String, Pending>,
    /// Theirs part → the part in ours that holds the same thing.
    copied: BTreeMap<String, String>,
    /// Shape ids in use, per slide part of ours.
    shape_ids: BTreeMap<String, BTreeSet<u32>>,
    order: Vec<Entry>,
    /// Where the next slide inserted after an anchor goes (anchor id → last id).
    inserted_after: BTreeMap<Option<u32>, u32>,
    removed_parts: BTreeSet<String>,
    removed_rel_ids: Vec<String>,
    sld_prefix: String,
    used_sld_ids: BTreeSet<u32>,
    /// Parts nothing pointed at before the write; never removed by it.
    orphans_before: BTreeSet<String>,
}

/// The `sldId` entries of a presentation, in order.
fn slide_entries(text: &str, doc: &roxmltree::Document<'_>) -> Vec<Entry> {
    doc.root_element()
        .children()
        .find(|n| xml::is(*n, ns::P, "sldIdLst"))
        .map(|list| {
            list.children()
                .filter(|n| xml::is(*n, ns::P, "sldId"))
                .filter_map(|n| {
                    Some(Entry {
                        id: n.attribute("id")?.parse().ok()?,
                        xml: text[n.range()].to_owned(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn find_node<'a, 'i>(
    doc: &'a roxmltree::Document<'i>,
    span: &std::ops::Range<usize>,
) -> Option<Node<'a, 'i>> {
    doc.descendants()
        .find(|n| n.is_element() && n.range() == *span)
}

impl<'p, 'd> Writer<'p, 'd> {
    fn new(plan: &'p Plan<'d>) -> Result<Self> {
        let ours = plan.ours;
        let text = ours.part_text(&ours.main_part)?;
        let doc = xml::parse(&ours.main_part, text)?;
        let order = slide_entries(text, &doc);
        let sld_prefix = doc
            .root_element()
            .children()
            .find(|n| xml::is(*n, ns::P, "sldIdLst"))
            .map_or_else(
                || xml::prefix_of(text, doc.root_element()).to_owned(),
                |n| xml::prefix_of(text, n).to_owned(),
            );
        let used_sld_ids = order.iter().map(|e| e.id).collect();
        Ok(Self {
            plan,
            package: ours.package.clone(),
            edits: BTreeMap::new(),
            pending: BTreeMap::new(),
            copied: BTreeMap::new(),
            shape_ids: BTreeMap::new(),
            order,
            inserted_after: BTreeMap::new(),
            removed_parts: BTreeSet::new(),
            removed_rel_ids: Vec::new(),
            sld_prefix,
            used_sld_ids,
            orphans_before: orphans(&ours.package)?
                .into_iter()
                .map(|n| n.to_ascii_lowercase())
                .collect(),
        })
    }

    fn ours(&self) -> &'d Deck {
        self.plan.ours
    }

    fn theirs(&self) -> &'d Deck {
        self.plan.theirs
    }

    fn edits(&mut self, part: &str) -> &mut Edits {
        self.edits.entry(part.to_owned()).or_default()
    }

    fn pending(&mut self, part: &str) -> Result<&mut Pending> {
        if !self.pending.contains_key(part) {
            let existing = read_rels(&self.package, part)?;
            self.pending.insert(
                part.to_owned(),
                Pending {
                    existing,
                    added: Vec::new(),
                },
            );
        }
        Ok(self.pending.get_mut(part).expect("just inserted"))
    }

    // ----- Where things are in ours -------------------------------------------------

    /// Ours' slide for a base slide, if it still has one.
    fn ours_slide(&self, base: usize) -> Option<usize> {
        self.plan.ours_diff.after_of(base)
    }

    /// Ours' slide for a place in theirs: via the base.
    fn ours_slide_of_theirs(&self, theirs: usize) -> Option<usize> {
        let base = self.plan.theirs_diff.before_of(theirs)?;
        self.ours_slide(base)
    }

    /// Ours' shape for a base shape on a base slide.
    fn ours_shape(&self, base_slide: usize, base_shape: usize) -> Option<(usize, usize)> {
        let slide = self.ours_slide(base_slide)?;
        let shape = self
            .plan
            .ours_diff
            .shapes_of(base_slide)
            .iter()
            .find(|p| p.0 == base_shape)?
            .1;
        Some((slide, shape))
    }

    /// The slide id in ours that a gap after base slide `after` maps to: the
    /// nearest base slide at or before it that ours still has.
    fn anchor(&self, after: Option<usize>) -> Option<u32> {
        let mut at = after?;
        loop {
            if let Some(ours) = self.ours_slide(at) {
                return Some(self.ours().slides[ours].sld_id);
            }
            at = at.checked_sub(1)?;
        }
    }

    // ----- Parts ---------------------------------------------------------------------

    /// The part in ours holding what `theirs_part` holds, copying it (and what it
    /// points at) when ours has nothing with the same bytes.
    fn copy_part(&mut self, theirs_part: &str) -> Result<String> {
        if let Some(found) = self.copied.get(theirs_part) {
            return Ok(found.clone());
        }
        let theirs = self.theirs();
        let bytes = theirs
            .package
            .part(theirs_part)
            .ok_or_else(|| cannot("a part the master points at is missing"))?
            .to_vec();
        let identity = part_identity(&theirs.package, theirs_part);
        let dir = part_dir(theirs_part).to_ascii_lowercase();
        // Pictures and other media can be shared between slides. A chart, its
        // workbook or a notes slide belongs to one slide, so it is always copied.
        let shareable = dir == "ppt/media";
        let same = self
            .package
            .part_names()
            .filter(|n| {
                part_dir(n).eq_ignore_ascii_case(&dir)
                    && !self.removed_parts.contains(&n.to_ascii_lowercase())
            })
            .find(|n| shareable && part_identity(&self.package, n) == identity)
            .map(str::to_owned);
        if let Some(same) = same {
            self.copied.insert(theirs_part.to_owned(), same.clone());
            return Ok(same);
        }
        let name = new_name(&self.package, theirs_part);
        self.copied.insert(theirs_part.to_owned(), name.clone());
        self.package.set_part(&name, bytes);
        self.copy_type(theirs_part, &name)?;
        let rels = read_rels(&theirs.package, theirs_part)?;
        let mut new_rels = Vec::new();
        for rel in &rels.items {
            let target = if rel.external {
                rel.target.clone()
            } else {
                let target = resolve_target(theirs_part, &rel.target);
                let ours_target = self.copy_part(&target)?;
                relative_target(&name, &ours_target)
            };
            new_rels.push((
                rel.id.clone(),
                NewRelationship {
                    rel_type: rel.rel_type.clone(),
                    target,
                    external: rel.external,
                },
            ));
        }
        add_relationships_with_ids(&mut self.package, &name, &new_rels)?;
        Ok(name)
    }

    /// Gives `ours_part` the content type `theirs_part` has.
    fn copy_type(&mut self, theirs_part: &str, ours_part: &str) -> Result<()> {
        let types = ContentTypes::parse(
            self.theirs()
                .package
                .part(ContentTypes::PART)
                .ok_or_else(|| cannot("the master has no content types"))?,
        )?;
        if types.has_override(theirs_part) {
            let ct = types.of(theirs_part).unwrap_or_default().to_owned();
            add_override(&mut self.package, ours_part, &ct)
        } else {
            let ours_types = ContentTypes::parse(
                self.package
                    .part(ContentTypes::PART)
                    .ok_or_else(|| cannot("this version has no content types"))?,
            )?;
            if ours_types.of(ours_part).is_some() {
                return Ok(());
            }
            let ct = types
                .of(theirs_part)
                .ok_or_else(|| cannot("a part has no content type"))?
                .to_owned();
            let ext = ours_part.rsplit_once('.').map_or("", |(_, e)| e).to_owned();
            add_default(&mut self.package, &ext, &ct)
        }
    }

    /// Ours' relationship id, from `ours_part`, for what theirs' `rid` (from
    /// `theirs_part`) points at.
    fn map_rel(&mut self, theirs_part: &str, rid: &str, ours_part: &str) -> Result<String> {
        let rels = read_rels(&self.theirs().package, theirs_part)?;
        let rel = rels
            .by_id(rid)
            .ok_or_else(|| cannot("a relationship the master uses is missing"))?
            .clone();
        let target = if rel.external {
            rel.target.clone()
        } else {
            let target = resolve_target(theirs_part, &rel.target);
            let ours_target = if rel.rel_type == REL_SLIDE {
                // A link to another slide of the deck.
                let theirs_index = self
                    .theirs()
                    .slides
                    .iter()
                    .position(|s| s.part.eq_ignore_ascii_case(&target))
                    .ok_or_else(|| cannot("a link to a slide that isn't there"))?;
                let ours_index = self
                    .ours_slide_of_theirs(theirs_index)
                    .ok_or_else(|| cannot("a link to a slide this version doesn't have"))?;
                self.ours().slides[ours_index].part.clone()
            } else {
                self.copy_part(&target)?
            };
            relative_target(ours_part, &ours_target)
        };
        Ok(self.pending(ours_part)?.id_for(NewRelationship {
            rel_type: rel.rel_type,
            target,
            external: rel.external,
        }))
    }

    /// Theirs' element at `span` in `theirs_part`, made ready to go into
    /// `ours_part`: relationship ids mapped, shape ids made unique (the outermost
    /// one set to `top_id` when given), and any namespace prefix ours doesn't
    /// declare declared on the element itself.
    fn fragment(
        &mut self,
        theirs_part: &str,
        span: &std::ops::Range<usize>,
        ours_part: &str,
        top_id: Option<u32>,
        keep_ids: &BTreeSet<u32>,
    ) -> Result<String> {
        let theirs_text = self.theirs().part_text(theirs_part)?.to_owned();
        let doc = xml::parse(theirs_part, &theirs_text)?;
        let node = find_node(&doc, span).ok_or_else(|| cannot("a shape moved while reading"))?;
        let ours_text = self.ours().part_text(ours_part)?.to_owned();
        let ours_doc = xml::parse(ours_part, &ours_text)?;
        let ours_root = ours_doc.root_element();

        let mut edits = Edits::new();
        let base = span.start;
        let mut used: BTreeSet<u32> = self
            .shape_ids
            .get(ours_part)
            .cloned()
            .unwrap_or_else(|| shape_ids_in(&ours_text));
        for id in keep_ids {
            used.remove(id);
        }
        let mut first = true;
        let mut declare: BTreeMap<String, String> = BTreeMap::new();
        let fragment_text = &theirs_text[span.clone()];
        for n in node.descendants().filter(Node::is_element) {
            // Namespaces: every prefix the fragment uses must mean the same in ours.
            let prefix = xml::prefix_of(&theirs_text, n);
            if let Some(uri) = n.tag_name().namespace() {
                check_prefix(prefix, uri, ours_root, fragment_text, &mut declare)?;
            }
            for attribute in n.attributes() {
                let Some(uri) = attribute.namespace() else {
                    continue;
                };
                let qname = &theirs_text[attribute.range_qname()];
                let prefix = qname.split_once(':').map_or("", |(p, _)| p);
                if prefix == "xmlns" || qname == "xmlns" {
                    continue;
                }
                check_prefix(prefix, uri, ours_root, fragment_text, &mut declare)?;
                if uri == ns::R {
                    let new = self.map_rel(theirs_part, attribute.value(), ours_part)?;
                    let range = attribute.range_value();
                    edits.replace(range.start - base..range.end - base, xml::escape_attr(&new));
                }
            }
            if xml::is(n, ns::P, "cNvPr")
                && let Some(attribute) = n
                    .attributes()
                    .find(|a| a.name() == "id" && a.namespace().is_none())
            {
                let theirs_id: u32 = attribute.value().parse().unwrap_or(0);
                let id = match top_id {
                    Some(top) if first => top,
                    _ if theirs_id != 0 && !used.contains(&theirs_id) => theirs_id,
                    _ => used.iter().next_back().map_or(2, |m| m + 1).max(2),
                };
                used.insert(id);
                first = false;
                let range = attribute.range_value();
                edits.replace(range.start - base..range.end - base, id.to_string());
            }
        }
        self.shape_ids.insert(ours_part.to_owned(), used);
        let mut out = edits.apply(fragment_text)?;
        if !declare.is_empty() {
            let name_end = 1 + out[1..]
                .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
                .unwrap_or(out.len() - 1);
            let decls: String = declare
                .iter()
                .map(|(p, uri)| format!(r#" xmlns:{p}="{}""#, xml::escape_attr(uri)))
                .collect();
            out.insert_str(name_end, &decls);
        }
        Ok(out)
    }

    // ----- Shapes, notes, settings ---------------------------------------------------

    fn replace_shape(&mut self, ours_loc: (usize, usize), theirs_loc: Loc) -> Result<()> {
        let ours_slide = &self.ours().slides[ours_loc.0];
        let ours_shape = &ours_slide.shapes[ours_loc.1];
        let theirs_slide = &self.theirs().slides[theirs_loc.slide];
        let theirs_shape =
            &theirs_slide.shapes[theirs_loc.shape.ok_or_else(|| cannot("no shape"))?];
        let mut keep: BTreeSet<u32> = ours_shape.members.iter().copied().collect();
        keep.insert(ours_shape.id);
        let fragment = self.fragment(
            &theirs_slide.part,
            &theirs_shape.span,
            &ours_slide.part,
            Some(ours_shape.id).filter(|id| *id != 0),
            &keep,
        )?;
        let (part, span) = (ours_slide.part.clone(), ours_shape.span.clone());
        self.edits(&part).replace(span, fragment);
        Ok(())
    }

    fn insert_shape(
        &mut self,
        ours_slide: usize,
        after: Option<usize>,
        theirs_loc: Loc,
    ) -> Result<()> {
        let slide = &self.ours().slides[ours_slide];
        let theirs_slide = &self.theirs().slides[theirs_loc.slide];
        let theirs_shape =
            &theirs_slide.shapes[theirs_loc.shape.ok_or_else(|| cannot("no shape"))?];
        let fragment = self.fragment(
            &theirs_slide.part,
            &theirs_shape.span,
            &slide.part,
            None,
            &BTreeSet::new(),
        )?;
        let at = match after {
            Some(index) => slide.shapes[index].span.end,
            None => {
                // After the tree's own properties.
                let text = self.ours().part_text(&slide.part)?;
                let doc = xml::parse(&slide.part, text)?;
                let tree =
                    find_node(&doc, &slide.tree_span).ok_or_else(|| cannot("no shape tree"))?;
                tree.children()
                    .filter(|n| xml::is(*n, ns::P, "nvGrpSpPr") || xml::is(*n, ns::P, "grpSpPr"))
                    .map(|n| n.range().end)
                    .max()
                    .unwrap_or(slide.tree_end)
            }
        };
        let part = slide.part.clone();
        self.edits(&part).insert(at, fragment);
        Ok(())
    }

    fn delete_shape(&mut self, ours_loc: (usize, usize)) {
        let slide = &self.ours().slides[ours_loc.0];
        let shape = &slide.shapes[ours_loc.1];
        let (part, span) = (slide.part.clone(), shape.span.clone());
        self.edits(&part).delete(span);
    }

    fn set_notes(&mut self, ours_slide: usize, theirs_slide: usize) -> Result<()> {
        let ours = &self.ours().slides[ours_slide];
        let theirs = &self.theirs().slides[theirs_slide];
        match (&ours.notes, &theirs.notes) {
            (Some(mine), Some(master)) => {
                let (Some(mine_span), Some(master_span)) = (&mine.body_span, &master.body_span)
                else {
                    return Err(cannot("speaker notes without a notes body"));
                };
                let fragment = self.fragment(
                    &master.part,
                    master_span,
                    &mine.part,
                    None,
                    &BTreeSet::new(),
                )?;
                let part = mine.part.clone();
                self.edits(&part).replace(mine_span.clone(), fragment);
            }
            (Some(mine), None) => {
                let Some(span) = &mine.body_span else {
                    return Ok(());
                };
                let text = self.ours().part_text(&mine.part)?;
                let doc = xml::parse(&mine.part, text)?;
                let body = find_node(&doc, span).ok_or_else(|| cannot("no notes body"))?;
                let p = xml::prefix_of(text, body);
                let a = body
                    .descendants()
                    .find(|n| n.tag_name().namespace() == Some(ns::A))
                    .map_or("a", |n| xml::prefix_of(text, n));
                let empty = format!(
                    "<{p}:txBody><{a}:bodyPr/><{a}:lstStyle/><{a}:p><{a}:endParaRPr lang=\"en-US\"/></{a}:p></{p}:txBody>"
                );
                let part = mine.part.clone();
                self.edits(&part).replace(span.clone(), empty);
            }
            (None, Some(master)) => {
                let notes = self.copy_notes(&master.part, &ours.part)?;
                let target = relative_target(&ours.part, &notes);
                let part = ours.part.clone();
                self.pending(&part)?.id_for(NewRelationship {
                    rel_type: REL_NOTES_SLIDE.to_owned(),
                    target,
                    external: false,
                });
            }
            (None, None) => {}
        }
        Ok(())
    }

    /// Copies theirs' notes slide as a new part for ours' slide `slide_part`.
    fn copy_notes(&mut self, theirs_notes: &str, slide_part: &str) -> Result<String> {
        let notes_master = self
            .ours()
            .notes_master
            .clone()
            .ok_or_else(|| cannot("this version has no notes master"))?;
        let bytes = self
            .theirs()
            .package
            .part(theirs_notes)
            .ok_or_else(|| cannot("the master's notes are missing"))?
            .to_vec();
        let name = free_part_name(&self.package, |n| {
            format!("ppt/notesSlides/notesSlide{n}.xml")
        });
        self.package.set_part(&name, bytes);
        add_override(&mut self.package, &name, ns::CT_NOTES_SLIDE)?;
        let rels = read_rels(&self.theirs().package, theirs_notes)?;
        let mut new_rels = Vec::new();
        for rel in &rels.items {
            let kind = rel.rel_type.rsplit_once('/').map_or("", |(_, k)| k);
            let target = match kind {
                _ if rel.external => rel.target.clone(),
                "notesMaster" => relative_target(&name, &notes_master),
                "slide" => relative_target(&name, slide_part),
                _ => {
                    let target = resolve_target(theirs_notes, &rel.target);
                    let ours_target = self.copy_part(&target)?;
                    relative_target(&name, &ours_target)
                }
            };
            new_rels.push((
                rel.id.clone(),
                NewRelationship {
                    rel_type: rel.rel_type.clone(),
                    target,
                    external: rel.external,
                },
            ));
        }
        add_relationships_with_ids(&mut self.package, &name, &new_rels)?;
        Ok(name)
    }

    fn set_settings(
        &mut self,
        ours_slide: usize,
        theirs_slide: usize,
        change: &Change,
    ) -> Result<()> {
        let ours = self.ours().slides[ours_slide].clone();
        let theirs = self.theirs().slides[theirs_slide].clone();
        let settings = change.settings.clone().unwrap_or_default();
        let ours_text = self.ours().part_text(&ours.part)?.to_owned();
        let ours_doc = xml::parse(&ours.part, &ours_text)?;
        let root = ours_doc.root_element();

        if let Some(hidden) = settings.hidden {
            let existing = root
                .attributes()
                .find(|a| a.name() == "show" && a.namespace().is_none());
            match (hidden, existing) {
                (true, Some(attribute)) => {
                    let range = attribute.range_value();
                    self.edits(&ours.part).replace(range, "0");
                }
                (true, None) => {
                    let at = xml::start_tag_end(&ours_text, root)
                        - if xml::is_self_closing(&ours_text, root) {
                            2
                        } else {
                            1
                        };
                    self.edits(&ours.part).insert(at, r#" show="0""#);
                }
                (false, Some(attribute)) => {
                    // The attribute and the space before it.
                    let range = attribute.range();
                    let start = ours_text[..range.start].trim_end().len();
                    self.edits(&ours.part).delete(start..range.end);
                }
                (false, None) => {}
            }
        }
        if settings.layout.is_some()
            && let Some(layout) = theirs.layout.as_ref()
        {
            let target = self
                .ours()
                .layout_like(layout)
                .ok_or_else(|| cannot("this version doesn't have the layout"))?
                .part
                .clone();
            self.set_layout(&ours.part, &target)?;
        }
        if settings.other {
            let theirs_text = self.theirs().part_text(&theirs.part)?.to_owned();
            let theirs_doc = xml::parse(&theirs.part, &theirs_text)?;
            let theirs_root = theirs_doc.root_element();
            // The background, first in p:cSld.
            let theirs_bg = background(theirs_root).map(|n| n.range());
            let new_bg = match theirs_bg {
                Some(span) => {
                    self.fragment(&theirs.part, &span, &ours.part, None, &BTreeSet::new())?
                }
                None => String::new(),
            };
            match background(root) {
                Some(n) => self.edits(&ours.part).replace(n.range(), new_bg),
                None if !new_bg.is_empty() => {
                    let c_sld = xml::child(root, ns::P, "cSld")
                        .ok_or_else(|| cannot("no slide content"))?;
                    let at = xml::start_tag_end(&ours_text, c_sld);
                    self.edits(&ours.part).insert(at, new_bg);
                }
                None => {}
            }
            // The colour mapping and transition, between p:cSld and the animations.
            let between = |root: Node<'_, '_>| -> Vec<std::ops::Range<usize>> {
                root.children()
                    .filter(Node::is_element)
                    .filter(|n| {
                        !xml::is(*n, ns::P, "cSld")
                            && !xml::is(*n, ns::P, "timing")
                            && !xml::is(*n, ns::P, "extLst")
                    })
                    .map(|n| n.range())
                    .collect()
            };
            let mut replacement = String::new();
            for span in between(theirs_root) {
                replacement.push_str(&self.fragment(
                    &theirs.part,
                    &span,
                    &ours.part,
                    None,
                    &BTreeSet::new(),
                )?);
            }
            let mine = between(root);
            let c_sld_end = xml::child(root, ns::P, "cSld").map_or(0, |n| n.range().end);
            match (mine.first(), mine.last()) {
                (Some(first), Some(last)) => {
                    self.edits(&ours.part)
                        .replace(first.start..last.end, replacement);
                }
                _ => self.edits(&ours.part).insert(c_sld_end, replacement),
            }
        }
        Ok(())
    }

    /// Points ours' slide at another of its layouts.
    fn set_layout(&mut self, slide_part: &str, layout_part: &str) -> Result<()> {
        let rels_part = crate::package::rels_part_for(slide_part);
        let text = self.ours().part_text(&rels_part)?.to_owned();
        let doc = xml::parse(&rels_part, &text)?;
        let rel = doc
            .root_element()
            .children()
            .filter(|n| xml::is(*n, ns::PACKAGE_RELS, "Relationship"))
            .find(|n| {
                n.attribute("Type")
                    .is_some_and(|t| t.ends_with("/slideLayout"))
            })
            .ok_or_else(|| cannot("the slide has no layout"))?;
        let target = rel
            .attributes()
            .find(|a| a.name() == "Target")
            .ok_or_else(|| cannot("the layout has no target"))?;
        let new = relative_target(slide_part, layout_part);
        self.edits(&rels_part)
            .replace(target.range_value(), xml::escape_attr(&new));
        Ok(())
    }

    // ----- Slides ----------------------------------------------------------------------

    fn place(&mut self, entry: Entry, anchor: Option<u32>) {
        let after = self.inserted_after.get(&anchor).copied().or(anchor);
        let at = match after {
            Some(id) => self
                .order
                .iter()
                .position(|e| e.id == id)
                .map_or(self.order.len(), |i| i + 1),
            None => 0,
        };
        self.inserted_after.insert(anchor, entry.id);
        self.order.insert(at, entry);
    }

    fn new_sld_id(&mut self, wanted: u32) -> u32 {
        let id = if (256..2_147_483_648).contains(&wanted) && !self.used_sld_ids.contains(&wanted) {
            wanted
        } else {
            self.used_sld_ids
                .iter()
                .next_back()
                .map_or(256, |m| m + 1)
                .max(256)
        };
        self.used_sld_ids.insert(id);
        id
    }

    /// Copies theirs' slide `index` into ours, placed after `anchor` (an ours slide id).
    fn insert_slide(&mut self, index: usize, anchor: Option<u32>) -> Result<()> {
        let theirs = self.theirs().slides[index].clone();
        let layout = theirs
            .layout
            .as_ref()
            .map(|l| {
                self.ours()
                    .layout_like(l)
                    .map(|found| found.part.clone())
                    .ok_or_else(|| cannot("this version doesn't have the layout"))
            })
            .transpose()?;
        let bytes = self
            .theirs()
            .package
            .part(&theirs.part)
            .ok_or_else(|| cannot("the master's slide is missing"))?
            .to_vec();
        let name = free_part_name(&self.package, |n| format!("ppt/slides/slide{n}.xml"));
        self.package.set_part(&name, bytes);
        self.copied.insert(theirs.part.clone(), name.clone());
        add_override(&mut self.package, &name, ns::CT_SLIDE)?;
        let rels = read_rels(&self.theirs().package, &theirs.part)?;
        let mut new_rels = Vec::new();
        for rel in &rels.items {
            let kind = rel.rel_type.rsplit_once('/').map_or("", |(_, k)| k);
            let target = match kind {
                _ if rel.external => rel.target.clone(),
                "slideLayout" => {
                    relative_target(&name, layout.as_deref().ok_or_else(|| cannot("no layout"))?)
                }
                "notesSlide" => {
                    let target = resolve_target(&theirs.part, &rel.target);
                    let notes = self.copy_notes(&target, &name)?;
                    relative_target(&name, &notes)
                }
                // Comments never travel with a slide.
                "comments" => continue,
                _ if rel.rel_type.contains("/2018/10/") => continue,
                "slide" => {
                    let target = resolve_target(&theirs.part, &rel.target);
                    let theirs_index = self
                        .theirs()
                        .slides
                        .iter()
                        .position(|s| s.part.eq_ignore_ascii_case(&target));
                    let ours_part = theirs_index
                        .and_then(|i| self.ours_slide_of_theirs(i))
                        .map_or_else(|| name.clone(), |i| self.ours().slides[i].part.clone());
                    relative_target(&name, &ours_part)
                }
                _ => {
                    let target = resolve_target(&theirs.part, &rel.target);
                    let ours_target = self.copy_part(&target)?;
                    relative_target(&name, &ours_target)
                }
            };
            new_rels.push((
                rel.id.clone(),
                NewRelationship {
                    rel_type: rel.rel_type.clone(),
                    target,
                    external: rel.external,
                },
            ));
        }
        add_relationships_with_ids(&mut self.package, &name, &new_rels)?;
        let main = self.ours().main_part.clone();
        let rid = self.pending(&main)?.id_for(NewRelationship {
            rel_type: REL_SLIDE.to_owned(),
            target: relative_target(&main, &name),
            external: false,
        });
        let id = self.new_sld_id(theirs.sld_id);
        let p = self.sld_prefix.clone();
        let r = self.r_prefix()?;
        let element = if p.is_empty() {
            "sldId".to_owned()
        } else {
            format!("{p}:sldId")
        };
        self.place(
            Entry {
                id,
                xml: format!(r#"<{element} id="{id}" {r}:id="{rid}"/>"#),
            },
            anchor,
        );
        Ok(())
    }

    /// The prefix ours' presentation uses for the relationships namespace.
    fn r_prefix(&self) -> Result<String> {
        let text = self.ours().part_text(&self.ours().main_part)?;
        let doc = xml::parse(&self.ours().main_part, text)?;
        Ok(doc
            .root_element()
            .lookup_prefix(ns::R)
            .unwrap_or("r")
            .to_owned())
    }

    fn delete_slide(&mut self, ours_index: usize) -> Result<()> {
        let slide = self.ours().slides[ours_index].clone();
        self.order.retain(|e| e.id != slide.sld_id);
        self.removed_rel_ids.push(slide.rel_id.clone());
        self.remove_part_tree(&slide.part);
        if let Some(notes) = &slide.notes {
            self.remove_part_tree(&notes.part);
        }
        // Comments on the slide go with it.
        let rels = read_rels(&self.package, &slide.part)?;
        for rel in rels.of_kind("comments") {
            self.remove_part_tree(&resolve_target(&slide.part, &rel.target));
        }
        Ok(())
    }

    fn remove_part_tree(&mut self, part: &str) {
        self.removed_parts.insert(part.to_ascii_lowercase());
        self.removed_parts
            .insert(crate::package::rels_part_for(part).to_ascii_lowercase());
    }

    fn move_slide(&mut self, ours_index: usize, anchor: Option<u32>) {
        let id = self.ours().slides[ours_index].sld_id;
        if let Some(at) = self.order.iter().position(|e| e.id == id) {
            let entry = self.order.remove(at);
            self.place(entry, anchor);
        }
    }

    // ----- One decision ------------------------------------------------------------------

    fn bring(&mut self, item: &Item, decision: Decision) -> Result<()> {
        let change = &item.change;
        let base = change.before;
        match (change.unit, change.kind) {
            (Unit::Slide, ChangeKind::Insert) => {
                let theirs = change.after.ok_or_else(|| cannot("no slide"))?.slide;
                let gap = match change.gap {
                    Some(Gap::Slide(after)) => after,
                    _ => None,
                };
                let mut anchor = self.anchor(gap);
                if let (Status::Overlap(OverlapKind::BothAdded), Some(mine)) =
                    (&item.status, &item.mine)
                {
                    let mine_index = mine.after.ok_or_else(|| cannot("no slide"))?.slide;
                    let mine_id = self.ours().slides[mine_index].sld_id;
                    match decision {
                        Decision::KeepBoth {
                            masters_first: false,
                        } => anchor = Some(mine_id),
                        Decision::KeepBoth {
                            masters_first: true,
                        } => {}
                        Decision::UseMasters => self.delete_slide(mine_index)?,
                        _ => return Err(cannot("both added a slide here: choose first")),
                    }
                }
                self.insert_slide(theirs, anchor)
            }
            (Unit::Slide, ChangeKind::Delete) => {
                let ours = self
                    .ours_slide(base.ok_or_else(|| cannot("no slide"))?.slide)
                    .ok_or_else(|| cannot("this version doesn't have the slide"))?;
                self.delete_slide(ours)
            }
            (Unit::Slide, _) => {
                let ours = self
                    .ours_slide(base.ok_or_else(|| cannot("no slide"))?.slide)
                    .ok_or_else(|| cannot("this version doesn't have the slide"))?;
                let gap = match change.gap {
                    Some(Gap::Slide(after)) => after,
                    _ => None,
                };
                let anchor = self.anchor(gap);
                self.move_slide(ours, anchor);
                Ok(())
            }
            (Unit::Shape, ChangeKind::Insert) => {
                let Some(Gap::Shape { slide, after }) = change.gap else {
                    return Err(cannot("no place for the shape"));
                };
                let ours_slide = self
                    .ours_slide(slide)
                    .ok_or_else(|| cannot("this version doesn't have the slide"))?;
                let after = after.and_then(|a| {
                    // The nearest earlier base shape ours still has.
                    (0..=a)
                        .rev()
                        .find_map(|k| self.ours_shape(slide, k).map(|(_, s)| s))
                });
                self.insert_shape(
                    ours_slide,
                    after,
                    change.after.ok_or_else(|| cannot("no shape"))?,
                )
            }
            (Unit::Shape, ChangeKind::Delete) => {
                let b = base.ok_or_else(|| cannot("no shape"))?;
                let ours = self
                    .ours_shape(b.slide, b.shape.ok_or_else(|| cannot("no shape"))?)
                    .ok_or_else(|| cannot("this version doesn't have the shape"))?;
                self.delete_shape(ours);
                Ok(())
            }
            (Unit::Shape, _) => {
                let b = base.ok_or_else(|| cannot("no shape"))?;
                let ours = self
                    .ours_shape(b.slide, b.shape.ok_or_else(|| cannot("no shape"))?)
                    .ok_or_else(|| cannot("this version doesn't have the shape"))?;
                self.replace_shape(ours, change.after.ok_or_else(|| cannot("no shape"))?)
            }
            (Unit::Notes, _) => {
                let b = base.ok_or_else(|| cannot("no slide"))?;
                let ours = self.ours_slide(b.slide).ok_or_else(|| cannot("no slide"))?;
                self.set_notes(ours, change.after.ok_or_else(|| cannot("no slide"))?.slide)
            }
            (Unit::Settings, _) => {
                let b = base.ok_or_else(|| cannot("no slide"))?;
                let ours = self.ours_slide(b.slide).ok_or_else(|| cannot("no slide"))?;
                self.set_settings(
                    ours,
                    change.after.ok_or_else(|| cannot("no slide"))?.slide,
                    change,
                )
            }
            (Unit::Animations | Unit::Design, _) => {
                Err(cannot("this change is brought in by hand"))
            }
        }
    }

    // ----- Finishing ---------------------------------------------------------------------

    fn finish(mut self) -> Result<Vec<u8>> {
        let ours = self.plan.ours;
        // The slide order and sections, rebuilt once.
        let main = ours.main_part.clone();
        let text = ours.part_text(&main)?.to_owned();
        let doc = xml::parse(&main, &text)?;
        let root = doc.root_element();
        let original: Vec<u32> = slide_entries(&text, &doc).iter().map(|e| e.id).collect();
        let final_ids: Vec<u32> = self.order.iter().map(|e| e.id).collect();
        if original != final_ids || !self.order.iter().all(|e| original.contains(&e.id)) {
            let list = root.children().find(|n| xml::is(*n, ns::P, "sldIdLst"));
            let content: String = self.order.iter().map(|e| e.xml.as_str()).collect();
            match list {
                Some(list) if !xml::is_self_closing(&text, list) => {
                    let start = xml::start_tag_end(&text, list);
                    let end = xml::end_tag_start(&text, list).unwrap_or(start);
                    self.edits(&main).replace(start..end, content);
                }
                Some(list) => {
                    let mut edits = Edits::new();
                    xml::append_child(&mut edits, &text, list, &content);
                    // Expanding <p:sldIdLst/>: one edit over the whole element.
                    let expanded = edits.apply(&text[..list.range().end])?;
                    self.edits(&main)
                        .replace(list.range(), expanded[list.range().start..].to_owned());
                }
                None => return Err(cannot("the deck has no slide list")),
            }
            self.rebuild_sections(&text, root, &final_ids)?;
        }
        // Custom shows that list a removed slide.
        if !self.removed_rel_ids.is_empty() {
            for show in root.descendants().filter(|n| xml::is(*n, ns::P, "sld")) {
                if show
                    .attribute((ns::R, "id"))
                    .is_some_and(|id| self.removed_rel_ids.iter().any(|r| r == id))
                {
                    let range = show.range();
                    self.edits(&main).delete(range);
                }
            }
        }

        // Edited parts: applied, then re-parsed as the last guard.
        let edits = std::mem::take(&mut self.edits);
        let edited: Vec<String> = edits.keys().cloned().collect();
        for (part, edits) in edits {
            if self.removed_parts.contains(&part.to_ascii_lowercase()) {
                continue;
            }
            let original = ours.part_text(&part)?;
            let new = edits.apply(original)?;
            xml::parse(&part, &new).map_err(|_| cannot("the result would not be a valid deck"))?;
            self.package.set_part(&part, new.into_bytes());
        }
        // New relationships of existing parts.
        let pending = std::mem::take(&mut self.pending);
        for (part, pending) in pending {
            if !self.removed_parts.contains(&part.to_ascii_lowercase()) {
                add_relationships_with_ids(&mut self.package, &part, &pending.added)?;
            }
        }
        if !self.removed_rel_ids.is_empty() {
            remove_relationships(&mut self.package, &main, &self.removed_rel_ids)?;
        }
        // A replaced picture or chart leaves its old relationship unused.
        for part in edited
            .iter()
            .filter(|p| !p.ends_with(".rels") && *p != &main)
        {
            self.drop_unused_rels(part)?;
        }
        // Removed slides, their notes, and what only they pointed at.
        self.remove_parts()?;
        self.package.to_bytes()
    }

    fn rebuild_sections(&mut self, text: &str, root: Node<'_, '_>, order: &[u32]) -> Result<()> {
        let sections: Vec<Node<'_, '_>> = root
            .descendants()
            .filter(|n| xml::is(*n, ns::P14, "section"))
            .collect();
        if sections.is_empty() {
            return Ok(());
        }
        let member: Vec<Vec<u32>> = sections
            .iter()
            .map(|s| {
                s.descendants()
                    .filter(|n| xml::is(*n, ns::P14, "sldId"))
                    .filter_map(|n| n.attribute("id").and_then(|v| v.parse().ok()))
                    .collect()
            })
            .collect();
        let mut lists: Vec<Vec<u32>> = vec![Vec::new(); sections.len()];
        let mut current = 0;
        for id in order {
            if let Some(found) = member.iter().position(|m| m.contains(id)) {
                current = found;
            }
            // A new slide joins the section of the slide before it.
            lists[current].push(*id);
        }
        let main = self.plan.ours.main_part.clone();
        for (section, ids) in sections.iter().zip(&lists) {
            let Some(list) = section
                .children()
                .find(|n| xml::is(*n, ns::P14, "sldIdLst"))
            else {
                continue;
            };
            let prefix = xml::prefix_of(text, list).to_owned();
            let element = if prefix.is_empty() {
                "sldId".to_owned()
            } else {
                format!("{prefix}:sldId")
            };
            let content: String = ids
                .iter()
                .map(|id| format!(r#"<{element} id="{id}"/>"#))
                .collect();
            let name = xml::qname(text, list).to_owned();
            self.edits(&main)
                .replace(list.range(), format!("<{name}>{content}</{name}>"));
        }
        Ok(())
    }

    /// Removes relationships of `part` to content that nothing in it uses any
    /// more: pictures, charts, links and embedded objects.
    fn drop_unused_rels(&mut self, part: &str) -> Result<()> {
        if self.removed_parts.contains(&part.to_ascii_lowercase()) {
            return Ok(());
        }
        let Some(bytes) = self.package.part(part) else {
            return Ok(());
        };
        let text = xml::text(part, bytes)?.to_owned();
        let doc = xml::parse(part, &text)?;
        let used: BTreeSet<&str> = doc
            .descendants()
            .flat_map(|n| n.attributes())
            .filter(|a| a.namespace() == Some(ns::R))
            .map(|a| a.value())
            .collect();
        let rels = read_rels(&self.package, part)?;
        const CONTENT: [&str; 13] = [
            "chart",
            "image",
            "hyperlink",
            "oleObject",
            "package",
            "video",
            "audio",
            "media",
            "diagramData",
            "diagramLayout",
            "diagramQuickStyle",
            "diagramColors",
            "diagramDrawing",
        ];
        let unused: Vec<String> = rels
            .items
            .iter()
            .filter(|r| {
                let kind = r.rel_type.rsplit_once('/').map_or("", |(_, k)| k);
                CONTENT.contains(&kind) && !used.contains(r.id.as_str())
            })
            .map(|r| r.id.clone())
            .collect();
        if !unused.is_empty() {
            remove_relationships(&mut self.package, part, &unused)?;
        }
        Ok(())
    }

    /// Removes the parts of removed slides, then any media, chart, workbook,
    /// notes, comment or tag part nothing points at any more.
    fn remove_parts(&mut self) -> Result<()> {
        loop {
            let names: Vec<String> = self.package.part_names().map(str::to_owned).collect();
            for name in &names {
                if self.removed_parts.contains(&name.to_ascii_lowercase()) {
                    self.package.remove_part(name);
                    remove_override(&mut self.package, name)?;
                }
            }
            // Parts nothing points at that something pointed at before.
            let orphans: Vec<String> = orphans(&self.package)?
                .into_iter()
                .filter(|n| !self.orphans_before.contains(&n.to_ascii_lowercase()))
                .collect();
            if orphans.is_empty() {
                break;
            }
            for orphan in orphans {
                self.remove_part_tree(&orphan);
            }
        }
        // Relationships still pointing at removed parts (a notes slide's link back).
        let removed = self.removed_parts.clone();
        crate::clean::prune_relationships(&mut self.package, &removed)?;
        Ok(())
    }
}

/// Slide content parts (media, charts, workbooks, notes, comments, tags,
/// SmartArt, slides) that no relationship points at.
fn orphans(package: &Package) -> Result<Vec<String>> {
    let mut referenced: BTreeSet<String> = BTreeSet::new();
    for name in package.part_names().filter(|n| n.ends_with(".rels")) {
        let source = crate::clean::source_of(name);
        let rels = Relationships::parse(name, package.part(name).unwrap_or_default())?;
        for rel in rels.items.iter().filter(|r| !r.external) {
            referenced.insert(resolve_target(&source, &rel.target).to_ascii_lowercase());
        }
    }
    Ok(package
        .part_names()
        .filter(|n| {
            let lower = n.to_ascii_lowercase();
            [
                "ppt/media/",
                "ppt/charts/",
                "ppt/embeddings/",
                "ppt/notesslides/",
                "ppt/comments/",
                "ppt/tags/",
                "ppt/diagrams/",
                "ppt/slides/",
            ]
            .iter()
            .any(|dir| lower.starts_with(dir))
                && !lower.contains("/_rels/")
                && !referenced.contains(&lower)
        })
        .map(str::to_owned)
        .collect())
}

/// A slide's background, first in `p:cSld`.
fn background<'a, 'i>(root: Node<'a, 'i>) -> Option<Node<'a, 'i>> {
    xml::child(root, ns::P, "cSld").and_then(|c| xml::child(c, ns::P, "bg"))
}

/// A free part name like `theirs_part`: same folder, same stem, a new number.
fn new_name(package: &Package, theirs_part: &str) -> String {
    let (dir, file) = theirs_part.rsplit_once('/').unwrap_or(("", theirs_part));
    let (stem, ext) = file.rsplit_once('.').unwrap_or((file, ""));
    let stem = stem.trim_end_matches(|c: char| c.is_ascii_digit());
    free_part_name(package, |n| {
        let name = if ext.is_empty() {
            format!("{stem}{n}")
        } else {
            format!("{stem}{n}.{ext}")
        };
        if dir.is_empty() {
            name
        } else {
            format!("{dir}/{name}")
        }
    })
}

/// Every `cNvPr` id in a slide part.
fn shape_ids_in(text: &str) -> BTreeSet<u32> {
    let Ok(doc) = roxmltree::Document::parse(text) else {
        return BTreeSet::new();
    };
    doc.descendants()
        .filter(|n| xml::is(*n, ns::P, "cNvPr"))
        .filter_map(|n| n.attribute("id").and_then(|v| v.parse().ok()))
        .collect()
}

fn check_prefix(
    prefix: &str,
    uri: &str,
    ours_root: Node<'_, '_>,
    fragment: &str,
    declare: &mut BTreeMap<String, String>,
) -> Result<()> {
    if prefix.is_empty() || prefix == "xml" {
        return Ok(());
    }
    // Declared inside the fragment itself.
    if fragment.contains(&format!("xmlns:{prefix}=")) {
        return Ok(());
    }
    match ours_root.lookup_namespace_uri(Some(prefix)) {
        Some(found) if found == uri => Ok(()),
        Some(_) => Err(cannot("the two decks write this differently")),
        None => {
            declare.insert(prefix.to_owned(), uri.to_owned());
            Ok(())
        }
    }
}

/// The deck without the slides at `indices` (show order), their notes and
/// whatever only they pointed at. Used for clean copies, which leave hidden
/// slides out.
///
/// # Errors
/// [`Error::CannotEdit`] when the result wouldn't hold together.
pub fn remove_slides(deck: &Deck, indices: &[usize]) -> Result<Vec<u8>> {
    let same = super::diff::diff(deck, deck);
    let plan = Plan {
        base: deck,
        ours: deck,
        theirs: deck,
        ours_diff: same.clone(),
        theirs_diff: same,
        items: Vec::new(),
    };
    let mut writer = Writer::new(&plan)?;
    for &index in indices {
        if index < deck.slides.len() {
            writer.delete_slide(index)?;
        }
    }
    let out = writer.finish()?;
    super::check::structure(&out)?;
    Ok(out)
}

/// Writes the decided items of a plan into ours. Items not mentioned are left
/// out; items already there or to bring in by hand are never written.
///
/// # Errors
/// [`Error::CannotEdit`] when a decision doesn't fit its item, or the result
/// couldn't be written safely. Nothing is written in that case.
pub fn apply(plan: &Plan<'_>, decisions: &[(String, Decision)]) -> Result<Vec<u8>> {
    let mut writer = Writer::new(plan)?;
    let mut wrote = false;
    for item in &plan.items {
        let Some((_, decision)) = decisions.iter().find(|(id, _)| *id == item.change.id) else {
            continue;
        };
        match (&item.status, decision) {
            (_, Decision::Decline | Decision::KeepMine) | (Status::AlreadyThere { .. }, _) => {}
            (Status::Clean, Decision::BringIn | Decision::UseMasters) => {
                writer.bring(item, *decision)?;
                wrote = true;
            }
            (Status::Overlap(_), Decision::UseMasters | Decision::KeepBoth { .. }) => {
                if !item.masters_writable {
                    return Err(cannot("this side can't be written"));
                }
                // The master's side replaces this version's, unit by unit.
                writer.bring(item, *decision)?;
                wrote = true;
            }
            (Status::Overlap(_), Decision::BringIn) => {
                return Err(cannot("both changed this: choose a side first"));
            }
            (Status::ByHand(_), _) => return Err(cannot("this change is brought in by hand")),
            (Status::Clean, Decision::KeepBoth { .. }) => {
                return Err(cannot("nothing to keep both of"));
            }
        }
    }
    if !wrote {
        return plan.ours.package.to_bytes();
    }
    let out = writer.finish()?;
    super::check::written(plan, decisions, &out)?;
    Ok(out)
}
