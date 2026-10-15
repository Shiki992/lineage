// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Writing chosen changes into a document as real tracked changes (decision 0003).
//!
//! Every change is attributed to the author in [`Revision`] (the engine uses
//! "Lineage") so it shows in Word's own review tools. Only the paragraphs, rows
//! and tables a change touches are rewritten; everything else is copied through
//! byte for byte. [`Check`] says in advance whether a change can be written, so
//! the panel never offers a button that would fail.
//!
//! [`Bring::WordsOnly`] writes only the words of a change, in this file's own
//! formatting: for files that came back saved by another app (decision 0014).

pub mod context;
pub mod emit;
mod ends;

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use context::{DocInfo, prefixes_compatible, rel_target};
use emit::{Like, Look, cannot, deletable};

use crate::diff::inline::{Step, diff_paragraphs};
use crate::diff::{ByHand, Change, ChangeKind, DocDiff, Path, Unit};
use crate::docx::{Block, Docx, Inline, Paragraph, StoryKey, Table};
use crate::error::{Error, Result};
use crate::merge::locate::{
    InsertPoint, Mapped, block_at, insert_point, map_path, sequence_at, source_of,
};
use crate::merge::{Decision, Item, OverlapKind, Plan, Status};
use crate::package::rels_part_for;
use crate::xml::{self, Edits, escape_attr, ns};

/// What a change brought in brings with it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Bring {
    /// The words and their formatting, as theirs has them.
    #[default]
    Everything,
    /// The words only, in this file's formatting (decision 0014):
    /// - an edited paragraph keeps its formatting, and added words take a
    ///   neighbouring run's;
    /// - a new paragraph takes the formatting of the nearest paragraph of theirs
    ///   formatted alike, as this file has it (theirs' own if there is none);
    /// - a moved paragraph keeps its formatting.
    ///
    /// Formatting-only changes are still written whole: keeping one asks for
    /// that formatting.
    WordsOnly,
}

/// Who made a tracked change, and when (an ISO 8601 UTC time, such as
/// `2026-10-15T09:30:00Z`). The host supplies the time: the engine has no clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revision {
    pub author: String,
    pub date: String,
}

impl Revision {
    /// A revision by "Lineage" at `date`.
    #[must_use]
    pub fn lineage(date: &str) -> Self {
        Self {
            author: "Lineage".to_owned(),
            date: date.to_owned(),
        }
    }

    fn validate(&self) -> Result<()> {
        let d = self.date.as_bytes();
        let ok = d.len() == 20
            && d[4] == b'-'
            && d[7] == b'-'
            && d[10] == b'T'
            && d[13] == b':'
            && d[16] == b':'
            && d[19] == b'Z'
            && d.iter()
                .enumerate()
                .all(|(i, c)| matches!(i, 4 | 7 | 10 | 13 | 16 | 19) || c.is_ascii_digit());
        if ok && !self.author.is_empty() {
            Ok(())
        } else {
            Err(Error::CannotEdit(
                "revision author or date is invalid".to_owned(),
            ))
        }
    }
}

/// Checks whether changes can be written, using each part's namespaces and
/// relationships.
pub struct Check<'d> {
    ours: &'d Docx,
    theirs: &'d Docx,
    info_o: DocInfo,
    info_t: DocInfo,
}

fn story_part<'d>(docx: &'d Docx, story: &StoryKey) -> Option<&'d str> {
    docx.story(story).map(|s| s.part.as_str())
}

impl<'d> Check<'d> {
    #[must_use]
    pub fn new(ours: &'d Docx, theirs: &'d Docx) -> Self {
        Self {
            ours,
            theirs,
            info_o: DocInfo::new(ours),
            info_t: DocInfo::new(theirs),
        }
    }

    fn parts(&self, story: &StoryKey) -> std::result::Result<(&'d str, &'d str), ByHand> {
        let part_o = story_part(self.ours, story).ok_or(ByHand::Story)?;
        let part_t = story_part(self.theirs, story).ok_or(ByHand::Story)?;
        let (io, it) = (
            self.info_o.part(part_o).ok_or(ByHand::Markup)?,
            self.info_t.part(part_t).ok_or(ByHand::Markup)?,
        );
        match (io.w_prefix(), it.w_prefix()) {
            (Some(a), Some(b)) if a == b => Ok((part_o, part_t)),
            _ => Err(ByHand::Markup),
        }
    }

    /// Can theirs' block be copied into ours' part?
    fn copyable(
        &self,
        block: &Block,
        part_t: &str,
        part_o: &str,
    ) -> std::result::Result<(), ByHand> {
        match block {
            Block::Paragraph(p) => self.copyable_paragraph(p, part_t, part_o),
            Block::Table(t) => {
                if t.lock.is_some() {
                    return Err(ByHand::Locked);
                }
                for row in &t.rows {
                    for cell in &row.cells {
                        for block in &cell.blocks {
                            match block {
                                Block::Paragraph(p) => {
                                    self.copyable_paragraph(p, part_t, part_o)?
                                }
                                _ => return Err(ByHand::TableStructure),
                            }
                        }
                    }
                }
                let src = self.theirs.part_text(part_t).map_err(|_| ByHand::Markup)?;
                self.prefixes_ok(&src[t.span.clone()], part_t, part_o)
            }
            Block::SectionBreak(_) => Err(ByHand::Section),
            Block::Opaque(_) => Err(ByHand::Opaque),
        }
    }

    fn prefixes_ok(
        &self,
        fragment: &str,
        part_t: &str,
        part_o: &str,
    ) -> std::result::Result<(), ByHand> {
        let (Some(it), Some(io)) = (self.info_t.part(part_t), self.info_o.part(part_o)) else {
            return Err(ByHand::Markup);
        };
        if prefixes_compatible(fragment, it, io) {
            Ok(())
        } else {
            Err(ByHand::Markup)
        }
    }

    fn copyable_paragraph(
        &self,
        p: &Paragraph,
        part_t: &str,
        part_o: &str,
    ) -> std::result::Result<(), ByHand> {
        if p.lock.is_some() {
            return Err(ByHand::Locked);
        }
        let (Some(it), Some(io)) = (self.info_t.part(part_t), self.info_o.part(part_o)) else {
            return Err(ByHand::Markup);
        };
        for inline in &p.inlines {
            if let Inline::Atom(atom) = inline {
                if atom.kind == crate::docx::AtomKind::NoteReference {
                    return Err(ByHand::Note);
                }
                for id in &atom.rel_ids {
                    let theirs = rel_target(part_t, &it.rels, id);
                    let ours = rel_target(part_o, &io.rels, id);
                    let same = match (&theirs, &ours) {
                        (Some(t), Some(o)) if t == o => {
                            t.1 || self.theirs.package.part(&t.2) == self.ours.package.part(&o.2)
                        }
                        _ => false,
                    };
                    if !same {
                        return Err(ByHand::NewPart);
                    }
                }
            }
        }
        for container in &p.containers {
            for id in &container.rel_ids {
                match rel_target(part_t, &it.rels, id) {
                    // External hyperlinks are fine: the writer adds the relationship.
                    Some((_, true, _)) => {}
                    Some(t) => {
                        if rel_target(part_o, &io.rels, id) != Some(t) {
                            return Err(ByHand::NewPart);
                        }
                    }
                    None => return Err(ByHand::NewPart),
                }
            }
        }
        // The writer never copies theirs' own `w:p` start tag (a rewrite keeps
        // ours', an insertion writes a bare one), so only what follows it must
        // use prefixes that mean the same in ours. Found in Phase 3: a Google
        // Docs export puts `w14:paraId` on every paragraph, and a version
        // that doesn't declare `w14` had every change marked "by hand".
        let src = self.theirs.part_text(part_t).map_err(|_| ByHand::Markup)?;
        self.prefixes_ok(&src[p.start_tag.end..p.span.end], part_t, part_o)
    }

    /// Can ours' block be deleted as a tracked change?
    fn deletable(block: &Block) -> std::result::Result<(), ByHand> {
        match block {
            Block::Paragraph(p) => {
                if p.has_revisions {
                    return Err(ByHand::Revisions);
                }
                if p.lock.is_some() {
                    return Err(ByHand::Locked);
                }
                for inline in &p.inlines {
                    if let Inline::Atom(atom) = inline
                        && !deletable(atom)
                    {
                        return Err(ByHand::Opaque);
                    }
                }
                Ok(())
            }
            Block::Table(t) => {
                if t.has_revisions {
                    return Err(ByHand::Revisions);
                }
                if t.lock.is_some() {
                    return Err(ByHand::Locked);
                }
                for row in &t.rows {
                    for cell in &row.cells {
                        for block in &cell.blocks {
                            Self::deletable(block)?;
                        }
                    }
                }
                Ok(())
            }
            Block::SectionBreak(_) => Err(ByHand::Section),
            Block::Opaque(_) => Err(ByHand::Opaque),
        }
    }

    /// Can ours' paragraph be rewritten into theirs?
    fn rewritable(
        &self,
        po: &Paragraph,
        pt: &Paragraph,
        part_o: &str,
        part_t: &str,
    ) -> std::result::Result<(), ByHand> {
        if po.has_revisions {
            return Err(ByHand::Revisions);
        }
        if po.lock.is_some() {
            return Err(ByHand::Locked);
        }
        self.copyable_paragraph(pt, part_t, part_o)?;
        let src_o = self.ours.part_text(part_o).ok();
        let src_t = self.theirs.part_text(part_t).ok();
        let d = diff_paragraphs(po, pt, src_o, src_t);
        for step in &d.steps {
            if let Step::Removed { a } = step
                && d.a[*a].is_atom
                && let Inline::Atom(atom) = &po.inlines[d.a[*a].inline]
                && !deletable(atom)
            {
                return Err(ByHand::Opaque);
            }
        }
        Ok(())
    }

    fn ours_same(
        &self,
        ours_diff: &DocDiff,
        change: &Change,
    ) -> std::result::Result<&'d Block, ByHand> {
        let before = change.before.as_ref().ok_or(ByHand::Placement)?;
        match map_path(ours_diff.story(&change.story), before) {
            Mapped::Same(path) => {
                block_at(self.ours, &change.story, &path).ok_or(ByHand::Placement)
            }
            _ => Err(ByHand::Placement),
        }
    }

    fn ours_at(
        &self,
        ours_diff: &DocDiff,
        story: &StoryKey,
        base: &Path,
    ) -> std::result::Result<&'d Block, ByHand> {
        let mapped = map_path(ours_diff.story(story), base);
        let path = mapped.path().ok_or(ByHand::Placement)?;
        block_at(self.ours, story, path).ok_or(ByHand::Placement)
    }

    fn insertable_at(
        &self,
        ours_diff: &DocDiff,
        story: &StoryKey,
        gap: Option<&Path>,
    ) -> std::result::Result<(), ByHand> {
        match insert_point(ours_diff.story(story), gap).ok_or(ByHand::Placement)? {
            InsertPoint::After(path) => match block_at(self.ours, story, &path) {
                Some(Block::Paragraph(p)) if p.lock.is_some() => Err(ByHand::Locked),
                Some(Block::Table(t)) if t.lock.is_some() => Err(ByHand::Locked),
                Some(Block::Opaque(_)) => Err(ByHand::Opaque),
                Some(_) => Ok(()),
                None => Err(ByHand::Placement),
            },
            InsertPoint::Start(_) => Ok(()),
        }
    }

    fn theirs_after(&self, change: &Change) -> std::result::Result<&'d Block, ByHand> {
        let after = change.after.as_ref().ok_or(ByHand::Placement)?;
        block_at(self.theirs, &change.story, after).ok_or(ByHand::Placement)
    }

    /// Whether a clean change can be brought in.
    ///
    /// # Errors
    /// The reason it must be brought in by hand.
    pub fn change(&self, ours_diff: &DocDiff, change: &Change) -> std::result::Result<(), ByHand> {
        if let Some(reason) = change.by_hand {
            return Err(reason);
        }
        if !change.story.is_writable() {
            return Err(ByHand::Note);
        }
        let (part_o, part_t) = self.parts(&change.story)?;
        if change.unit == Unit::Row {
            return self.row_change(ours_diff, change, part_o, part_t);
        }
        match change.kind {
            ChangeKind::Insert => {
                self.copyable(self.theirs_after(change)?, part_t, part_o)?;
                self.insertable_at(ours_diff, &change.story, change.gap.as_ref())
            }
            ChangeKind::Delete => Self::deletable(self.ours_same(ours_diff, change)?),
            ChangeKind::Move => {
                Self::deletable(self.ours_same(ours_diff, change)?)?;
                self.copyable(self.theirs_after(change)?, part_t, part_o)?;
                self.insertable_at(ours_diff, &change.story, change.gap.as_ref())
            }
            ChangeKind::Modify | ChangeKind::Format => match (
                self.ours_same(ours_diff, change)?,
                self.theirs_after(change)?,
            ) {
                (Block::Paragraph(po), Block::Paragraph(pt)) => {
                    self.rewritable(po, pt, part_o, part_t)
                }
                _ => Err(ByHand::Opaque),
            },
        }
    }

    fn row_change(
        &self,
        ours_diff: &DocDiff,
        change: &Change,
        part_o: &str,
        part_t: &str,
    ) -> std::result::Result<(), ByHand> {
        match change.kind {
            ChangeKind::Insert => {
                let after = change.after.as_ref().ok_or(ByHand::Placement)?;
                let (table_t, row_t) = split_row(after).ok_or(ByHand::Placement)?;
                let Some(Block::Table(tt)) = block_at(self.theirs, &change.story, &table_t) else {
                    return Err(ByHand::Placement);
                };
                let row = tt.rows.get(row_t).ok_or(ByHand::Placement)?;
                for cell in &row.cells {
                    for block in &cell.blocks {
                        match block {
                            Block::Paragraph(p) => self.copyable_paragraph(p, part_t, part_o)?,
                            _ => return Err(ByHand::TableStructure),
                        }
                    }
                }
                let gap = change.gap.as_ref().ok_or(ByHand::Placement)?;
                let (table_base, _) = split_row(gap).unwrap_or((gap.clone(), 0));
                let Block::Table(to) = self.ours_at(ours_diff, &change.story, &table_base)? else {
                    return Err(ByHand::Placement);
                };
                let span = |t: &Table, r: &crate::docx::Row| {
                    r.cells.iter().map(|c| c.grid_span).sum::<usize>() == t.columns
                };
                if !span(to, row) || to.lock.is_some() {
                    return Err(ByHand::TableStructure);
                }
                Ok(())
            }
            ChangeKind::Delete => {
                let before = change.before.as_ref().ok_or(ByHand::Placement)?;
                let Mapped::Same(path) = map_path(ours_diff.story(&change.story), before) else {
                    return Err(ByHand::Placement);
                };
                let (table, row) = split_row(&path).ok_or(ByHand::Placement)?;
                let Some(Block::Table(t)) = block_at(self.ours, &change.story, &table) else {
                    return Err(ByHand::Placement);
                };
                if t.lock.is_some() {
                    return Err(ByHand::Locked);
                }
                let row = t.rows.get(row).ok_or(ByHand::Placement)?;
                if row.revision.is_some() {
                    return Err(ByHand::Revisions);
                }
                for cell in &row.cells {
                    for block in &cell.blocks {
                        Self::deletable(block)?;
                    }
                }
                Ok(())
            }
            _ => Err(ByHand::TableStructure),
        }
    }

    /// Whether "use the master's version" can be written for an overlap.
    #[must_use]
    pub fn masters(&self, ours_diff: &DocDiff, item: &Item) -> bool {
        let change = &item.change;
        let Status::Overlap(kind) = item.status else {
            return false;
        };
        if change.unit != Unit::Paragraph || !change.story.is_writable() {
            return false;
        }
        let Ok((part_o, part_t)) = self.parts(&change.story) else {
            return false;
        };
        let ours_block = || {
            change
                .before
                .as_ref()
                .and_then(|b| self.ours_at(ours_diff, &change.story, b).ok())
        };
        match kind {
            OverlapKind::BothEdited | OverlapKind::FormattingVsText => {
                matches!((ours_block(), self.theirs_after(change).ok()),
                    (Some(Block::Paragraph(po)), Some(Block::Paragraph(pt))) if self.rewritable(po, pt, part_o, part_t).is_ok())
            }
            OverlapKind::DeletedVsEdited => {
                ours_block().is_some_and(|b| Self::deletable(b).is_ok())
            }
            OverlapKind::EditedVsDeleted => {
                self.theirs_after(change)
                    .is_ok_and(|b| self.copyable(b, part_t, part_o).is_ok())
                    && self
                        .insertable_at(ours_diff, &change.story, change.before.as_ref())
                        .is_ok()
            }
            OverlapKind::BothAdded => {
                self.theirs_after(change)
                    .is_ok_and(|b| self.copyable(b, part_t, part_o).is_ok())
                    && self
                        .insertable_at(ours_diff, &change.story, change.gap.as_ref())
                        .is_ok()
            }
        }
    }
}

/// A row path (last step without a column) split into the table and the row.
fn split_row(path: &Path) -> Option<(Path, usize)> {
    let last = path.cells.last()?;
    if last.column.is_some() {
        return None;
    }
    let mut table = path.clone();
    let row = last.row;
    table.cells.pop();
    Some((table, row))
}

/// Writes tracked changes into ours.
pub(crate) struct Writer<'a> {
    pub ours: &'a Docx,
    pub theirs: &'a Docx,
    rev: Revision,
    bring: Bring,
    /// Ours' prefix for WordprocessingML.
    pub w: String,
    next_id: Cell<u64>,
    docs_o: BTreeMap<String, roxmltree::Document<'a>>,
    docs_t: BTreeMap<String, roxmltree::Document<'a>>,
    info_o: DocInfo,
    info_t: DocInfo,
    edits: RefCell<BTreeMap<String, Edits>>,
    /// External hyperlinks to add, per ours part: (new id, target).
    new_rels: RefCell<BTreeMap<String, Vec<(String, String)>>>,
    /// Deleted paragraphs of ours, as (part, start of span).
    deleted: RefCell<Vec<(String, usize)>>,
    /// Deleted paragraphs that end their sequence (see `ends.rs`).
    last_deleted: RefCell<Vec<ends::LastDeleted>>,
    /// Paragraphs inserted from theirs (see `ends.rs`).
    inserted: RefCell<Vec<ends::Inserted>>,
    /// Paragraphs of ours rewritten into theirs.
    rewrites: RefCell<Vec<ends::Rewrite>>,
    /// Paragraph marks of ours to mark inserted or deleted: (part, paragraph, kind).
    marks: RefCell<Vec<(String, Paragraph, &'static str)>>,
}

fn parse_stories(docx: &Docx) -> Result<BTreeMap<String, roxmltree::Document<'_>>> {
    let mut docs = BTreeMap::new();
    for story in &docx.stories {
        if docs.contains_key(&story.part) || !story.key.is_writable() {
            continue;
        }
        let text = docx.part_text(&story.part)?;
        docs.insert(story.part.clone(), xml::parse(&story.part, text)?);
    }
    Ok(docs)
}

/// Highest numeric `w:id` in any part that holds annotations.
fn max_annotation_id(docx: &Docx) -> u64 {
    let mut max = 0;
    for name in docx.package.part_names() {
        if !(name.starts_with("word/") && name.ends_with(".xml")) {
            continue;
        }
        let Ok(text) = docx.part_text(name) else {
            continue;
        };
        if !text.contains(":id=") {
            continue;
        }
        let Ok(doc) = xml::parse(name, text) else {
            continue;
        };
        for node in doc.descendants().filter(roxmltree::Node::is_element) {
            if let Some(id) = node
                .attribute((ns::W, "id"))
                .and_then(|v| v.parse::<u64>().ok())
            {
                max = max.max(id);
            }
        }
    }
    max
}

impl<'a> Writer<'a> {
    pub(crate) fn new(
        ours: &'a Docx,
        theirs: &'a Docx,
        rev: &Revision,
        bring: Bring,
    ) -> Result<Self> {
        rev.validate()?;
        let info_o = DocInfo::new(ours);
        let w = info_o
            .part(&ours.main_part)
            .and_then(|i| i.w_prefix())
            .ok_or_else(|| cannot("no prefix for WordprocessingML"))?
            .to_owned();
        Ok(Self {
            ours,
            theirs,
            rev: rev.clone(),
            bring,
            w,
            next_id: Cell::new(max_annotation_id(ours) + 1),
            docs_o: parse_stories(ours)?,
            docs_t: parse_stories(theirs)?,
            info_o,
            info_t: DocInfo::new(theirs),
            edits: RefCell::new(BTreeMap::new()),
            new_rels: RefCell::new(BTreeMap::new()),
            deleted: RefCell::new(Vec::new()),
            last_deleted: RefCell::new(Vec::new()),
            inserted: RefCell::new(Vec::new()),
            rewrites: RefCell::new(Vec::new()),
            marks: RefCell::new(Vec::new()),
        })
    }

    pub(crate) fn doc(&self, from: emit::From, part: &str) -> Result<&roxmltree::Document<'a>> {
        let docs = match from {
            emit::From::Ours => &self.docs_o,
            emit::From::Theirs => &self.docs_t,
        };
        docs.get(part).ok_or_else(|| cannot("part is not a story"))
    }

    /// `w:id`, `w:author` and `w:date` for one new revision.
    pub(crate) fn rev_attrs(&self) -> String {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        format!(
            r#"{w}:id="{id}" {w}:author="{}" {w}:date="{}""#,
            escape_attr(&self.rev.author),
            escape_attr(&self.rev.date),
            w = self.w
        )
    }

    /// Adds an edit to a part and returns its index there.
    fn edit(&self, part: &str, range: std::ops::Range<usize>, text: String) -> usize {
        let mut edits = self.edits.borrow_mut();
        let part_edits = edits.entry(part.to_owned()).or_default();
        part_edits.replace(range, text);
        part_edits.last_index().unwrap_or(0)
    }

    /// Rewrites `r:id` values in a start tag copied from theirs so they point at
    /// the same targets in ours, adding external hyperlink relationships.
    pub(crate) fn remap_rel_ids(
        &self,
        part_t: &str,
        part_o: &str,
        start_tag: &str,
    ) -> Result<String> {
        let (Some(it), Some(io)) = (self.info_t.part(part_t), self.info_o.part(part_o)) else {
            return Err(cannot("unknown part"));
        };
        let Some(r) = it.r_prefix() else {
            return Ok(start_tag.to_owned());
        };
        let mut out = start_tag.to_owned();
        for quote in ['"', '\''] {
            let needle = format!("{r}:id={quote}");
            let mut from = 0;
            while let Some(found) = out[from..].find(&needle) {
                let value_start = from + found + needle.len();
                let Some(len) = out[value_start..].find(quote) else {
                    break;
                };
                let id = out[value_start..value_start + len].to_owned();
                let new_id = self.ours_rel_for(part_t, part_o, &id, it, io)?;
                out.replace_range(value_start..value_start + len, &new_id);
                from = value_start + new_id.len();
            }
        }
        Ok(out)
    }

    fn ours_rel_for(
        &self,
        part_t: &str,
        part_o: &str,
        id: &str,
        it: &context::PartInfo,
        io: &context::PartInfo,
    ) -> Result<String> {
        let theirs =
            rel_target(part_t, &it.rels, id).ok_or_else(|| cannot("unknown relationship"))?;
        if rel_target(part_o, &io.rels, id).as_ref() == Some(&theirs) {
            return Ok(id.to_owned());
        }
        if let Some(rel) = io
            .rels
            .items
            .iter()
            .find(|rel| rel_target(part_o, &io.rels, &rel.id).as_ref() == Some(&theirs))
        {
            return Ok(rel.id.clone());
        }
        if !(theirs.1 && theirs.0 == ns::REL_HYPERLINK) {
            return Err(cannot("a relationship ours does not have"));
        }
        let mut new_rels = self.new_rels.borrow_mut();
        let added = new_rels.entry(part_o.to_owned()).or_default();
        if let Some((existing, _)) = added.iter().find(|(_, target)| *target == theirs.2) {
            return Ok(existing.clone());
        }
        let mut n = added.len() + 1;
        let new_id = loop {
            let candidate = format!("rIdLineage{n}");
            if io.rels.by_id(&candidate).is_none() && !added.iter().any(|(i, _)| *i == candidate) {
                break candidate;
            }
            n += 1;
        };
        added.push((new_id.clone(), theirs.2));
        Ok(new_id)
    }

    fn part_o(&self, story: &StoryKey) -> Result<&'a str> {
        story_part(self.ours, story).ok_or_else(|| cannot("story missing"))
    }

    fn part_t(&self, story: &StoryKey) -> Result<&'a str> {
        story_part(self.theirs, story).ok_or_else(|| cannot("story missing"))
    }

    /// Byte offset in ours where content goes for an insert point.
    fn offset(&self, story: &StoryKey, point: &InsertPoint) -> Result<usize> {
        let part = self.part_o(story)?;
        match point {
            InsertPoint::After(path) => {
                let block =
                    block_at(self.ours, story, path).ok_or_else(|| cannot("block not found"))?;
                match block {
                    Block::SectionBreak(s) if s.is_final => Ok(s.span.start),
                    Block::SectionBreak(_) => {
                        // A paragraph's section break: insert after that paragraph.
                        let blocks = sequence_at(self.ours, story, path)
                            .ok_or_else(|| cannot("sequence not found"))?;
                        let index = path
                            .cells
                            .last()
                            .and_then(|s| s.block)
                            .unwrap_or(path.block);
                        let previous = index
                            .checked_sub(1)
                            .and_then(|i| blocks.get(i))
                            .ok_or_else(|| cannot("no paragraph"))?;
                        Ok(previous.span().end)
                    }
                    other => Ok(other.span().end),
                }
            }
            InsertPoint::Start(None) => {
                let blocks = &self
                    .ours
                    .story(story)
                    .ok_or_else(|| cannot("story missing"))?
                    .blocks;
                match blocks.first() {
                    Some(first) => Ok(first.span().start),
                    None => {
                        let doc = self.doc(emit::From::Ours, part)?;
                        let root = doc.root_element();
                        let container = xml::child(root, ns::W, "body").unwrap_or(root);
                        Ok(xml::start_tag_end(self.ours.part_text(part)?, container))
                    }
                }
            }
            InsertPoint::Start(Some(cell_path)) => {
                let mut table_path = cell_path.clone();
                // What is left is the path to the table (inside an enclosing cell
                // for nested tables).
                let step = table_path.cells.pop().ok_or_else(|| cannot("not a cell"))?;
                let Some(Block::Table(table)) = block_at(self.ours, story, &table_path) else {
                    return Err(cannot("table not found"));
                };
                let cell = table
                    .rows
                    .get(step.row)
                    .and_then(|r| r.cells.get(step.column.unwrap_or(0)))
                    .ok_or_else(|| cannot("cell not found"))?;
                let doc = self.doc(emit::From::Ours, part)?;
                let node =
                    emit::node_at(doc, cell.span.start).ok_or_else(|| cannot("cell not found"))?;
                let src = self.ours.part_text(part)?;
                Ok(xml::child(node, ns::W, "tcPr")
                    .map_or_else(|| xml::start_tag_end(src, node), |p| p.range().end))
            }
        }
    }

    /// The XML for a block copied from theirs as a tracked insertion.
    fn inserted_block(
        &self,
        story: &StoryKey,
        after: &Path,
        like: Option<&Like>,
    ) -> Result<String> {
        let part_t = self.part_t(story)?;
        let part_o = self.part_o(story)?;
        match block_at(self.theirs, story, after).ok_or_else(|| cannot("block not found"))? {
            Block::Paragraph(p) => self.inserted_paragraph(part_t, p, part_o, like),
            Block::Table(t) => self.inserted_table(part_t, t, part_o),
            _ => Err(cannot("this block cannot be inserted")),
        }
    }

    /// Words only: the paragraph of ours whose formatting a paragraph new from
    /// theirs takes. That is the nearest paragraph of theirs with text that is
    /// formatted alike (the same paragraph formatting, else the same style and
    /// list or not) and came from the base and is still in ours. `None` (theirs'
    /// own formatting) in [`Bring::Everything`], for tables, or when none fits.
    fn like_for_insert(
        &self,
        plan: &Plan<'_>,
        story: &StoryKey,
        after_t: &Path,
    ) -> Result<Option<Like>> {
        if self.bring != Bring::WordsOnly {
            return Ok(None);
        }
        let (Some(Block::Paragraph(pt)), Some(blocks)) = (
            block_at(self.theirs, story, after_t),
            sequence_at(self.theirs, story, after_t),
        ) else {
            return Ok(None);
        };
        let index = after_t
            .cells
            .last()
            .and_then(|s| s.block)
            .unwrap_or(after_t.block);
        let path_to = |k: usize| {
            let mut path = after_t.clone();
            match path.cells.last_mut() {
                Some(last) => last.block = Some(k),
                None => path.block = k,
            }
            path
        };
        let same_props = |q: &Paragraph| q.props == pt.props;
        let same_kind = |q: &Paragraph| {
            q.props.style == pt.props.style
                && q.props.numbering.is_some() == pt.props.numbering.is_some()
        };
        let tiers: [&dyn Fn(&Paragraph) -> bool; 2] = [&same_props, &same_kind];
        for alike in tiers {
            for distance in 1..blocks.len() {
                for k in [index.checked_sub(distance), index.checked_add(distance)]
                    .into_iter()
                    .flatten()
                {
                    let Some(Block::Paragraph(q)) = blocks.get(k) else {
                        continue;
                    };
                    if q.text().trim().is_empty() || !alike(q) {
                        continue;
                    }
                    let Some(base) = source_of(plan.theirs_diff.story(story), &path_to(k)) else {
                        continue;
                    };
                    let mapped = map_path(plan.ours_diff.story(story), &base);
                    let Some(Block::Paragraph(po)) =
                        mapped.path().and_then(|p| block_at(self.ours, story, p))
                    else {
                        continue;
                    };
                    if po.text().trim().is_empty() {
                        continue;
                    }
                    return self
                        .like(self.part_o(story)?, po, self.part_t(story)?, q)
                        .map(Some);
                }
            }
        }
        Ok(None)
    }

    /// Words only: a moved paragraph keeps its own formatting in ours.
    fn like_for_move(
        &self,
        story: &StoryKey,
        ours_path: &Path,
        after_t: &Path,
    ) -> Result<Option<Like>> {
        if self.bring != Bring::WordsOnly {
            return Ok(None);
        }
        match (
            block_at(self.ours, story, ours_path),
            block_at(self.theirs, story, after_t),
        ) {
            (Some(Block::Paragraph(po)), Some(Block::Paragraph(pt))) => self
                .like(self.part_o(story)?, po, self.part_t(story)?, pt)
                .map(Some),
            _ => Ok(None),
        }
    }

    fn insert(
        &self,
        story: &StoryKey,
        point: &InsertPoint,
        after_t: &Path,
        like: Option<Like>,
    ) -> Result<()> {
        let at = self.offset(story, point)?;
        let xml = self.inserted_block(story, after_t, like.as_ref())?;
        let part = self.part_o(story)?;
        let edit = self.edit(part, at..at, xml);
        if let Some(Block::Paragraph(pt)) = block_at(self.theirs, story, after_t) {
            let anchor = match point {
                InsertPoint::After(path) if self.ends_sequence(story, path) => {
                    match block_at(self.ours, story, path) {
                        Some(Block::Paragraph(p)) if !p.has_revisions && p.lock.is_none() => {
                            Some(p.clone())
                        }
                        _ => None,
                    }
                }
                _ => None,
            };
            self.inserted.borrow_mut().push(ends::Inserted {
                part: part.to_owned(),
                theirs_part: self.part_t(story)?.to_owned(),
                at,
                edit,
                para: pt.clone(),
                anchor,
                like,
            });
        }
        Ok(())
    }

    /// True when the paragraph's mark must stay: it ends its sequence (Word
    /// cannot delete that mark) or carries a section break.
    fn keeps_mark(&self, story: &StoryKey, path: &Path) -> bool {
        let Some(blocks) = sequence_at(self.ours, story, path) else {
            return true;
        };
        let index = path
            .cells
            .last()
            .and_then(|s| s.block)
            .unwrap_or(path.block);
        // Word removes a deleted paragraph that a table follows; only the last
        // paragraph of a sequence (or one ending a section) keeps its mark.
        !matches!(
            blocks.get(index + 1),
            Some(Block::Paragraph(_) | Block::Table(_))
        )
    }

    fn delete(&self, story: &StoryKey, ours_path: &Path) -> Result<()> {
        let part = self.part_o(story)?;
        let block =
            block_at(self.ours, story, ours_path).ok_or_else(|| cannot("block not found"))?;
        match block {
            Block::Paragraph(p) => {
                let keep = self.keeps_mark(story, ours_path);
                let xml = self.deleted_paragraph(part, p, keep)?;
                let edit = self.edit(part, block.span(), xml);
                self.deleted
                    .borrow_mut()
                    .push((part.to_owned(), p.span.start));
                if keep && self.ends_sequence(story, ours_path) {
                    self.last_deleted.borrow_mut().push(ends::LastDeleted {
                        part: part.to_owned(),
                        story: story.clone(),
                        path: ours_path.clone(),
                        para: p.clone(),
                        edit,
                    });
                }
            }
            Block::Table(t) => {
                let xml = self.deleted_table(part, t)?;
                self.edit(part, block.span(), xml);
            }
            _ => return Err(cannot("this block cannot be deleted")),
        }
        Ok(())
    }

    /// Rewrites ours' paragraph into theirs. `words` keeps ours' formatting
    /// (in [`Bring::WordsOnly`]; a formatting-only change passes `false`).
    fn rewrite(
        &self,
        story: &StoryKey,
        ours_path: &Path,
        theirs_path: &Path,
        words: bool,
    ) -> Result<()> {
        let (part_o, part_t) = (self.part_o(story)?, self.part_t(story)?);
        let (Some(Block::Paragraph(po)), Some(Block::Paragraph(pt))) = (
            block_at(self.ours, story, ours_path),
            block_at(self.theirs, story, theirs_path),
        ) else {
            return Err(cannot("only paragraphs can be rewritten"));
        };
        let words = words && self.bring == Bring::WordsOnly;
        let look = if words { Look::Ours } else { Look::Theirs };
        let xml = self.modified_paragraph(part_o, po, part_t, pt, look)?;
        self.edit(part_o, po.span.clone(), xml);
        self.rewrites.borrow_mut().push(ends::Rewrite {
            part: part_o.to_owned(),
            start: po.span.start,
            formatting: (!words).then(|| (part_t.to_owned(), pt.clone())),
        });
        Ok(())
    }

    fn rows(&self, plan: &Plan<'_>, change: &Change) -> Result<()> {
        let story = &change.story;
        let part_o = self.part_o(story)?;
        match change.kind {
            ChangeKind::Insert => {
                let part_t = self.part_t(story)?;
                let after = change.after.as_ref().ok_or_else(|| cannot("no row"))?;
                let (table_t, row_t) = split_row(after).ok_or_else(|| cannot("no row"))?;
                let Some(Block::Table(tt)) = block_at(self.theirs, story, &table_t) else {
                    return Err(cannot("table not found"));
                };
                let row = tt.rows.get(row_t).ok_or_else(|| cannot("row not found"))?;
                let gap = change.gap.as_ref().ok_or_else(|| cannot("no place"))?;
                let mapped = map_path(plan.ours_diff.story(story), gap);
                let path = mapped.path().ok_or_else(|| cannot("row place not found"))?;
                let (table_o, after_row) = match split_row(path) {
                    Some((t, r)) => (t, Some(r)),
                    None => (path.clone(), None),
                };
                let Some(Block::Table(to)) = block_at(self.ours, story, &table_o) else {
                    return Err(cannot("table not found"));
                };
                let at = match after_row {
                    Some(r) => to.rows.get(r).map(|row| row.span.end),
                    None => to.rows.first().map(|row| row.span.start),
                }
                .ok_or_else(|| cannot("row not found"))?;
                let xml = self.inserted_row(part_t, row, part_o)?;
                self.edit(part_o, at..at, xml);
                Ok(())
            }
            ChangeKind::Delete => {
                let before = change.before.as_ref().ok_or_else(|| cannot("no row"))?;
                let Mapped::Same(path) = map_path(plan.ours_diff.story(story), before) else {
                    return Err(cannot("row changed"));
                };
                let (table, r) = split_row(&path).ok_or_else(|| cannot("no row"))?;
                let Some(Block::Table(t)) = block_at(self.ours, story, &table) else {
                    return Err(cannot("table not found"));
                };
                let row = t.rows.get(r).ok_or_else(|| cannot("row not found"))?;
                let xml = self.deleted_row(part_o, row)?;
                self.edit(part_o, row.span.clone(), xml);
                Ok(())
            }
            _ => Err(cannot("row formatting is brought in by hand")),
        }
    }

    fn mapped_same(plan: &Plan<'_>, change: &Change) -> Result<Path> {
        let before = change.before.as_ref().ok_or_else(|| cannot("no place"))?;
        match map_path(plan.ours_diff.story(&change.story), before) {
            Mapped::Same(path) => Ok(path),
            _ => Err(cannot("this version changed it")),
        }
    }

    fn mapped_any(plan: &Plan<'_>, change: &Change) -> Result<Path> {
        let before = change.before.as_ref().ok_or_else(|| cannot("no place"))?;
        map_path(plan.ours_diff.story(&change.story), before)
            .path()
            .cloned()
            .ok_or_else(|| cannot("not found"))
    }

    fn point(plan: &Plan<'_>, change: &Change, gap: Option<&Path>) -> Result<InsertPoint> {
        insert_point(plan.ours_diff.story(&change.story), gap)
            .ok_or_else(|| cannot("no place to insert"))
    }

    /// Brings a clean change in.
    pub(crate) fn bring_in(&self, plan: &Plan<'_>, change: &Change) -> Result<()> {
        if change.unit == Unit::Row {
            return self.rows(plan, change);
        }
        let story = &change.story;
        let after = || change.after.as_ref().ok_or_else(|| cannot("no content"));
        match change.kind {
            ChangeKind::Insert => self.insert(
                story,
                &Self::point(plan, change, change.gap.as_ref())?,
                after()?,
                self.like_for_insert(plan, story, after()?)?,
            ),
            ChangeKind::Delete => self.delete(story, &Self::mapped_same(plan, change)?),
            ChangeKind::Modify => {
                self.rewrite(story, &Self::mapped_same(plan, change)?, after()?, true)
            }
            ChangeKind::Format => {
                self.rewrite(story, &Self::mapped_same(plan, change)?, after()?, false)
            }
            ChangeKind::Move => {
                let ours_path = Self::mapped_same(plan, change)?;
                let like = self.like_for_move(story, &ours_path, after()?)?;
                self.delete(story, &ours_path)?;
                self.insert(
                    story,
                    &Self::point(plan, change, change.gap.as_ref())?,
                    after()?,
                    like,
                )
            }
        }
    }

    /// Writes the master's side of an overlap.
    pub(crate) fn use_masters(
        &self,
        plan: &Plan<'_>,
        item: &Item,
        decision: Decision,
    ) -> Result<()> {
        let change = &item.change;
        let story = &change.story;
        let Status::Overlap(kind) = item.status else {
            return Err(cannot("not an overlap"));
        };
        let after = || change.after.as_ref().ok_or_else(|| cannot("no content"));
        // Their side is the words unless they changed only the formatting.
        let words = change.kind != ChangeKind::Format;
        let like = || -> Result<Option<Like>> {
            if words {
                self.like_for_insert(plan, story, after()?)
            } else {
                Ok(None)
            }
        };
        match kind {
            OverlapKind::BothEdited | OverlapKind::FormattingVsText => {
                self.rewrite(story, &Self::mapped_any(plan, change)?, after()?, words)
            }
            OverlapKind::DeletedVsEdited => self.delete(story, &Self::mapped_any(plan, change)?),
            OverlapKind::EditedVsDeleted => self.insert(
                story,
                &Self::point(plan, change, change.before.as_ref())?,
                after()?,
                like()?,
            ),
            OverlapKind::BothAdded => {
                // This version's own additions in the same place.
                let mine: Vec<&Change> = plan
                    .ours_diff
                    .changes
                    .iter()
                    .filter(|o| {
                        o.kind == ChangeKind::Insert && o.story == *story && o.gap == change.gap
                    })
                    .collect();
                match decision {
                    Decision::KeepBoth {
                        masters_first: false,
                    } => {
                        let last = mine
                            .iter()
                            .filter_map(|o| o.after.clone())
                            .max()
                            .ok_or_else(|| cannot("no addition of mine"))?;
                        self.insert(story, &InsertPoint::After(last), after()?, like()?)
                    }
                    Decision::KeepBoth {
                        masters_first: true,
                    } => self.insert(
                        story,
                        &Self::point(plan, change, change.gap.as_ref())?,
                        after()?,
                        like()?,
                    ),
                    _ => {
                        for o in &mine {
                            if let Some(path) = &o.after {
                                self.delete(story, path)?;
                            }
                        }
                        self.insert(
                            story,
                            &Self::point(plan, change, change.gap.as_ref())?,
                            after()?,
                            like()?,
                        )
                    }
                }
            }
        }
    }

    /// Applies every edit and returns the new package bytes.
    pub(crate) fn finish(self) -> Result<Vec<u8>> {
        self.settle_sequence_ends()?;
        let mut package = self.ours.package.clone();
        for (part, edits) in self.edits.into_inner() {
            let text = self.ours.part_text(&part)?;
            let new = edits.apply(text)?;
            // The last line of defence: never hand back a part that is not
            // well-formed XML. Nothing is written in that case.
            xml::parse(&part, &new)
                .map_err(|_| cannot("the result would not be a valid document"))?;
            package.set_part(&part, new.into_bytes());
        }
        for (part, rels) in self.new_rels.into_inner() {
            add_relationships(&mut package, &part, &rels)?;
        }
        package.to_bytes()
    }
}

/// Adds external hyperlink relationships to a part's `.rels`, creating it if needed.
fn add_relationships(
    package: &mut crate::Package,
    part: &str,
    rels: &[(String, String)],
) -> Result<()> {
    let rels_part = rels_part_for(part);
    let fragment: String = rels
        .iter()
        .map(|(id, target)| {
            format!(
                r#"<Relationship Id="{}" Type="{}" Target="{}" TargetMode="External"/>"#,
                escape_attr(id),
                ns::REL_HYPERLINK,
                escape_attr(target)
            )
        })
        .collect();
    let new = match package.part(&rels_part) {
        Some(bytes) => {
            let text = xml::text(&rels_part, bytes)?;
            let doc = xml::parse(&rels_part, text)?;
            let mut edits = Edits::new();
            xml::append_child(&mut edits, text, doc.root_element(), &fragment);
            edits.apply(text)?
        }
        None => format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n<Relationships xmlns=\"{}\">{fragment}</Relationships>",
            ns::PACKAGE_RELS
        ),
    };
    package.set_part(&rels_part, new.into_bytes());
    Ok(())
}

/// Writes the decided items of a plan into ours as tracked changes, words and
/// formatting ([`Bring::Everything`]).
///
/// Items not mentioned in `decisions` are left out. Items that are already there
/// or must be brought in by hand are never written.
///
/// # Errors
/// [`Error::CannotEdit`] if a decision does not fit its item (bringing in an
/// overlap without choosing a side, or something to be brought in by hand), or
/// the result could not be written safely. Nothing is written in that case.
pub fn apply(plan: &Plan<'_>, decisions: &[(String, Decision)], rev: &Revision) -> Result<Vec<u8>> {
    apply_with(plan, decisions, rev, Bring::Everything)
}

/// [`apply`], bringing in what `bring` says: everything, or only the words.
///
/// # Errors
/// As [`apply`].
pub fn apply_with(
    plan: &Plan<'_>,
    decisions: &[(String, Decision)],
    rev: &Revision,
    bring: Bring,
) -> Result<Vec<u8>> {
    let writer = Writer::new(plan.ours, plan.theirs, rev, bring)?;
    let mut wrote = false;
    for item in &plan.items {
        let Some((_, decision)) = decisions.iter().find(|(id, _)| *id == item.change.id) else {
            continue;
        };
        match (&item.status, decision) {
            (_, Decision::Decline | Decision::KeepMine) | (Status::AlreadyThere { .. }, _) => {}
            (Status::Clean, Decision::BringIn | Decision::UseMasters) => {
                writer.bring_in(plan, &item.change)?;
                wrote = true;
            }
            (Status::Overlap(_), Decision::UseMasters | Decision::KeepBoth { .. }) => {
                writer.use_masters(plan, item, *decision)?;
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
    writer.finish()
}

#[cfg(test)]
mod tests;
