// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Keeping each sequence's last paragraph mark where it belongs.
//!
//! The body of a document, and every table cell, must end with a paragraph,
//! and Word cannot delete that last paragraph mark. Deleting the last paragraph
//! naively leaves an empty paragraph behind when accepted, and adding a paragraph
//! after it leaves one behind when rejected. Both add blank lines (and sometimes
//! a page) that nobody asked for.
//!
//! So the final mark always stays with the sequence's original last paragraph,
//! and Word's own technique handles the rest:
//!
//! - **Added after the last paragraph.** The old last paragraph's mark is
//!   marked inserted, and the final added paragraph takes over the old final
//!   mark: not marked inserted, with the old paragraph's formatting kept in
//!   `w:pPrChange`. Accept: both paragraphs. Reject: exactly the old one.
//! - **Last paragraph deleted, something added in its place.** The last
//!   paragraph is rewritten into the final added one (a tracked rewrite), and
//!   any other additions go before it.
//! - **Last paragraph deleted, nothing added.** The paragraph before it gets
//!   its mark deleted, and the deleted last paragraph takes that paragraph's
//!   formatting (old formatting in `w:pPrChange`). Accept: the paragraph before
//!   simply ends the sequence. Reject: both paragraphs, as they were.
//!
//! When none of these fit (the paragraph before is a table, already has
//! tracked changes, or is rewritten), the old behaviour stays: the last
//! paragraph is emptied, never removed.

use super::Writer;
use super::emit::{From, cannot};
use crate::diff::Path;
use crate::docx::{Block, Paragraph, StoryKey};
use crate::error::Result;
use crate::merge::locate::sequence_at;

/// A deleted paragraph that ends its sequence.
pub(crate) struct LastDeleted {
    pub part: String,
    pub story: StoryKey,
    pub path: Path,
    pub para: Paragraph,
    pub edit: usize,
}

/// A paragraph inserted from theirs.
pub(crate) struct Inserted {
    pub part: String,
    pub theirs_part: String,
    pub at: usize,
    pub edit: usize,
    pub para: Paragraph,
    /// The ours paragraph it was inserted after, when that paragraph ends its sequence.
    pub anchor: Option<Paragraph>,
}

impl Writer<'_> {
    /// True when the block at `path` is a paragraph that ends its sequence:
    /// nothing after it, or only the body's final section properties.
    pub(crate) fn ends_sequence(&self, story: &StoryKey, path: &Path) -> bool {
        let Some(blocks) = sequence_at(self.ours, story, path) else {
            return false;
        };
        let index = path
            .cells
            .last()
            .and_then(|s| s.block)
            .unwrap_or(path.block);
        matches!(blocks.get(index), Some(Block::Paragraph(_)))
            && match blocks.get(index + 1) {
                None => true,
                Some(Block::SectionBreak(s)) => s.is_final,
                Some(_) => false,
            }
    }

    /// The paragraph before a deleted last paragraph that can take over the
    /// sequence's end: without tracked changes, locks or anything inserted
    /// between them. Deleted paragraphs in between are skipped. Returns the
    /// paragraph, and where its formatting will come from: itself, or the
    /// master's paragraph it is being rewritten into.
    fn partner(&self, last: &LastDeleted) -> Option<(Paragraph, Option<(String, Paragraph)>)> {
        let blocks = sequence_at(self.ours, &last.story, &last.path)?;
        let index = last
            .path
            .cells
            .last()
            .and_then(|s| s.block)
            .unwrap_or(last.path.block);
        let deleted = self.deleted.borrow();
        let rewrites = self.rewrites.borrow();
        let edits = self.edits.borrow();
        let part_edits = edits.get(&last.part)?;
        for k in (0..index).rev() {
            let Block::Paragraph(p) = &blocks[k] else {
                return None;
            };
            let is_deleted = deleted
                .iter()
                .any(|(part, start)| *part == last.part && *start == p.span.start);
            if is_deleted {
                continue;
            }
            if p.has_revisions || p.lock.is_some() {
                return None;
            }
            let rewrite = rewrites
                .iter()
                .find(|(part, start, _, _)| *part == last.part && *start == p.span.start)
                .map(|(_, _, theirs_part, theirs)| (theirs_part.clone(), theirs.clone()));
            if rewrite.is_none() && part_edits.find_replacement(&p.span).is_some() {
                return None;
            }
            let between = p.span.end..=last.para.span.start;
            if part_edits.inserts_within(&between) {
                return None;
            }
            return Some((p.clone(), rewrite));
        }
        None
    }

    /// Where the run of deleted paragraphs ending at a deleted last paragraph
    /// begins: the end of the nearest block before it that stays, or the start
    /// of the sequence. Anything inserted from there on sits just before the
    /// last paragraph once the deletions are accepted.
    fn deleted_run_start(&self, last: &LastDeleted) -> Option<usize> {
        let blocks = sequence_at(self.ours, &last.story, &last.path)?;
        let index = last
            .path
            .cells
            .last()
            .and_then(|s| s.block)
            .unwrap_or(last.path.block);
        let deleted = self.deleted.borrow();
        for k in (0..index).rev() {
            let block = &blocks[k];
            let is_deleted = matches!(block, Block::Paragraph(p)
                if deleted.iter().any(|(part, start)| *part == last.part && *start == p.span.start));
            if !is_deleted {
                return Some(block.span().end);
            }
        }
        blocks.first().map(|b| b.span().start)
    }

    /// Rearranges the end of each sequence the writer touched (see the module
    /// documentation). Runs once, after every change has been written and before
    /// the edits are applied.
    pub(crate) fn settle_sequence_ends(&self) -> Result<()> {
        let last_deleted = std::mem::take(&mut *self.last_deleted.borrow_mut());
        let inserted = std::mem::take(&mut *self.inserted.borrow_mut());
        let mut handled_anchors: Vec<(String, usize)> = Vec::new();

        for last in &last_deleted {
            let start = last.para.span.start;
            let after: Vec<&Inserted> = inserted
                .iter()
                .filter(|i| {
                    i.part == last.part && i.anchor.as_ref().is_some_and(|a| a.span.start == start)
                })
                .collect();
            let run_start = self.deleted_run_start(last).unwrap_or(start);
            let mut before: Vec<&Inserted> = inserted
                .iter()
                .filter(|i| {
                    i.part == last.part && i.anchor.is_none() && i.at >= run_start && i.at <= start
                })
                .collect();
            // Output order: by position, then in the order they were added.
            before.sort_by_key(|i| (i.at, i.edit));
            if let Some(final_one) = after.last().or(before.last()) {
                // Rewrite the last paragraph into the final addition; any other
                // additions after it move in front of it.
                let rewritten = self.modified_paragraph(
                    &last.part,
                    &last.para,
                    &final_one.theirs_part,
                    &final_one.para,
                )?;
                let mut edits = self.edits.borrow_mut();
                let part_edits = edits
                    .get_mut(&last.part)
                    .ok_or_else(|| cannot("no edits"))?;
                for other in after.iter().take(after.len().saturating_sub(1)) {
                    if let Some(edit) = part_edits.get_mut(other.edit) {
                        edit.range = start..start;
                    }
                }
                if let Some(edit) = part_edits.get_mut(final_one.edit) {
                    edit.text.clear();
                }
                if let Some(edit) = part_edits.get_mut(last.edit) {
                    edit.text = rewritten;
                }
                handled_anchors.push((last.part.clone(), start));
                continue;
            }
            if let Some((partner, rewrite)) = self.partner(last) {
                let source = match &rewrite {
                    Some((theirs_part, theirs)) => (From::Theirs, theirs_part.as_str(), theirs),
                    None => (From::Ours, last.part.as_str(), &partner),
                };
                let text = self.deleted_last_paragraph(&last.part, &last.para, source)?;
                if let Some(edit) = self
                    .edits
                    .borrow_mut()
                    .get_mut(&last.part)
                    .and_then(|e| e.get_mut(last.edit))
                {
                    edit.text = text;
                }
                self.marks
                    .borrow_mut()
                    .push((last.part.clone(), partner, "del"));
            }
        }

        // Additions after a last paragraph that stays.
        let mut anchors: Vec<(String, Paragraph)> = Vec::new();
        for item in &inserted {
            if let Some(anchor) = &item.anchor {
                let key = (item.part.clone(), anchor.span.start);
                if !handled_anchors.contains(&key)
                    && !anchors
                        .iter()
                        .any(|(p, a)| *p == key.0 && a.span.start == key.1)
                {
                    anchors.push((item.part.clone(), anchor.clone()));
                }
            }
        }
        for (part, anchor) in anchors {
            let Some(final_one) = inserted.iter().rfind(|i| {
                i.part == part
                    && i.anchor
                        .as_ref()
                        .is_some_and(|a| a.span.start == anchor.span.start)
            }) else {
                continue;
            };
            let text = self.final_inserted_paragraph(
                &final_one.theirs_part,
                &final_one.para,
                &part,
                &anchor,
            )?;
            if let Some(edit) = self
                .edits
                .borrow_mut()
                .get_mut(&part)
                .and_then(|e| e.get_mut(final_one.edit))
            {
                edit.text = text;
            }
            self.marks.borrow_mut().push((part, anchor, "ins"));
        }

        // Paragraph marks that change: in the paragraph's rewrite if it has one,
        // else as a rewrite of the paragraph as it is.
        let marks = std::mem::take(&mut *self.marks.borrow_mut());
        for (part, para, kind) in marks {
            let existing = self
                .edits
                .borrow()
                .get(&part)
                .and_then(|e| e.find_replacement(&para.span));
            match existing {
                Some(index) => {
                    let current = self
                        .edits
                        .borrow()
                        .get(&part)
                        .and_then(|e| e.list_text(index))
                        .unwrap_or_default();
                    let marked = self.add_mark(&part, &current, kind)?;
                    if let Some(edit) = self
                        .edits
                        .borrow_mut()
                        .get_mut(&part)
                        .and_then(|e| e.get_mut(index))
                    {
                        edit.text = marked;
                    }
                }
                None => {
                    let src = self.ours.part_text(&part)?;
                    let marked = self.add_mark(&part, &src[para.span.clone()], kind)?;
                    self.edit(&part, para.span.clone(), marked);
                }
            }
        }
        Ok(())
    }
}
