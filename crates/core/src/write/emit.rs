// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! XML for tracked changes: inserted, deleted and rewritten paragraphs, rows and
//! tables. Element order follows ECMA-376 Part 1: in `w:pPr` the paragraph
//! mark's `w:rPr` comes after the other properties and before `w:sectPr` and
//! `w:pPrChange`; in a paragraph mark's `w:rPr`, `w:ins`/`w:del` come first; in
//! `w:rPr`, `w:rPrChange` comes last; in `w:trPr`, `w:ins`/`w:del` come after the
//! row properties.

use roxmltree::Node;

use super::Writer;
use crate::diff::inline::{Step, diff_paragraphs};
use crate::docx::{Atom, AtomKind, Block, Inline, Paragraph, Row, Table};
use crate::error::{Error, Result};
use crate::xml::{self, Edits, escape_text, ns};

/// Which document a piece of content comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum From {
    Ours,
    Theirs,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Same,
    /// Theirs formatting, with ours kept in `w:rPrChange`.
    Reformatted {
        old_inline: usize,
    },
    Removed,
    Added,
    Marker,
}

struct Piece {
    from: From,
    op: Op,
    inline: usize,
    /// Text for text pieces; `None` for atoms and markers.
    text: Option<String>,
}

/// Atoms the writer can wrap in a deletion.
pub(crate) fn deletable(atom: &Atom) -> bool {
    matches!(
        atom.kind,
        AtomKind::Drawing
            | AtomKind::Symbol
            | AtomKind::PageBreak
            | AtomKind::ColumnBreak
            | AtomKind::Field
    )
}

impl<'a> Writer<'a> {
    fn src(&self, from: From, part: &str) -> Result<&'a str> {
        let docx = match from {
            From::Ours => self.ours,
            From::Theirs => self.theirs,
        };
        docx.part_text(part)
    }

    fn tag(&self, name: &str) -> String {
        format!("{}:{name}", self.w)
    }

    /// Run content for text: `w:t` (or `w:delText`) plus elements for tabs,
    /// breaks and special hyphens.
    pub(crate) fn text_xml(&self, text: &str, deleted: bool) -> String {
        let t = self.tag(if deleted { "delText" } else { "t" });
        let mut out = String::new();
        let mut plain = String::new();
        let flush = |plain: &mut String, out: &mut String| {
            if !plain.is_empty() {
                out.push_str(&format!(
                    r#"<{t} xml:space="preserve">{}</{t}>"#,
                    escape_text(plain)
                ));
                plain.clear();
            }
        };
        for ch in text.chars() {
            let special = match ch {
                '\t' => Some("tab"),
                '\n' => Some("br"),
                '\r' => Some("cr"),
                '\u{2011}' => Some("noBreakHyphen"),
                '\u{00AD}' => Some("softHyphen"),
                _ => None,
            };
            match special {
                Some(name) => {
                    flush(&mut plain, &mut out);
                    out.push_str(&format!("<{}/>", self.tag(name)));
                }
                None => plain.push(ch),
            }
        }
        flush(&mut plain, &mut out);
        out
    }

    fn run_xml(&self, rpr: &str, inner: &str) -> String {
        let r = self.tag("r");
        format!("<{r}>{rpr}{inner}</{r}>")
    }

    /// The raw `w:rPr` of a run, with any `w:rPrChange` removed.
    fn rpr_of(&self, from: From, part: &str, p: &Paragraph, run: Option<usize>) -> Result<String> {
        let Some(run) = run else {
            return Ok(String::new());
        };
        let Some(span) = p.runs[run].rpr.clone() else {
            return Ok(String::new());
        };
        let src = self.src(from, part)?;
        let doc = self.doc(from, part)?;
        let node = node_at(doc, span.start).ok_or_else(|| cannot("run properties not found"))?;
        Ok(rebuild_without(src, node, &["rPrChange"]))
    }

    /// Inner children of a `w:rPr`, minus `w:rPrChange`.
    fn rpr_inner(
        &self,
        from: From,
        part: &str,
        p: &Paragraph,
        run: Option<usize>,
    ) -> Result<String> {
        let Some(span) = run.and_then(|r| p.runs[r].rpr.clone()) else {
            return Ok(String::new());
        };
        let src = self.src(from, part)?;
        let doc = self.doc(from, part)?;
        let node = node_at(doc, span.start).ok_or_else(|| cannot("run properties not found"))?;
        Ok(children_except(src, node, &["rPrChange"]).concat())
    }

    fn inline_run(p: &Paragraph, inline: usize) -> Option<usize> {
        match &p.inlines[inline] {
            Inline::Text { run, .. } => Some(*run),
            Inline::Atom(atom) => atom.run,
            Inline::Marker(_) => None,
        }
    }

    fn inline_container(p: &Paragraph, inline: usize) -> Option<usize> {
        match &p.inlines[inline] {
            Inline::Text { run, .. } => p.runs[*run].container,
            Inline::Atom(atom) => atom.container,
            Inline::Marker(marker) => marker.container,
        }
    }

    /// XML for one piece.
    #[allow(clippy::too_many_arguments)]
    fn piece_xml(
        &self,
        piece: &Piece,
        part_o: &str,
        po: Option<&Paragraph>,
        part_t: &str,
        pt: Option<&Paragraph>,
    ) -> Result<String> {
        let (part, p) = match piece.from {
            From::Ours => (part_o, po.ok_or_else(|| cannot("no paragraph"))?),
            From::Theirs => (part_t, pt.ok_or_else(|| cannot("no paragraph"))?),
        };
        let src = self.src(piece.from, part)?;
        let run = Self::inline_run(p, piece.inline);
        if piece.op == Op::Marker {
            return match &p.inlines[piece.inline] {
                Inline::Marker(marker) => Ok(src[marker.span.clone()].to_owned()),
                _ => Err(cannot("marker expected")),
            };
        }
        let rpr = match piece.op {
            Op::Reformatted { old_inline } => {
                let po = po.ok_or_else(|| cannot("no paragraph"))?;
                let old =
                    self.rpr_inner(From::Ours, part_o, po, Self::inline_run(po, old_inline))?;
                let new = self.rpr_inner(From::Theirs, part_t, p, run)?;
                let rpr = self.tag("rPr");
                format!(
                    "<{rpr}>{new}<{c} {rev}><{rpr}>{old}</{rpr}></{c}></{rpr}>",
                    c = self.tag("rPrChange"),
                    rev = self.rev_attrs()
                )
            }
            _ => self.rpr_of(piece.from, part, p, run)?,
        };
        let deleted = piece.op == Op::Removed;
        match (&piece.text, &p.inlines[piece.inline]) {
            (Some(text), _) => Ok(self.run_xml(&rpr, &self.text_xml(text, deleted))),
            (None, Inline::Atom(atom)) => {
                if deleted && !deletable(atom) {
                    return Err(cannot("this content cannot be deleted as a tracked change"));
                }
                let raw = &src[atom.span.clone()];
                let raw = if deleted {
                    self.to_deleted(raw)
                } else {
                    raw.to_owned()
                };
                Ok(if atom.run.is_some() {
                    self.run_xml(&rpr, &raw)
                } else {
                    raw
                })
            }
            _ => Err(cannot("unexpected inline")),
        }
    }

    /// `w:t` → `w:delText`, `w:instrText` → `w:delInstrText` in runs of an atom.
    fn to_deleted(&self, raw: &str) -> String {
        let w = &self.w;
        raw.replace(&format!("<{w}:t>"), &format!("<{w}:delText>"))
            .replace(&format!("<{w}:t "), &format!("<{w}:delText "))
            .replace(&format!("</{w}:t>"), &format!("</{w}:delText>"))
            .replace(&format!("<{w}:instrText>"), &format!("<{w}:delInstrText>"))
            .replace(&format!("<{w}:instrText "), &format!("<{w}:delInstrText "))
            .replace(
                &format!("</{w}:instrText>"),
                &format!("</{w}:delInstrText>"),
            )
    }

    /// Paragraph content from pieces: grouped by container, with consecutive
    /// deletions and insertions sharing one `w:del` or `w:ins`.
    fn content_xml(
        &self,
        pieces: &[Piece],
        part_o: &str,
        po: Option<&Paragraph>,
        part_t: &str,
        pt: Option<&Paragraph>,
    ) -> Result<String> {
        let mut out = String::new();
        let mut index = 0;
        while index < pieces.len() {
            let from = pieces[index].from;
            let p = match from {
                From::Ours => po,
                From::Theirs => pt,
            }
            .ok_or_else(|| cannot("no paragraph"))?;
            let container = Self::inline_container(p, pieces[index].inline);
            let mut end = index + 1;
            while end < pieces.len()
                && pieces[end].from == from
                && Self::inline_container(p, pieces[end].inline) == container
            {
                end += 1;
            }
            let mut inner = String::new();
            let mut k = index;
            while k < end {
                let op = pieces[k].op;
                if matches!(op, Op::Removed | Op::Added) {
                    let mut wrapped = String::new();
                    let mut m = k;
                    while m < end && pieces[m].op == op {
                        wrapped.push_str(&self.piece_xml(&pieces[m], part_o, po, part_t, pt)?);
                        m += 1;
                    }
                    let name = self.tag(if op == Op::Removed { "del" } else { "ins" });
                    inner.push_str(&format!("<{name} {}>{wrapped}</{name}>", self.rev_attrs()));
                    k = m;
                } else {
                    inner.push_str(&self.piece_xml(&pieces[k], part_o, po, part_t, pt)?);
                    k += 1;
                }
            }
            match container {
                None => out.push_str(&inner),
                Some(c) => {
                    let part = match from {
                        From::Ours => part_o,
                        From::Theirs => part_t,
                    };
                    let src = self.src(from, part)?;
                    let container = &p.containers[c];
                    let start = &src[container.start_tag.clone()];
                    let start = if from == From::Theirs {
                        self.remap_rel_ids(part_t, part_o, start)?
                    } else {
                        start.to_owned()
                    };
                    out.push_str(&format!("{start}{inner}</{}>", container.qname));
                }
            }
            index = end;
        }
        Ok(out)
    }

    /// Pieces for a whole paragraph with one operation (all added or all removed).
    fn whole(p: &Paragraph, from: From, op: Op) -> Vec<Piece> {
        let mut pieces: Vec<Piece> = Vec::new();
        for (index, inline) in p.inlines.iter().enumerate() {
            match inline {
                Inline::Text { text, .. } => pieces.push(Piece {
                    from,
                    op,
                    inline: index,
                    text: Some(text.clone()),
                }),
                Inline::Atom(_) => pieces.push(Piece {
                    from,
                    op,
                    inline: index,
                    text: None,
                }),
                Inline::Marker(_) => {
                    // Markers stay with ours; content copied from theirs drops its
                    // bookmarks and comment ranges.
                    if from == From::Ours {
                        pieces.push(Piece {
                            from,
                            op: Op::Marker,
                            inline: index,
                            text: None,
                        });
                    }
                }
            }
        }
        pieces
    }

    /// Children of a `w:pPr` (as raw XML) except the named ones.
    fn ppr_children(
        &self,
        from: From,
        part: &str,
        p: &Paragraph,
        except: &[&str],
    ) -> Result<Vec<&'a str>> {
        let Some(span) = p.ppr.clone() else {
            return Ok(Vec::new());
        };
        let src = self.src(from, part)?;
        let doc = self.doc(from, part)?;
        let node =
            node_at(doc, span.start).ok_or_else(|| cannot("paragraph properties not found"))?;
        Ok(children_except(src, node, except))
    }

    /// The paragraph mark's run properties (inner XML), minus revision markup.
    fn mark_rpr_inner(&self, from: From, part: &str, p: &Paragraph) -> Result<String> {
        let Some(span) = p.ppr.clone() else {
            return Ok(String::new());
        };
        let src = self.src(from, part)?;
        let doc = self.doc(from, part)?;
        let ppr =
            node_at(doc, span.start).ok_or_else(|| cannot("paragraph properties not found"))?;
        Ok(xml::child(ppr, ns::W, "rPr")
            .map(|rpr| {
                children_except(src, rpr, &["ins", "del", "moveFrom", "moveTo", "rPrChange"])
                    .concat()
            })
            .unwrap_or_default())
    }

    /// `w:pPr` with the paragraph mark marked inserted or deleted.
    fn ppr_with_mark(&self, from: From, part: &str, p: &Paragraph, mark: &str) -> Result<String> {
        let rest = self
            .ppr_children(from, part, p, &["rPr", "sectPr", "pPrChange"])?
            .concat();
        let sect = if from == From::Ours {
            self.ppr_children(from, part, p, &[])?
                .into_iter()
                .filter(|c| is_element(c, &self.w, "sectPr"))
                .collect::<String>()
        } else {
            String::new()
        };
        let mark_rpr = self.mark_rpr_inner(from, part, p)?;
        let (ppr, rpr) = (self.tag("pPr"), self.tag("rPr"));
        Ok(format!(
            "<{ppr}>{rest}<{rpr}><{m} {rev}/>{mark_rpr}</{rpr}>{sect}</{ppr}>",
            m = self.tag(mark),
            rev = self.rev_attrs()
        ))
    }

    /// A new paragraph from theirs, as a tracked insertion.
    pub(crate) fn inserted_paragraph(
        &self,
        part_t: &str,
        pt: &Paragraph,
        part_o: &str,
    ) -> Result<String> {
        let ppr = self.ppr_with_mark(From::Theirs, part_t, pt, "ins")?;
        let pieces = Self::whole(pt, From::Theirs, Op::Added);
        let content = self.content_xml(&pieces, part_o, None, part_t, Some(pt))?;
        let p = self.tag("p");
        Ok(format!("<{p}>{ppr}{content}</{p}>"))
    }

    /// One of ours' paragraphs, as a tracked deletion. `keep_mark` keeps the
    /// paragraph mark (for the last paragraph of a cell or the body, which Word
    /// cannot delete): the paragraph is emptied instead.
    pub(crate) fn deleted_paragraph(
        &self,
        part_o: &str,
        po: &Paragraph,
        keep_mark: bool,
    ) -> Result<String> {
        let src = self.src(From::Ours, part_o)?;
        let ppr = if keep_mark {
            po.ppr
                .clone()
                .map(|s| src[s].to_owned())
                .unwrap_or_default()
        } else {
            self.ppr_with_mark(From::Ours, part_o, po, "del")?
        };
        let pieces = Self::whole(po, From::Ours, Op::Removed);
        let content = self.content_xml(&pieces, part_o, Some(po), part_o, None)?;
        let start = &src[po.start_tag.clone()];
        Ok(element(start, &format!("{ppr}{content}")))
    }

    /// `w:pPr` with `new`'s properties, keeping `old`'s in `w:pPrChange` (and the
    /// mark's run properties in `w:rPrChange`), so rejecting the change gives
    /// `old`'s formatting back. Used where the last paragraph mark of a sequence
    /// changes hands (see `ends.rs`). Section breaks are never copied.
    fn swapped_ppr(
        &self,
        new: (From, &str, &Paragraph),
        old: (From, &str, &Paragraph),
    ) -> Result<String> {
        let (new_from, new_part, new_p) = new;
        let (old_from, old_part, old_p) = old;
        let except = ["rPr", "sectPr", "pPrChange"];
        let new_rest = self
            .ppr_children(new_from, new_part, new_p, &except)?
            .concat();
        let old_rest = self
            .ppr_children(old_from, old_part, old_p, &except)?
            .concat();
        let new_mark = self.mark_rpr_inner(new_from, new_part, new_p)?;
        let old_mark = self.mark_rpr_inner(old_from, old_part, old_p)?;
        let (ppr, rpr) = (self.tag("pPr"), self.tag("rPr"));
        let mut out = format!("<{ppr}>{new_rest}");
        if !new_mark.is_empty() || new_mark != old_mark {
            out.push_str(&format!("<{rpr}>{new_mark}"));
            if new_mark != old_mark {
                out.push_str(&format!(
                    "<{c} {rev}><{rpr}>{old_mark}</{rpr}></{c}>",
                    c = self.tag("rPrChange"),
                    rev = self.rev_attrs()
                ));
            }
            out.push_str(&format!("</{rpr}>"));
        }
        if new_p.props != old_p.props {
            out.push_str(&format!(
                "<{c} {rev}><{ppr}>{old_rest}</{ppr}></{c}>",
                c = self.tag("pPrChange"),
                rev = self.rev_attrs()
            ));
        }
        out.push_str(&format!("</{ppr}>"));
        Ok(out)
    }

    /// The last of the paragraphs added after a sequence's last paragraph
    /// (`anchor`). It takes over the sequence's final paragraph mark: its mark is
    /// not marked inserted (the anchor's is, see `ends.rs`), and its properties
    /// record the anchor's as the old ones. Accepting gives both paragraphs;
    /// rejecting gives the anchor back exactly, with no empty paragraph left.
    pub(crate) fn final_inserted_paragraph(
        &self,
        part_t: &str,
        pt: &Paragraph,
        part_o: &str,
        anchor: &Paragraph,
    ) -> Result<String> {
        let ppr = self.swapped_ppr((From::Theirs, part_t, pt), (From::Ours, part_o, anchor))?;
        let pieces = Self::whole(pt, From::Theirs, Op::Added);
        let content = self.content_xml(&pieces, part_o, None, part_t, Some(pt))?;
        let p = self.tag("p");
        Ok(format!("<{p}>{ppr}{content}</{p}>"))
    }

    /// A sequence's last paragraph, deleted. Its text is deleted and it takes
    /// `partner`'s properties (its new ones, if it is being rewritten), while
    /// `partner` (the paragraph before it) gets its mark deleted (see `ends.rs`). Accepting joins `partner`'s text onto
    /// this mark with `partner`'s formatting, so no empty paragraph is left;
    /// rejecting gives both back.
    pub(crate) fn deleted_last_paragraph(
        &self,
        part_o: &str,
        po: &Paragraph,
        partner: (From, &str, &Paragraph),
    ) -> Result<String> {
        let src = self.src(From::Ours, part_o)?;
        let ppr = self.swapped_ppr(partner, (From::Ours, part_o, po))?;
        let pieces = Self::whole(po, From::Ours, Op::Removed);
        let content = self.content_xml(&pieces, part_o, Some(po), part_o, None)?;
        let start = &src[po.start_tag.clone()];
        Ok(element(start, &format!("{ppr}{content}")))
    }

    /// Marks a paragraph's mark as inserted or deleted, in raw paragraph XML
    /// (ours as it is, or a paragraph this writer already rewrote). The marker
    /// goes first in the mark's `w:rPr`, as the schema requires. A mark that is
    /// already a tracked insertion or deletion is left alone.
    pub(crate) fn add_mark(&self, part_o: &str, paragraph: &str, kind: &str) -> Result<String> {
        let info = self
            .info_o
            .part(part_o)
            .ok_or_else(|| cannot("unknown part"))?;
        let mut decls = String::new();
        for (prefix, uri) in &info.namespaces {
            let uri = xml::escape_attr(uri);
            if prefix.is_empty() {
                decls.push_str(&format!(r#" xmlns="{uri}""#));
            } else {
                decls.push_str(&format!(r#" xmlns:{prefix}="{uri}""#));
            }
        }
        let head = format!("<lineage-wrap{decls}>");
        let wrapped = format!("{head}{paragraph}</lineage-wrap>");
        let doc = xml::parse("paragraph", &wrapped)?;
        let p = doc
            .root_element()
            .first_element_child()
            .filter(|n| xml::is(*n, ns::W, "p"))
            .ok_or_else(|| cannot("not a paragraph"))?;
        let marker = format!("<{} {}/>", self.tag(kind), self.rev_attrs());
        let mut edits = Edits::new();
        match xml::child(p, ns::W, "pPr") {
            Some(ppr) => match xml::child(ppr, ns::W, "rPr") {
                Some(rpr) => {
                    let marked = rpr.children().any(|c| {
                        ["ins", "del", "moveFrom", "moveTo"]
                            .iter()
                            .any(|m| xml::is(c, ns::W, m))
                    });
                    if marked {
                        return Ok(paragraph.to_owned());
                    }
                    xml::prepend_child(&mut edits, &wrapped, rpr, &marker);
                }
                None => {
                    let rpr = format!("<{r}>{marker}</{r}>", r = self.tag("rPr"));
                    let before = ppr
                        .children()
                        .find(|c| xml::is(*c, ns::W, "sectPr") || xml::is(*c, ns::W, "pPrChange"));
                    match before {
                        Some(node) => edits.insert(node.range().start, rpr),
                        None => xml::append_child(&mut edits, &wrapped, ppr, &rpr),
                    }
                }
            },
            None => {
                let ppr = format!(
                    "<{p_}><{r}>{marker}</{r}></{p_}>",
                    p_ = self.tag("pPr"),
                    r = self.tag("rPr")
                );
                // Opens `<w:p/>` if needed.
                xml::prepend_child(&mut edits, &wrapped, p, &ppr);
            }
        }
        let out = edits.apply(&wrapped)?;
        Ok(out[head.len()..out.len() - "</lineage-wrap>".len()].to_owned())
    }

    /// One of ours' paragraphs rewritten into theirs, with every difference
    /// tracked: removed and added text, changed run and paragraph formatting.
    pub(crate) fn modified_paragraph(
        &self,
        part_o: &str,
        po: &Paragraph,
        part_t: &str,
        pt: &Paragraph,
    ) -> Result<String> {
        let src_o = self.src(From::Ours, part_o)?;
        let src_t = self.src(From::Theirs, part_t)?;
        let ppr = if po.props == pt.props {
            po.ppr
                .clone()
                .map(|s| src_o[s].to_owned())
                .unwrap_or_default()
        } else {
            let new = self
                .ppr_children(From::Theirs, part_t, pt, &["rPr", "sectPr", "pPrChange"])?
                .concat();
            let keep = self
                .ppr_children(From::Ours, part_o, po, &[])?
                .into_iter()
                .filter(|c| is_element(c, &self.w, "rPr") || is_element(c, &self.w, "sectPr"))
                .collect::<String>();
            let old = self
                .ppr_children(From::Ours, part_o, po, &["rPr", "sectPr", "pPrChange"])?
                .concat();
            let tag = self.tag("pPr");
            format!(
                "<{tag}>{new}{keep}<{c} {rev}><{tag}>{old}</{tag}></{c}></{tag}>",
                c = self.tag("pPrChange"),
                rev = self.rev_attrs()
            )
        };
        let d = diff_paragraphs(po, pt, Some(src_o), Some(src_t));
        let mut pieces: Vec<Piece> = Vec::new();
        let mut marker_cursor = 0;
        let flush_markers = |upto: usize, pieces: &mut Vec<Piece>, cursor: &mut usize| {
            while *cursor < upto.min(po.inlines.len()) {
                if matches!(po.inlines[*cursor], Inline::Marker(_)) {
                    pieces.push(Piece {
                        from: From::Ours,
                        op: Op::Marker,
                        inline: *cursor,
                        text: None,
                    });
                }
                *cursor += 1;
            }
        };
        let push = |pieces: &mut Vec<Piece>,
                    from: From,
                    op: Op,
                    inline: usize,
                    text: Option<&str>,
                    is_atom: bool| {
            if !is_atom
                && let Some(last) = pieces.last_mut()
                && last.from == from
                && last.op == op
                && last.inline == inline
                && last.text.is_some()
            {
                if let (Some(t), Some(add)) = (last.text.as_mut(), text) {
                    t.push_str(add);
                }
                return;
            }
            pieces.push(Piece {
                from,
                op,
                inline,
                text: if is_atom {
                    None
                } else {
                    text.map(str::to_owned)
                },
            });
        };
        for step in &d.steps {
            match *step {
                Step::Same {
                    a,
                    b,
                    format_changed,
                } => {
                    let ta = &d.a[a];
                    flush_markers(ta.inline, &mut pieces, &mut marker_cursor);
                    if format_changed {
                        let tb = &d.b[b];
                        push(
                            &mut pieces,
                            From::Theirs,
                            Op::Reformatted {
                                old_inline: ta.inline,
                            },
                            tb.inline,
                            Some(&tb.text),
                            tb.is_atom,
                        );
                    } else {
                        push(
                            &mut pieces,
                            From::Ours,
                            Op::Same,
                            ta.inline,
                            Some(&ta.text),
                            ta.is_atom,
                        );
                    }
                }
                Step::Removed { a } => {
                    let ta = &d.a[a];
                    flush_markers(ta.inline, &mut pieces, &mut marker_cursor);
                    push(
                        &mut pieces,
                        From::Ours,
                        Op::Removed,
                        ta.inline,
                        Some(&ta.text),
                        ta.is_atom,
                    );
                }
                Step::Added { b } => {
                    let tb = &d.b[b];
                    push(
                        &mut pieces,
                        From::Theirs,
                        Op::Added,
                        tb.inline,
                        Some(&tb.text),
                        tb.is_atom,
                    );
                }
            }
        }
        flush_markers(po.inlines.len(), &mut pieces, &mut marker_cursor);
        let content = self.content_xml(&pieces, part_o, Some(po), part_t, Some(pt))?;
        let start = &src_o[po.start_tag.clone()];
        Ok(element(start, &format!("{ppr}{content}")))
    }

    /// `w:trPr` (raw children) plus an `w:ins` or `w:del` marker.
    fn trpr_with(&self, from: From, part: &str, row: &Row, mark: &str) -> Result<String> {
        let mut inner = String::new();
        if let Some(span) = row.trpr.clone() {
            let src = self.src(from, part)?;
            let doc = self.doc(from, part)?;
            let node =
                node_at(doc, span.start).ok_or_else(|| cannot("row properties not found"))?;
            inner = children_except(src, node, &["ins", "del", "trPrChange"]).concat();
        }
        let trpr = self.tag("trPr");
        Ok(format!(
            "<{trpr}>{inner}<{m} {rev}/></{trpr}>",
            m = self.tag(mark),
            rev = self.rev_attrs()
        ))
    }

    fn row_node(&self, from: From, part: &str, row: &Row) -> Result<Node<'_, 'a>> {
        let doc = self.doc(from, part)?;
        node_at(doc, row.span.start).ok_or_else(|| cannot("row not found"))
    }

    /// A row from theirs, as a tracked insertion.
    pub(crate) fn inserted_row(&self, part_t: &str, row: &Row, part_o: &str) -> Result<String> {
        let src = self.src(From::Theirs, part_t)?;
        let node = self.row_node(From::Theirs, part_t, row)?;
        let prefix: String = xml::child(node, ns::W, "tblPrEx")
            .map(|n| src[n.range()].to_owned())
            .unwrap_or_default();
        let trpr = self.trpr_with(From::Theirs, part_t, row, "ins")?;
        let mut cells = String::new();
        for (cell, tc) in row
            .cells
            .iter()
            .zip(node.children().filter(|c| xml::is(*c, ns::W, "tc")))
        {
            let tcpr = xml::child(tc, ns::W, "tcPr")
                .map(|n| src[n.range()].to_owned())
                .unwrap_or_default();
            let mut inner = String::new();
            for block in &cell.blocks {
                match block {
                    Block::Paragraph(p) => {
                        inner.push_str(&self.inserted_paragraph(part_t, p, part_o)?)
                    }
                    _ => return Err(cannot("a table inside a new row")),
                }
            }
            let tag = self.tag("tc");
            cells.push_str(&format!("<{tag}>{tcpr}{inner}</{tag}>"));
        }
        let tag = self.tag("tr");
        Ok(format!("<{tag}>{prefix}{trpr}{cells}</{tag}>"))
    }

    /// One of ours' rows, as a tracked deletion.
    pub(crate) fn deleted_row(&self, part_o: &str, row: &Row) -> Result<String> {
        let src = self.src(From::Ours, part_o)?;
        let node = self.row_node(From::Ours, part_o, row)?;
        let start = &src[node.range().start..xml::start_tag_end(src, node)];
        let prefix: String = xml::child(node, ns::W, "tblPrEx")
            .map(|n| src[n.range()].to_owned())
            .unwrap_or_default();
        let trpr = self.trpr_with(From::Ours, part_o, row, "del")?;
        let mut cells = String::new();
        for (cell, tc) in row
            .cells
            .iter()
            .zip(node.children().filter(|c| xml::is(*c, ns::W, "tc")))
        {
            let tc_start = &src[tc.range().start..xml::start_tag_end(src, tc)];
            let tcpr = xml::child(tc, ns::W, "tcPr")
                .map(|n| src[n.range()].to_owned())
                .unwrap_or_default();
            let mut inner = String::new();
            let last = cell
                .blocks
                .iter()
                .rposition(|b| matches!(b, Block::Paragraph(_)));
            for (index, block) in cell.blocks.iter().enumerate() {
                match block {
                    Block::Paragraph(p) => {
                        inner.push_str(&self.deleted_paragraph(part_o, p, Some(index) == last)?);
                    }
                    Block::Table(t) => inner.push_str(&self.deleted_table(part_o, t)?),
                    _ => return Err(cannot("unexpected content in a cell")),
                }
            }
            cells.push_str(&element(tc_start, &format!("{tcpr}{inner}")));
        }
        Ok(element(start, &format!("{prefix}{trpr}{cells}")))
    }

    fn table_head(&self, from: From, part: &str, table: &Table) -> Result<(String, String)> {
        let src = self.src(from, part)?;
        let doc = self.doc(from, part)?;
        let node = node_at(doc, table.span.start).ok_or_else(|| cannot("table not found"))?;
        let start = src[node.range().start..xml::start_tag_end(src, node)].to_owned();
        let mut head = String::new();
        for name in ["tblPr", "tblGrid"] {
            if let Some(child) = xml::child(node, ns::W, name) {
                head.push_str(&rebuild_without(
                    src,
                    child,
                    &["tblPrChange", "tblGridChange"],
                ));
            }
        }
        Ok((start, head))
    }

    /// A whole table from theirs, as tracked row insertions.
    pub(crate) fn inserted_table(
        &self,
        part_t: &str,
        table: &Table,
        part_o: &str,
    ) -> Result<String> {
        let (_, head) = self.table_head(From::Theirs, part_t, table)?;
        let mut rows = String::new();
        for row in &table.rows {
            rows.push_str(&self.inserted_row(part_t, row, part_o)?);
        }
        let tag = self.tag("tbl");
        Ok(format!("<{tag}>{head}{rows}</{tag}>"))
    }

    /// One of ours' tables, as tracked row deletions.
    pub(crate) fn deleted_table(&self, part_o: &str, table: &Table) -> Result<String> {
        let (start, head) = self.table_head(From::Ours, part_o, table)?;
        let mut rows = String::new();
        for row in &table.rows {
            rows.push_str(&self.deleted_row(part_o, row)?);
        }
        Ok(element(&start, &format!("{head}{rows}")))
    }
}

pub(crate) fn cannot(detail: &str) -> Error {
    Error::CannotEdit(detail.to_owned())
}

/// The element starting at `start`.
pub(crate) fn node_at<'d, 'i>(
    doc: &'d roxmltree::Document<'i>,
    start: usize,
) -> Option<Node<'d, 'i>> {
    doc.descendants()
        .find(|n| n.is_element() && n.range().start == start)
}

/// Raw XML of the element's children, except the named W elements (text and
/// whitespace between children are dropped).
pub(crate) fn children_except<'i>(
    src: &'i str,
    node: Node<'_, 'i>,
    except: &[&str],
) -> Vec<&'i str> {
    node.children()
        .filter(|c| c.is_element())
        .filter(|c| {
            !(c.tag_name().namespace() == Some(ns::W) && except.contains(&c.tag_name().name()))
        })
        .map(|c| &src[c.range()])
        .collect()
}

/// The element with the named W children removed.
pub(crate) fn rebuild_without(src: &str, node: Node<'_, '_>, except: &[&str]) -> String {
    let start = &src[node.range().start..xml::start_tag_end(src, node)];
    if xml::is_self_closing(src, node) {
        return start.to_owned();
    }
    let inner = children_except(src, node, except).concat();
    format!("{start}{inner}</{}>", xml::qname(src, node))
}

/// An element from a start tag copied from the source and new content. A
/// self-closing start tag (`<w:p w14:paraId="1"/>`, as Word writes empty
/// paragraphs) is opened first, so the element is never closed twice.
pub(crate) fn element(start_tag: &str, inner: &str) -> String {
    let open = match start_tag.strip_suffix("/>") {
        Some(head) => format!("{}>", head.trim_end()),
        None => start_tag.to_owned(),
    };
    format!("{open}{inner}</{}>", qname_of(start_tag))
}

/// The qualified name in a start tag such as `<w:p w14:paraId="1">`.
pub(crate) fn qname_of(start_tag: &str) -> &str {
    let rest = start_tag.trim_start_matches('<');
    let end = rest
        .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
        .unwrap_or(rest.len());
    &rest[..end]
}

fn is_element(raw: &str, prefix: &str, name: &str) -> bool {
    let open = format!("<{prefix}:{name}");
    raw.starts_with(&open)
        && raw[open.len()..]
            .chars()
            .next()
            .is_some_and(|c| c == '>' || c == '/' || c.is_whitespace())
}
