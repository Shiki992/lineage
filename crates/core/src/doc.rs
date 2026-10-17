// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! One way in for every format: a [`Doc`] is a Word document or a deck, and
//! the functions here pick the right engine. Family logic (`crates/lineage`)
//! calls only these, so it never needs to know which format a family is.

use crate::clean::CleanReport;
use crate::diff::SegmentKind;
use crate::diff::spec::ChangeList;
use crate::docx::StoryKey;
use crate::error::{Error, Result};
use crate::merge::{Decision, Status};
use crate::package::Package;
use crate::package::props::LineageIds;
use crate::pptx::{self, Deck, Kind, kind_of};
use crate::view::{Theirs, WhatChanged};
use crate::write::{Bring, Revision};
use crate::{Docx, View};

/// An opened file of a format Lineage reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Doc {
    Word(Docx),
    Deck(Deck),
}

/// True for the formats this version reads and writes.
#[must_use]
pub fn readable(format: &str) -> bool {
    matches!(format, "docx" | "pptx")
}

impl Doc {
    /// Opens a file, telling a Word document from a deck by its main part.
    ///
    /// # Errors
    /// [`Error`] when the bytes are neither, or can't be read.
    pub fn open(bytes: &[u8]) -> Result<Self> {
        match kind_of(bytes)? {
            Kind::Word => Ok(Self::Word(Docx::open(bytes)?)),
            Kind::PowerPoint => Ok(Self::Deck(Deck::open(bytes)?)),
        }
    }

    /// Opens a file that must be of `format` ("docx" or "pptx").
    ///
    /// # Errors
    /// [`Error::NotAWordDocument`] or [`Error::NotADeck`] for the other kind;
    /// [`Error::Unsupported`] for a format this version doesn't read.
    pub fn open_as(bytes: &[u8], format: &str) -> Result<Self> {
        match format {
            "docx" => Ok(Self::Word(Docx::open(bytes)?)),
            "pptx" => Ok(Self::Deck(Deck::open(bytes)?)),
            other => Err(Error::Unsupported(format!("{other} files"))),
        }
    }

    /// The family format: "docx" or "pptx".
    #[must_use]
    pub fn format(&self) -> &'static str {
        match self {
            Self::Word(_) => "docx",
            Self::Deck(_) => "pptx",
        }
    }

    #[must_use]
    pub fn package(&self) -> &Package {
        match self {
            Self::Word(d) => &d.package,
            Self::Deck(d) => &d.package,
        }
    }

    /// The text a person reads, one line per paragraph: the body of a document,
    /// every shape of every slide (notes left out) of a deck. With a structure
    /// key: paragraph styles, or slide layouts and shape kinds.
    #[must_use]
    pub fn text_and_structure(&self) -> (Vec<String>, u64) {
        let mut structure = crate::hash::Fnv::new();
        match self {
            Self::Word(docx) => {
                fn walk(
                    blocks: &[crate::docx::Block],
                    lines: &mut Vec<String>,
                    h: &mut crate::hash::Fnv,
                ) {
                    for block in blocks {
                        match block {
                            crate::docx::Block::Paragraph(p) => {
                                h.str(p.props.style.as_deref().unwrap_or(""));
                                lines.push(p.text());
                            }
                            crate::docx::Block::Table(t) => {
                                h.str("table");
                                for row in &t.rows {
                                    for cell in &row.cells {
                                        walk(&cell.blocks, lines, h);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                let mut lines = Vec::new();
                walk(&docx.body().blocks, &mut lines, &mut structure);
                (lines, structure.finish())
            }
            Self::Deck(deck) => {
                let mut lines = Vec::new();
                for slide in &deck.slides {
                    structure.str(slide.layout.as_ref().map_or("", |l| l.kind.as_str()));
                    for shape in &slide.shapes {
                        structure.str(&format!("{:?}", shape.kind));
                        lines.extend(shape.paras.iter().map(|p| p.text.clone()));
                    }
                }
                (lines, structure.finish())
            }
        }
    }
}

fn same_kind<'d>(a: &'d Doc, b: &'d Doc) -> Result<()> {
    if a.format() == b.format() {
        Ok(())
    } else {
        Err(Error::CannotEdit(
            "a document and a deck can't be compared".to_owned(),
        ))
    }
}

/// "What changed" from `before` to `after`.
///
/// # Errors
/// When the two are not the same kind of file.
pub fn what_changed(before: &Doc, after: &Doc) -> Result<WhatChanged> {
    same_kind(before, after)?;
    Ok(match (before, after) {
        (Doc::Word(a), Doc::Word(b)) => crate::view::from_diff(&crate::diff::diff(a, b), b),
        (Doc::Deck(a), Doc::Deck(b)) => pptx::view::from_diff(&pptx::diff::diff(a, b)),
        _ => unreachable!("checked above"),
    })
}

/// The change list from `before` to `after`, with summaries.
///
/// # Errors
/// When the two are not the same kind of file.
pub fn change_list(before: &Doc, after: &Doc) -> Result<ChangeList> {
    same_kind(before, after)?;
    Ok(match (before, after) {
        (Doc::Word(a), Doc::Word(b)) => {
            let d = crate::diff::diff(a, b);
            crate::diff::spec::change_list(&d.changes, |c| {
                Some(crate::summary::summarize(c, &b.styles, Some(b)))
            })
        }
        (Doc::Deck(a), Doc::Deck(b)) => pptx::view::change_list(&pptx::diff::diff(a, b), a, b),
        _ => unreachable!("checked above"),
    })
}

/// True when the engine finds any change between two files of one format. For
/// documents both views are compared (every revision accepted, and rejected),
/// because Word re-saves files in its own way.
///
/// # Errors
/// When either file can't be read as `format`.
pub fn changed(a: &[u8], b: &[u8], format: &str) -> Result<bool> {
    match format {
        "docx" => {
            for view in [View::Accepted, View::Rejected] {
                let (da, db) = (Docx::open_view(a, view)?, Docx::open_view(b, view)?);
                if !crate::diff::diff(&da, &db).changes.is_empty() {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        "pptx" => {
            let (da, db) = (Deck::open(a)?, Deck::open(b)?);
            Ok(!pptx::diff::diff(&da, &db).changes.is_empty())
        }
        other => Err(Error::Unsupported(format!("{other} files"))),
    }
}

/// Words removed and added from `a` to `b` in what a person reads (the body,
/// or the slides), formatting ignored.
#[must_use]
pub fn changed_words(a: &Doc, b: &Doc) -> (usize, usize) {
    let count = |text: &str| {
        text.split(|c: char| !c.is_alphanumeric())
            .filter(|w| !w.is_empty())
            .count()
    };
    let (mut removed, mut added) = (0, 0);
    let mut tally = |segments: &[crate::diff::Segment]| {
        for segment in segments {
            match segment.kind {
                SegmentKind::Removed => removed += count(&segment.text),
                SegmentKind::Added => added += count(&segment.text),
                SegmentKind::Same | SegmentKind::Reformatted => {}
            }
        }
    };
    match (a, b) {
        (Doc::Word(x), Doc::Word(y)) => {
            for change in crate::diff::diff(x, y)
                .changes
                .iter()
                .filter(|c| c.story == StoryKey::Body)
            {
                tally(&change.segments);
            }
        }
        (Doc::Deck(x), Doc::Deck(y)) => {
            for change in pptx::diff::diff(x, y)
                .changes
                .iter()
                .filter(|c| c.unit != pptx::diff::Unit::Notes)
            {
                tally(&change.segments);
            }
        }
        _ => {
            let (la, _) = a.text_and_structure();
            let (lb, _) = b.text_and_structure();
            removed = la.iter().map(|l| count(l)).sum();
            added = lb.iter().map(|l| count(l)).sum();
        }
    }
    (removed, added)
}

/// A merge of either format.
#[derive(Clone, Debug)]
pub enum Plan<'d> {
    Word(crate::merge::Plan<'d>),
    Deck(pptx::merge::Plan<'d>),
}

/// Merges `theirs` into `ours`, both made from `base`.
///
/// # Errors
/// When the three are not the same kind of file.
pub fn plan<'d>(base: &'d Doc, ours: &'d Doc, theirs: &'d Doc) -> Result<Plan<'d>> {
    same_kind(base, ours)?;
    same_kind(base, theirs)?;
    Ok(match (base, ours, theirs) {
        (Doc::Word(b), Doc::Word(o), Doc::Word(t)) => Plan::Word(crate::merge::plan(b, o, t)),
        (Doc::Deck(b), Doc::Deck(o), Doc::Deck(t)) => Plan::Deck(pptx::merge::plan(b, o, t)),
        _ => unreachable!("checked above"),
    })
}

impl Plan<'_> {
    /// The cards for `theirs`' changes.
    #[must_use]
    pub fn view(&self, theirs: &Theirs) -> WhatChanged {
        match self {
            Self::Word(p) => crate::view::from_plan_for(p, theirs),
            Self::Deck(p) => pptx::view::from_plan_for(p, theirs),
        }
    }

    /// The status of the change with this id, if it is in the plan.
    #[must_use]
    pub fn status(&self, id: &str) -> Option<&Status> {
        match self {
            Self::Word(p) => p.item(id).map(|i| &i.status),
            Self::Deck(p) => p.item(id).map(|i| &i.status),
        }
    }

    /// The choices offered for the change with this id.
    #[must_use]
    pub fn choices(&self, id: &str) -> Vec<Decision> {
        match self {
            Self::Word(p) => p
                .item(id)
                .map(crate::merge::Item::choices)
                .unwrap_or_default(),
            Self::Deck(p) => p
                .item(id)
                .map(pptx::merge::Item::choices)
                .unwrap_or_default(),
        }
    }

    /// Every change id in the plan, in order.
    #[must_use]
    pub fn ids(&self) -> Vec<String> {
        match self {
            Self::Word(p) => p.items.iter().map(|i| i.change.id.clone()).collect(),
            Self::Deck(p) => p.items.iter().map(|i| i.change.id.clone()).collect(),
        }
    }

    /// Writes the decided changes: tracked changes by Lineage in a document,
    /// plain edits in a deck.
    ///
    /// # Errors
    /// [`Error::CannotEdit`] when a decision doesn't fit, or it can't be
    /// written safely. Nothing is written in that case.
    pub fn apply(&self, decisions: &[(String, Decision)], revision: &Revision) -> Result<Vec<u8>> {
        self.apply_with(decisions, revision, Bring::Everything)
    }

    /// [`Plan::apply`], bringing in only the words where `bring` says so
    /// (documents only: a deck's text keeps its own formatting).
    ///
    /// # Errors
    /// As [`Plan::apply`].
    pub fn apply_with(
        &self,
        decisions: &[(String, Decision)],
        revision: &Revision,
        bring: Bring,
    ) -> Result<Vec<u8>> {
        match self {
            Self::Word(p) => crate::write::apply_with(p, decisions, revision, bring),
            Self::Deck(p) => pptx::write::apply(p, decisions),
        }
    }
}

/// A clean copy for sending, of either format.
///
/// # Errors
/// When the file can't be read or cleaned safely.
pub fn clean_copy(bytes: &[u8], ids: Option<&LineageIds>) -> Result<(Vec<u8>, CleanReport)> {
    match kind_of(bytes)? {
        Kind::Word => crate::clean::clean_copy(bytes, ids),
        Kind::PowerPoint => pptx::clean::clean_copy(bytes, ids),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_formats_open_and_compare() {
        let deck = lineage_testkit::decks::pitch_deck().to_bytes();
        let edited =
            lineage_testkit::decks::edits::traction_update(lineage_testkit::decks::pitch_deck())
                .to_bytes();
        let (a, b) = (
            Doc::open(&deck).expect("opens"),
            Doc::open(&edited).expect("opens"),
        );
        assert_eq!(a.format(), "pptx");
        assert_eq!(
            what_changed(&a, &b).expect("compares").headline,
            "2 changes"
        );
        assert!(changed(&deck, &edited, "pptx").expect("reads"));
        assert!(!changed(&deck, &deck, "pptx").expect("reads"));
        assert_eq!(change_list(&a, &b).expect("lists").format, "pptx");
        let docx = crate::testutil::fixture("headings-paragraphs.docx").to_bytes();
        let word = Doc::open(&docx).expect("opens");
        assert_eq!(word.format(), "docx");
        assert!(what_changed(&word, &a).is_err());
        assert!(matches!(
            Doc::open_as(&docx, "pptx"),
            Err(Error::NotADeck(_))
        ));
        let (removed, added) = changed_words(&a, &b);
        assert!(removed > 0 && added > 0);
    }
}
