// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! WordprocessingML → canonical model.
//!
//! Every element is classified as a block, a run, an atom, a container or a
//! marker. Anything unknown becomes an atom or an opaque block, so it is compared
//! by its XML and copied untouched, never dropped.

use std::collections::BTreeMap;

use roxmltree::{Node, NodeType};

use super::model::{
    Atom, AtomKind, Block, Cell, Container, HeaderRef, Inline, Lock, Marker, NumRef, Opaque,
    ParaProps, Paragraph, PropMap, Revision, Row, Run, RunProps, SectionBreak, Table, View,
};
use crate::hash::{Fnv, fnv};
use crate::package::Relationships;
use crate::xml::{self, ns};

/// Properties that are on/off switches: `<w:b/>`, `<w:b w:val="true"/>` and
/// `<w:b w:val="1"/>` all mean on.
const TOGGLES: &[&str] = &[
    "b",
    "bCs",
    "i",
    "iCs",
    "caps",
    "smallCaps",
    "strike",
    "dstrike",
    "outline",
    "shadow",
    "emboss",
    "imprint",
    "noProof",
    "snapToGrid",
    "vanish",
    "webHidden",
    "rtl",
    "cs",
    "specVanish",
    "oMath",
    "keepNext",
    "keepLines",
    "pageBreakBefore",
    "widowControl",
    "suppressLineNumbers",
    "suppressAutoHyphens",
    "kinsoku",
    "wordWrap",
    "overflowPunct",
    "topLinePunct",
    "autoSpaceDE",
    "autoSpaceDN",
    "bidi",
    "adjustRightInd",
    "contextualSpacing",
    "mirrorIndents",
    "suppressOverlap",
    "tblHeader",
    "cantSplit",
    "noWrap",
    "hideMark",
    "titlePg",
    "rtlGutter",
    "formProt",
    "noEndnote",
];

/// Run properties Word changes by itself (spell-checking language, proofing),
/// which are not differences a person made.
const IGNORED_RUN_PROPS: &[&str] = &[
    "lang",
    "noProof",
    "rPrChange",
    "ins",
    "del",
    "moveFrom",
    "moveTo",
];

/// Paragraph properties held elsewhere in the model, or that are revision markup.
const IGNORED_PARA_PROPS: &[&str] = &["pStyle", "numPr", "rPr", "sectPr", "pPrChange"];

/// Attributes that are bookkeeping, not formatting.
fn ignored_attr(attr: &roxmltree::Attribute<'_, '_>) -> bool {
    attr.name().starts_with("rsid")
        || (attr.namespace() == Some(ns::W) && attr.name() == "hint")
        || attr.namespace() == Some(ns::W14)
}

fn key_name(node: Node<'_, '_>) -> String {
    let tag = node.tag_name();
    match tag.namespace() {
        Some(ns::W) | None => tag.name().to_owned(),
        Some(other) => format!("{{{other}}}{}", tag.name()),
    }
}

/// Normalised text for one property element, including its children.
fn canon(node: Node<'_, '_>) -> String {
    let name = key_name(node);
    let mut attrs: Vec<(String, String)> = node
        .attributes()
        .filter(|a| !ignored_attr(a))
        .map(|a| {
            let key = match a.namespace() {
                Some(ns::W) | None => a.name().to_owned(),
                Some(other) => format!("{{{other}}}{}", a.name()),
            };
            let value = if key == "val" && TOGGLES.contains(&name.as_str()) {
                toggle(a.value()).to_owned()
            } else {
                a.value().to_owned()
            };
            (key, value)
        })
        .collect();
    if TOGGLES.contains(&name.as_str()) && !attrs.iter().any(|(k, _)| k == "val") {
        attrs.push(("val".to_owned(), "1".to_owned()));
    }
    attrs.sort();
    let mut out: String = attrs.iter().map(|(k, v)| format!("{k}={v};")).collect();
    let children: Vec<String> = node
        .children()
        .filter(Node::is_element)
        .map(|c| format!("{}[{}]", key_name(c), canon(c)))
        .collect();
    if !children.is_empty() {
        out.push('{');
        out.push_str(&children.join(","));
        out.push('}');
    }
    out
}

fn toggle(value: &str) -> &'static str {
    match value {
        "0" | "false" | "off" => "0",
        _ => "1",
    }
}

fn prop_map(node: Node<'_, '_>, skip: &[&str]) -> PropMap {
    let mut map = BTreeMap::new();
    for child in node.children().filter(Node::is_element) {
        let name = key_name(child);
        if skip.contains(&name.as_str()) {
            continue;
        }
        map.insert(name, canon(child));
    }
    PropMap(map)
}

fn w_attr<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attribute((ns::W, name))
}

fn is_w(node: Node<'_, '_>, name: &str) -> bool {
    xml::is(node, ns::W, name)
}

fn hash_raw(text: &str, node: Node<'_, '_>) -> u64 {
    fnv(&text[node.range()])
}

/// All text inside a node, in document order.
fn inner_text(node: Node<'_, '_>) -> String {
    let mut out = String::new();
    for d in node.descendants() {
        if d.is_element() && (is_w(d, "t") || xml::is(d, ns::M, "t")) {
            out.push_str(d.text().unwrap_or_default());
        } else if is_w(d, "tab") {
            out.push('\t');
        }
    }
    out
}

/// Relationship ids referenced anywhere inside a node (`r:id`, `r:embed`, ...).
fn rel_ids(node: Node<'_, '_>) -> Vec<String> {
    let mut ids = Vec::new();
    for d in node.descendants().filter(Node::is_element) {
        for a in d.attributes() {
            if a.namespace() == Some(ns::R) && !ids.iter().any(|i| i == a.value()) {
                ids.push(a.value().to_owned());
            }
        }
    }
    ids
}

/// Shared state for reading one part.
pub(crate) struct Reader<'t> {
    pub text: &'t str,
    pub view: View,
    /// Text of each footnote and endnote, keyed `footnote:1`, for reference keys.
    pub notes: &'t BTreeMap<String, String>,
    /// Relationships of the part, to resolve header and footer references.
    pub rels: Option<&'t Relationships>,
    pub part: &'t str,
    /// Depth of a field that began in an earlier paragraph and has not ended.
    open_field: usize,
}

impl<'t> Reader<'t> {
    pub fn new(
        text: &'t str,
        part: &'t str,
        view: View,
        notes: &'t BTreeMap<String, String>,
        rels: Option<&'t Relationships>,
    ) -> Self {
        Self {
            text,
            view,
            notes,
            rels,
            part,
            open_field: 0,
        }
    }

    /// Reads the blocks inside a container (`w:body`, `w:hdr`, `w:tc`, ...).
    pub fn blocks(&mut self, container: Node<'_, '_>, lock: Option<Lock>) -> Vec<Block> {
        let mut blocks = Vec::new();
        for child in container.children().filter(Node::is_element) {
            self.block(child, lock, &mut blocks);
        }
        join_paragraph_marks(&mut blocks, self.view);
        blocks
    }

    fn block(&mut self, node: Node<'_, '_>, lock: Option<Lock>, out: &mut Vec<Block>) {
        if node.tag_name().namespace() == Some(ns::W) {
            match node.tag_name().name() {
                "p" => {
                    let (paragraph, section) = self.paragraph(node, lock);
                    out.push(Block::Paragraph(paragraph));
                    if let Some(section) = section {
                        out.push(Block::SectionBreak(section));
                    }
                    return;
                }
                "tbl" => {
                    out.push(Block::Table(self.table(node, lock)));
                    return;
                }
                "sectPr" => {
                    out.push(Block::SectionBreak(self.section(node, true)));
                    return;
                }
                // Block content controls and custom XML: read what is inside, but
                // never edit it automatically.
                "sdt" => {
                    if let Some(content) = xml::child(node, ns::W, "sdtContent") {
                        for child in content.children().filter(Node::is_element) {
                            self.block(child, Some(Lock::ContentControl), out);
                        }
                    }
                    return;
                }
                "customXml" => {
                    for child in node.children().filter(Node::is_element) {
                        if !is_w(child, "customXmlPr") {
                            self.block(child, Some(Lock::ContentControl), out);
                        }
                    }
                    return;
                }
                // Zero-width markup between blocks stays where it is.
                "bookmarkStart"
                | "bookmarkEnd"
                | "commentRangeStart"
                | "commentRangeEnd"
                | "moveFromRangeStart"
                | "moveFromRangeEnd"
                | "moveToRangeStart"
                | "moveToRangeEnd"
                | "permStart"
                | "permEnd"
                | "proofErr"
                | "customXmlInsRangeStart"
                | "customXmlInsRangeEnd"
                | "customXmlDelRangeStart"
                | "customXmlDelRangeEnd" => return,
                _ => {}
            }
        }
        if node.tag_name().namespace() != Some(ns::W)
            && node.tag_name().namespace() != Some(ns::MC)
            && node.tag_name().namespace() != Some(ns::M)
        {
            // Elements in other namespaces (ignorable extensions) carry nothing
            // Word shows; they stay in place and are not compared.
            return;
        }
        out.push(Block::Opaque(Opaque {
            span: node.range(),
            key: hash_raw(self.text, node),
            text: inner_text(node),
        }));
    }

    fn section(&self, node: Node<'_, '_>, is_final: bool) -> SectionBreak {
        let mut headers = Vec::new();
        for child in node.children().filter(Node::is_element) {
            let footer = is_w(child, "footerReference");
            if !(footer || is_w(child, "headerReference")) {
                continue;
            }
            let kind = w_attr(child, "type").unwrap_or("default").to_owned();
            let Some(id) = child.attribute((ns::R, "id")) else {
                continue;
            };
            if let Some(rel) = self.rels.and_then(|rels| rels.by_id(id)) {
                headers.push(HeaderRef {
                    footer,
                    kind,
                    part: crate::package::resolve_target(self.part, &rel.target),
                });
            }
        }
        SectionBreak {
            span: node.range(),
            props: prop_map(
                node,
                &["headerReference", "footerReference", "sectPrChange"],
            ),
            headers,
            is_final,
        }
    }

    fn paragraph(
        &mut self,
        node: Node<'_, '_>,
        lock: Option<Lock>,
    ) -> (Paragraph, Option<SectionBreak>) {
        let start_tag = node.range().start..xml::start_tag_end(self.text, node);
        let content_end = xml::end_tag_start(self.text, node).unwrap_or(start_tag.end);
        let mut para = Paragraph {
            span: node.range(),
            start_tag: start_tag.clone(),
            ppr: None,
            content: start_tag.end..content_end,
            anchor: node.attribute((ns::W14, "paraId")).map(str::to_owned),
            props: ParaProps::default(),
            runs: Vec::new(),
            containers: Vec::new(),
            inlines: Vec::new(),
            has_revisions: false,
            mark_revision: None,
            joined: Vec::new(),
            lock,
            note_id: None,
        };
        if self.open_field > 0 {
            para.lock = Some(Lock::SpanningField);
        }
        let mut section = None;
        let mut state = ParaState {
            field: None,
            carried: self.open_field,
            carried_start: None,
        };
        for child in node.children() {
            if is_w(child, "pPr") {
                section = self.ppr(child, &mut para);
                para.content.start = child.range().end;
                continue;
            }
            self.inline(child, &mut para, &mut state, Ctx::default());
        }
        // A field still open at the end of the paragraph spans paragraphs.
        if let Some(field) = state.field.take() {
            self.open_field = field.depth;
            para.lock = Some(Lock::SpanningField);
            let span = field.start..para.content.end;
            para.inlines.push(Inline::Atom(Atom {
                kind: AtomKind::Field,
                key: Fnv::new()
                    .str("open field")
                    .str(&normalise_instr(&field.instr))
                    .finish(),
                display: field.result,
                span,
                run: None,
                container: field.container,
                rel_ids: Vec::new(),
            }));
        } else {
            self.open_field = state.carried;
            if let Some(start) = state.carried_start {
                // The tail of a field that began earlier.
                para.inlines.insert(
                    0,
                    Inline::Atom(Atom {
                        kind: AtomKind::Field,
                        key: fnv("field continues"),
                        display: String::new(),
                        span: start..start,
                        run: None,
                        container: None,
                        rel_ids: Vec::new(),
                    }),
                );
            }
        }
        if para.lock.is_none() && self.open_field > 0 {
            para.lock = Some(Lock::SpanningField);
        }
        merge_texts(&mut para.inlines);
        (para, section)
    }

    fn ppr(&mut self, ppr: Node<'_, '_>, para: &mut Paragraph) -> Option<SectionBreak> {
        para.ppr = Some(ppr.range());
        let mut props = read_para_props(ppr);
        let mut section = None;
        for child in ppr.children().filter(Node::is_element) {
            if is_w(child, "rPr") {
                for mark in child.children().filter(Node::is_element) {
                    if is_w(mark, "ins") || is_w(mark, "moveTo") {
                        para.mark_revision = Some(Revision::Inserted);
                        para.has_revisions = true;
                    } else if is_w(mark, "del") || is_w(mark, "moveFrom") {
                        para.mark_revision = Some(Revision::Deleted);
                        para.has_revisions = true;
                    } else if is_w(mark, "rPrChange") {
                        para.has_revisions = true;
                    }
                }
            } else if is_w(child, "sectPr") {
                section = Some(self.section(child, false));
            } else if is_w(child, "pPrChange") {
                para.has_revisions = true;
                if self.view == View::Rejected
                    && let Some(old) = xml::child(child, ns::W, "pPr")
                {
                    props = read_para_props(old);
                }
            }
        }
        para.props = props;
        section
    }

    fn inline(
        &mut self,
        node: Node<'_, '_>,
        para: &mut Paragraph,
        state: &mut ParaState,
        ctx: Ctx,
    ) {
        match node.node_type() {
            NodeType::Comment | NodeType::PI => {
                if state.absorbing() {
                    return;
                }
                para.inlines.push(Inline::Marker(Marker {
                    span: node.range(),
                    comment: false,
                    container: ctx.container,
                }));
                return;
            }
            NodeType::Element => {}
            _ => return,
        }
        let namespace = node.tag_name().namespace();
        let name = node.tag_name().name();
        if namespace == Some(ns::M) {
            self.push_atom(para, state, ctx, node, AtomKind::Math, inner_text(node));
            return;
        }
        if namespace == Some(ns::MC) {
            self.push_atom(para, state, ctx, node, AtomKind::Other, inner_text(node));
            return;
        }
        if namespace != Some(ns::W) {
            if !state.absorbing() {
                para.inlines.push(Inline::Marker(Marker {
                    span: node.range(),
                    comment: false,
                    container: ctx.container,
                }));
            }
            return;
        }
        match name {
            "r" => self.run(node, para, state, ctx),
            "hyperlink" | "smartTag" | "customXml" | "dir" | "bdo" => {
                if ctx.container.is_some() {
                    // Nested wrappers are rare; keep the inner one whole.
                    self.push_atom(para, state, ctx, node, AtomKind::Other, inner_text(node));
                    return;
                }
                let index = para.containers.len();
                let start_tag = node.range().start..xml::start_tag_end(self.text, node);
                para.containers.push(Container {
                    span: node.range(),
                    start_tag,
                    qname: xml::qname(self.text, node).to_owned(),
                    rel_ids: node
                        .attributes()
                        .filter(|a| a.namespace() == Some(ns::R))
                        .map(|a| a.value().to_owned())
                        .collect(),
                });
                let inner = Ctx {
                    container: Some(index),
                    ..ctx
                };
                for child in node.children() {
                    if is_w(child, "smartTagPr") || is_w(child, "customXmlPr") {
                        continue;
                    }
                    self.inline(child, para, state, inner);
                }
            }
            "ins" | "moveTo" => {
                para.has_revisions = true;
                if self.view == View::Accepted {
                    for child in node.children() {
                        self.inline(child, para, state, ctx);
                    }
                }
            }
            "del" | "moveFrom" => {
                para.has_revisions = true;
                if self.view == View::Rejected {
                    let inner = Ctx {
                        deleted: true,
                        ..ctx
                    };
                    for child in node.children() {
                        self.inline(child, para, state, inner);
                    }
                }
            }
            "bookmarkStart"
            | "bookmarkEnd"
            | "commentRangeStart"
            | "commentRangeEnd"
            | "moveFromRangeStart"
            | "moveFromRangeEnd"
            | "moveToRangeStart"
            | "moveToRangeEnd"
            | "permStart"
            | "permEnd"
            | "proofErr"
            | "customXmlInsRangeStart"
            | "customXmlInsRangeEnd"
            | "customXmlDelRangeStart"
            | "customXmlDelRangeEnd"
            | "customXmlMoveFromRangeStart"
            | "customXmlMoveFromRangeEnd"
            | "customXmlMoveToRangeStart"
            | "customXmlMoveToRangeEnd" => {
                if name.starts_with("moveFromRange") || name.starts_with("moveToRange") {
                    para.has_revisions = true;
                }
                if !state.absorbing() {
                    para.inlines.push(Inline::Marker(Marker {
                        span: node.range(),
                        comment: name.starts_with("commentRange"),
                        container: ctx.container,
                    }));
                }
            }
            "fldSimple" => {
                let instr = w_attr(node, "instr").unwrap_or_default();
                let display = inner_text(node);
                if state.absorbing() {
                    return;
                }
                para.inlines.push(Inline::Atom(Atom {
                    kind: AtomKind::Field,
                    key: Fnv::new()
                        .str("field")
                        .str(&normalise_instr(instr))
                        .finish(),
                    display,
                    span: node.range(),
                    run: None,
                    container: ctx.container,
                    rel_ids: rel_ids(node),
                }));
            }
            "sdt" => {
                let display = xml::child(node, ns::W, "sdtContent")
                    .map(inner_text)
                    .unwrap_or_default();
                self.push_atom(para, state, ctx, node, AtomKind::ContentControl, display);
            }
            _ => self.push_atom(para, state, ctx, node, AtomKind::Other, inner_text(node)),
        }
    }

    fn push_atom(
        &self,
        para: &mut Paragraph,
        state: &ParaState,
        ctx: Ctx,
        node: Node<'_, '_>,
        kind: AtomKind,
        display: String,
    ) {
        if state.absorbing() {
            return;
        }
        para.inlines.push(Inline::Atom(Atom {
            kind,
            key: Fnv::new()
                .u64(kind as u64)
                .u64(hash_raw(self.text, node))
                .finish(),
            display,
            span: node.range(),
            run: None,
            container: ctx.container,
            rel_ids: rel_ids(node),
        }));
    }

    fn run(&mut self, node: Node<'_, '_>, para: &mut Paragraph, state: &mut ParaState, ctx: Ctx) {
        let rpr = xml::child(node, ns::W, "rPr");
        let mut props = rpr.map(read_run_props).unwrap_or_default();
        if let Some(change) = rpr.and_then(|r| xml::child(r, ns::W, "rPrChange")) {
            para.has_revisions = true;
            if self.view == View::Rejected {
                props = xml::child(change, ns::W, "rPr")
                    .map(read_run_props)
                    .unwrap_or_default();
            }
        }
        let index = para.runs.len();
        para.runs.push(Run {
            span: node.range(),
            rpr: rpr.map(|r| r.range()),
            props,
            container: ctx.container,
        });

        if node.children().any(|c| is_w(c, "commentReference")) && !state.absorbing() {
            para.inlines.push(Inline::Marker(Marker {
                span: node.range(),
                comment: true,
                container: ctx.container,
            }));
            return;
        }

        for child in node.children().filter(Node::is_element) {
            if child.tag_name().namespace() != Some(ns::W) {
                let kind = if xml::is(child, ns::MC, "AlternateContent") {
                    AtomKind::Drawing
                } else {
                    AtomKind::Other
                };
                self.run_atom(para, state, ctx, index, child, kind, inner_text(child));
                continue;
            }
            match child.tag_name().name() {
                "rPr" | "lastRenderedPageBreak" | "annotationRef" => {}
                "fldChar" => {
                    let kind = w_attr(child, "fldCharType").unwrap_or_default();
                    state.field_char(kind, node, ctx, para);
                }
                "instrText" | "delInstrText" => {
                    if let Some(field) = state.field.as_mut()
                        && field.depth == 1
                        && !field.separated
                    {
                        field.instr.push_str(child.text().unwrap_or_default());
                    }
                }
                "t" | "delText" => {
                    let text = child.text().unwrap_or_default();
                    if child.tag_name().name() == "delText" && !ctx.deleted {
                        continue;
                    }
                    if let Some(field) = state.field.as_mut() {
                        if field.depth == 1 && field.separated {
                            field.result.push_str(text);
                        }
                        continue;
                    }
                    if state.carried > 0 {
                        continue;
                    }
                    push_text(para, index, text);
                }
                "tab" | "ptab" => self.run_char(para, state, index, "\t"),
                "cr" => self.run_char(para, state, index, "\n"),
                "br" => match w_attr(child, "type") {
                    Some("page") => self.run_atom(
                        para,
                        state,
                        ctx,
                        index,
                        child,
                        AtomKind::PageBreak,
                        String::new(),
                    ),
                    Some("column") => {
                        self.run_atom(
                            para,
                            state,
                            ctx,
                            index,
                            child,
                            AtomKind::ColumnBreak,
                            String::new(),
                        );
                    }
                    _ => self.run_char(para, state, index, "\n"),
                },
                "noBreakHyphen" => self.run_char(para, state, index, "\u{2011}"),
                "softHyphen" => self.run_char(para, state, index, "\u{00AD}"),
                "sym" => {
                    let display = w_attr(child, "char")
                        .and_then(|c| u32::from_str_radix(c, 16).ok())
                        .and_then(char::from_u32)
                        .map(String::from)
                        .unwrap_or_default();
                    self.run_atom(para, state, ctx, index, child, AtomKind::Symbol, display);
                }
                "drawing" | "pict" | "object" => {
                    self.run_atom(
                        para,
                        state,
                        ctx,
                        index,
                        child,
                        AtomKind::Drawing,
                        String::new(),
                    );
                }
                "footnoteReference" | "endnoteReference" => {
                    let kind = if child.tag_name().name() == "footnoteReference" {
                        "footnote"
                    } else {
                        "endnote"
                    };
                    let id = w_attr(child, "id").unwrap_or_default();
                    let note = self
                        .notes
                        .get(&format!("{kind}:{id}"))
                        .cloned()
                        .unwrap_or_default();
                    if state.absorbing() {
                        continue;
                    }
                    para.inlines.push(Inline::Atom(Atom {
                        kind: AtomKind::NoteReference,
                        key: Fnv::new().str(kind).str(&note).finish(),
                        display: String::new(),
                        span: node.range(),
                        run: Some(index),
                        container: ctx.container,
                        rel_ids: Vec::new(),
                    }));
                }
                _ => {
                    let display = inner_text(child);
                    self.run_atom(para, state, ctx, index, child, AtomKind::Other, display);
                }
            }
        }
    }

    fn run_char(&self, para: &mut Paragraph, state: &ParaState, run: usize, ch: &str) {
        if state.absorbing() {
            return;
        }
        push_text(para, run, ch);
    }

    /// An atom inside a run. Its span is the element itself; the writer wraps it
    /// in a run with the run's own properties.
    #[allow(clippy::too_many_arguments)]
    fn run_atom(
        &self,
        para: &mut Paragraph,
        state: &ParaState,
        ctx: Ctx,
        run: usize,
        node: Node<'_, '_>,
        kind: AtomKind,
        display: String,
    ) {
        if state.absorbing() {
            return;
        }
        para.inlines.push(Inline::Atom(Atom {
            kind,
            key: Fnv::new()
                .u64(kind as u64)
                .u64(hash_raw(self.text, node))
                .finish(),
            display,
            span: node.range(),
            run: Some(run),
            container: ctx.container,
            rel_ids: rel_ids(node),
        }));
    }

    fn table(&mut self, node: Node<'_, '_>, lock: Option<Lock>) -> Table {
        let mut key = Fnv::new();
        let mut columns = 0;
        let mut has_revisions = false;
        let mut table_lock = None;
        let mut rows = Vec::new();
        for child in node.children().filter(Node::is_element) {
            if is_w(child, "tblPr") {
                key.str(
                    &prop_map(child, &["tblPrChange"])
                        .0
                        .values()
                        .cloned()
                        .collect::<Vec<_>>()
                        .join("|"),
                );
                has_revisions |= xml::child(child, ns::W, "tblPrChange").is_some();
            } else if is_w(child, "tblGrid") {
                columns = child.children().filter(|c| is_w(*c, "gridCol")).count();
                key.str(&canon(child));
                has_revisions |= xml::child(child, ns::W, "tblGridChange").is_some();
            } else if is_w(child, "tr") {
                let row = self.row(child, lock, &mut has_revisions, &mut table_lock);
                let hidden = matches!(
                    (row.revision, self.view),
                    (Some(Revision::Deleted), View::Accepted)
                        | (Some(Revision::Inserted), View::Rejected)
                );
                if !hidden {
                    rows.push(row);
                }
            } else if !matches!(
                child.tag_name().name(),
                "bookmarkStart"
                    | "bookmarkEnd"
                    | "proofErr"
                    | "commentRangeStart"
                    | "commentRangeEnd"
            ) {
                table_lock = Some(Lock::ContentControl);
            }
        }
        Table {
            span: node.range(),
            props_key: key.finish(),
            columns,
            rows,
            has_revisions,
            lock: table_lock.or(lock),
        }
    }

    fn row(
        &mut self,
        node: Node<'_, '_>,
        lock: Option<Lock>,
        has_revisions: &mut bool,
        table_lock: &mut Option<Lock>,
    ) -> Row {
        let mut row = Row {
            span: node.range(),
            trpr: None,
            props_key: 0,
            cells: Vec::new(),
            revision: None,
        };
        for child in node.children().filter(Node::is_element) {
            if is_w(child, "trPr") {
                row.trpr = Some(child.range());
                row.props_key = fnv(&prop_map(child, &["ins", "del", "trPrChange"])
                    .0
                    .values()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("|"));
                for mark in child.children().filter(Node::is_element) {
                    if is_w(mark, "ins") {
                        row.revision = Some(Revision::Inserted);
                        *has_revisions = true;
                    } else if is_w(mark, "del") {
                        row.revision = Some(Revision::Deleted);
                        *has_revisions = true;
                    } else if is_w(mark, "trPrChange") {
                        *has_revisions = true;
                    }
                }
            } else if is_w(child, "tc") {
                let cell = self.cell(child, lock, has_revisions);
                row.cells.push(cell);
            } else if is_w(child, "tblPrEx")
                || matches!(
                    child.tag_name().name(),
                    "bookmarkStart"
                        | "bookmarkEnd"
                        | "proofErr"
                        | "commentRangeStart"
                        | "commentRangeEnd"
                )
            {
            } else {
                *table_lock = Some(Lock::ContentControl);
            }
        }
        row
    }

    fn cell(&mut self, node: Node<'_, '_>, lock: Option<Lock>, has_revisions: &mut bool) -> Cell {
        let mut cell = Cell {
            span: node.range(),
            props_key: 0,
            grid_span: 1,
            vmerge: None,
            blocks: Vec::new(),
        };
        if let Some(tcpr) = xml::child(node, ns::W, "tcPr") {
            cell.props_key = fnv(&prop_map(
                tcpr,
                &["tcPrChange", "cellIns", "cellDel", "cellMerge"],
            )
            .0
            .values()
            .cloned()
            .collect::<Vec<_>>()
            .join("|"));
            if let Some(span) = xml::child(tcpr, ns::W, "gridSpan").and_then(|g| w_attr(g, "val")) {
                cell.grid_span = span.parse().unwrap_or(1);
            }
            if let Some(merge) = xml::child(tcpr, ns::W, "vMerge") {
                cell.vmerge = Some(w_attr(merge, "val") == Some("restart"));
            }
            for change in ["tcPrChange", "cellIns", "cellDel", "cellMerge"] {
                *has_revisions |= xml::child(tcpr, ns::W, change).is_some();
            }
        }
        let mut blocks = Vec::new();
        for child in node.children().filter(Node::is_element) {
            if !is_w(child, "tcPr") {
                self.block(child, lock, &mut blocks);
            }
        }
        join_paragraph_marks(&mut blocks, self.view);
        for block in &blocks {
            match block {
                Block::Paragraph(p) => *has_revisions |= p.has_revisions,
                Block::Table(t) => *has_revisions |= t.has_revisions,
                _ => {}
            }
        }
        cell.blocks = blocks;
        cell
    }
}

fn read_para_props(ppr: Node<'_, '_>) -> ParaProps {
    let style = xml::child(ppr, ns::W, "pStyle")
        .and_then(|s| w_attr(s, "val"))
        .map(str::to_owned);
    let numbering = xml::child(ppr, ns::W, "numPr").and_then(|num| {
        let num_id = xml::child(num, ns::W, "numId").and_then(|n| w_attr(n, "val"))?;
        let level = xml::child(num, ns::W, "ilvl")
            .and_then(|n| w_attr(n, "val"))
            .unwrap_or("0");
        Some(NumRef {
            num_id: num_id.to_owned(),
            level: level.to_owned(),
        })
    });
    ParaProps {
        style,
        numbering,
        direct: prop_map(ppr, IGNORED_PARA_PROPS),
    }
}

fn read_run_props(rpr: Node<'_, '_>) -> RunProps {
    let mut skip: Vec<&str> = IGNORED_RUN_PROPS.to_vec();
    skip.push("rStyle");
    RunProps {
        style: xml::child(rpr, ns::W, "rStyle")
            .and_then(|s| w_attr(s, "val"))
            .map(str::to_owned),
        direct: prop_map(rpr, &skip),
    }
}

fn normalise_instr(instr: &str) -> String {
    instr.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn push_text(para: &mut Paragraph, run: usize, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(Inline::Text {
        text: last,
        run: last_run,
    }) = para.inlines.last_mut()
        && *last_run == run
    {
        last.push_str(text);
        return;
    }
    para.inlines.push(Inline::Text {
        text: text.to_owned(),
        run,
    });
}

/// Joins adjacent text pieces of the same run (after fields were collapsed).
fn merge_texts(inlines: &mut Vec<Inline>) {
    let mut out: Vec<Inline> = Vec::with_capacity(inlines.len());
    for inline in inlines.drain(..) {
        if let (
            Some(Inline::Text {
                text: last,
                run: last_run,
            }),
            Inline::Text { text, run },
        ) = (out.last_mut(), &inline)
            && last_run == run
        {
            last.push_str(text);
            continue;
        }
        out.push(inline);
    }
    *inlines = out;
}

/// In the accepted view a paragraph whose mark is deleted joins the next one; in
/// the rejected view a paragraph whose mark is inserted does.
fn join_paragraph_marks(blocks: &mut Vec<Block>, view: View) {
    let joins = |p: &Paragraph| match view {
        View::Accepted => p.mark_revision == Some(Revision::Deleted),
        View::Rejected => p.mark_revision == Some(Revision::Inserted),
    };
    let mut index = 0;
    while index + 1 < blocks.len() {
        let should_join = matches!(&blocks[index], Block::Paragraph(p) if joins(p))
            && matches!(&blocks[index + 1], Block::Paragraph(_));
        if !should_join {
            index += 1;
            continue;
        }
        let Block::Paragraph(first) = blocks.remove(index) else {
            unreachable!("checked above");
        };
        let Block::Paragraph(second) = &mut blocks[index] else {
            unreachable!("checked above");
        };
        let run_offset = first.runs.len();
        let container_offset = first.containers.len();
        let mut inlines = first.inlines;
        for inline in &mut second.inlines {
            match inline {
                Inline::Text { run, .. } => *run += run_offset,
                Inline::Atom(atom) => {
                    atom.run = atom.run.map(|r| r + run_offset);
                    atom.container = atom.container.map(|c| c + container_offset);
                }
                Inline::Marker(marker) => {
                    marker.container = marker.container.map(|c| c + container_offset);
                }
            }
        }
        inlines.append(&mut second.inlines);
        second.inlines = inlines;
        merge_texts(&mut second.inlines);
        let mut runs = first.runs;
        for run in &mut second.runs {
            run.container = run.container.map(|c| c + container_offset);
        }
        runs.append(&mut second.runs);
        second.runs = runs;
        let mut containers = first.containers;
        containers.append(&mut second.containers);
        second.containers = containers;
        second.joined.insert(0, first.span.clone());
        second.joined.splice(0..0, first.joined);
        second.span = first.span.start..second.span.end;
        second.has_revisions = true;
        second.lock = second.lock.or(first.lock);
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Ctx {
    container: Option<usize>,
    /// Inside `w:del` in the rejected view, where `w:delText` is visible text.
    deleted: bool,
}

struct FieldBuilder {
    start: usize,
    depth: usize,
    separated: bool,
    instr: String,
    result: String,
    container: Option<usize>,
}

struct ParaState {
    field: Option<FieldBuilder>,
    /// Depth of a field carried in from an earlier paragraph.
    carried: usize,
    /// Where the carried field's content in this paragraph starts.
    carried_start: Option<usize>,
}

impl ParaState {
    fn absorbing(&self) -> bool {
        self.field.is_some() || self.carried > 0
    }

    fn field_char(&mut self, kind: &str, run: Node<'_, '_>, ctx: Ctx, para: &mut Paragraph) {
        if self.carried > 0 {
            if self.carried_start.is_none() {
                self.carried_start = Some(run.range().start);
            }
            match kind {
                "begin" => self.carried += 1,
                "end" => self.carried -= 1,
                _ => {}
            }
            return;
        }
        match (kind, self.field.as_mut()) {
            ("begin", None) => {
                self.field = Some(FieldBuilder {
                    start: run.range().start,
                    depth: 1,
                    separated: false,
                    instr: String::new(),
                    result: String::new(),
                    container: ctx.container,
                });
            }
            ("begin", Some(field)) => field.depth += 1,
            ("separate", Some(field)) => {
                if field.depth == 1 {
                    field.separated = true;
                }
            }
            ("end", Some(field)) => {
                field.depth -= 1;
                if field.depth == 0 {
                    let field = self.field.take().expect("present");
                    if field.container != ctx.container {
                        para.lock = Some(Lock::SpanningField);
                    }
                    para.inlines.push(Inline::Atom(Atom {
                        kind: AtomKind::Field,
                        key: Fnv::new()
                            .str("field")
                            .str(&normalise_instr(&field.instr))
                            .finish(),
                        display: field.result,
                        span: field.start..run.range().end,
                        run: None,
                        container: field.container,
                        rel_ids: Vec::new(),
                    }));
                }
            }
            _ => {}
        }
    }
}
