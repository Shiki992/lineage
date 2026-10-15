// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The canonical model of a Word document: stories → blocks → inlines.
//!
//! The model is for comparing and describing documents. It never replaces the
//! XML: every block, run and atom keeps its byte span in the part it came from,
//! and writers splice into those spans. Styles and numbering are referenced by id,
//! never flattened into the runs that use them.

use std::collections::BTreeMap;
use std::ops::Range;

/// A byte range in a part's text.
pub type Span = Range<usize>;

/// Which tracked changes a model shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum View {
    /// As if every tracked change were accepted. What the person sees with
    /// "No markup", and what the engine compares by default.
    #[default]
    Accepted,
    /// As if every tracked change were rejected: the document before them.
    Rejected,
}

/// Which story a block list belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum StoryKey {
    Body,
    /// A header, keyed by the section that references it and its kind
    /// (`default`, `first` or `even`), so it matches across documents even if
    /// Word renamed the part.
    Header {
        section: usize,
        kind: String,
    },
    Footer {
        section: usize,
        kind: String,
    },
    Footnotes,
    Endnotes,
}

impl StoryKey {
    /// True for stories whose changes the engine can write in this version.
    #[must_use]
    pub fn is_writable(&self) -> bool {
        matches!(self, Self::Body | Self::Header { .. } | Self::Footer { .. })
    }
}

/// One story: the main body, a header, a footer, or all footnotes or endnotes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Story {
    pub key: StoryKey,
    /// The part holding the story, such as `word/document.xml`.
    pub part: String,
    pub blocks: Vec<Block>,
}

/// A block of a story.
// Paragraphs are by far the most common block, so boxing them to shrink the
// rarer variants would cost more than it saves.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Block {
    Paragraph(Paragraph),
    Table(Table),
    /// Section properties. A paragraph's `w:sectPr` is a break after that
    /// paragraph; the body's own `w:sectPr` is the last section.
    SectionBreak(SectionBreak),
    /// Anything the engine does not model (unknown elements, `mc:AlternateContent`
    /// between paragraphs). Compared by its XML, never edited.
    Opaque(Opaque),
}

impl Block {
    /// The block's byte span in its part.
    #[must_use]
    pub fn span(&self) -> Span {
        match self {
            Self::Paragraph(p) => p.span.clone(),
            Self::Table(t) => t.span.clone(),
            Self::SectionBreak(s) => s.span.clone(),
            Self::Opaque(o) => o.span.clone(),
        }
    }

    /// Plain text, for summaries and similarity. Table cells are separated by
    /// tabs and rows by new lines.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Paragraph(p) => p.text(),
            Self::Table(t) => t.text(),
            Self::SectionBreak(_) => String::new(),
            Self::Opaque(o) => o.text.clone(),
        }
    }
}

/// A sorted map of property element → normalised attributes, so `<w:b/>` and
/// `<w:b w:val="true"/>` compare equal and attribute order never matters.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PropMap(pub BTreeMap<String, String>);

impl PropMap {
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.0.get(name).map(String::as_str)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A list reference: `w:numId` and `w:ilvl`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NumRef {
    pub num_id: String,
    pub level: String,
}

/// Paragraph properties, with the style and list referenced.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct ParaProps {
    pub style: Option<String>,
    pub numbering: Option<NumRef>,
    /// Everything else in `w:pPr`, normalised.
    pub direct: PropMap,
}

/// Run properties, with the character style referenced.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RunProps {
    pub style: Option<String>,
    pub direct: PropMap,
}

/// A tracked change around content, as read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Revision {
    Inserted,
    Deleted,
}

/// One `w:r` (or the run-like element a field or atom lives in).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub span: Span,
    /// The `w:rPr` element, copied as is when the run's text is re-emitted.
    pub rpr: Option<Span>,
    pub props: RunProps,
    /// The hyperlink or other wrapper this run sits in, if any.
    pub container: Option<usize>,
}

/// A wrapper around runs that the writer must re-create: a hyperlink, smart tag
/// or similar. Its start tag is copied as is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Container {
    pub span: Span,
    /// The start tag, such as `<w:hyperlink r:id="rId5" w:history="1">`.
    pub start_tag: Span,
    /// The qualified name, for the end tag.
    pub qname: String,
    /// Relationship ids the start tag refers to (`r:id`).
    pub rel_ids: Vec<String>,
}

/// Content that is never split: a picture, a whole field, a note reference, a
/// symbol, an inline content control, maths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AtomKind {
    Drawing,
    Field,
    NoteReference,
    Symbol,
    ContentControl,
    Math,
    PageBreak,
    ColumnBreak,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Atom {
    pub kind: AtomKind,
    /// Identity for comparison: a hash of what the atom shows or does.
    pub key: u64,
    /// Text a person would see, such as a field's result. Used in summaries only.
    pub display: String,
    /// Source span; the writer copies it as is.
    pub span: Span,
    /// The run whose properties the atom has, when it lives in one run.
    pub run: Option<usize>,
    /// The container the atom sits in.
    pub container: Option<usize>,
    /// Relationship ids the atom refers to (`r:embed`, `r:id`).
    pub rel_ids: Vec<String>,
}

/// Zero-width markers: bookmarks, comment ranges, proofing marks. Ignored when
/// comparing, kept where they were when a paragraph is rewritten.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub span: Span,
    /// True for comment ranges and references, which sent copies drop.
    pub comment: bool,
    pub container: Option<usize>,
}

/// One piece of a paragraph's content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Inline {
    /// Text from one run. Tabs, line breaks and special hyphens are characters.
    Text {
        text: String,
        run: usize,
    },
    Atom(Atom),
    Marker(Marker),
}

/// Why a paragraph must not be edited automatically.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lock {
    /// Inside a field that spans paragraphs, such as a table of contents.
    SpanningField,
    /// Inside a block content control or custom XML.
    ContentControl,
    /// Inside a footnote or endnote.
    Note,
}

/// A paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Paragraph {
    pub span: Span,
    /// The start tag, copied when the paragraph is rewritten.
    pub start_tag: Span,
    /// The `w:pPr` element.
    pub ppr: Option<Span>,
    /// Where the content after `w:pPr` starts and ends.
    pub content: Span,
    /// `w14:paraId`: a hint for matching, never trusted alone.
    pub anchor: Option<String>,
    pub props: ParaProps,
    pub runs: Vec<Run>,
    pub containers: Vec<Container>,
    pub inlines: Vec<Inline>,
    /// Existing tracked changes inside (content, paragraph mark or properties).
    pub has_revisions: bool,
    /// The paragraph mark is a tracked insertion or deletion.
    pub mark_revision: Option<Revision>,
    /// Further source paragraphs joined into this one because their marks are
    /// deleted (accepted view) or inserted (rejected view).
    pub joined: Vec<Span>,
    pub lock: Option<Lock>,
    /// The footnote or endnote this paragraph belongs to.
    pub note_id: Option<String>,
}

impl Paragraph {
    /// The paragraph's text, with atoms shown by their display text.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        for inline in &self.inlines {
            match inline {
                Inline::Text { text, .. } => out.push_str(text),
                Inline::Atom(atom) => out.push_str(&atom.display),
                Inline::Marker(_) => {}
            }
        }
        out
    }

    /// True when the paragraph has no visible content.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inlines.iter().all(|inline| match inline {
            Inline::Text { text, .. } => text.is_empty(),
            Inline::Atom(_) => false,
            Inline::Marker(_) => true,
        })
    }
}

/// A table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub span: Span,
    /// Hash of `w:tblPr` and `w:tblGrid`.
    pub props_key: u64,
    /// Number of grid columns.
    pub columns: usize,
    pub rows: Vec<Row>,
    pub has_revisions: bool,
    /// Set when the table holds structure the engine does not model, such as
    /// content controls around cells.
    pub lock: Option<Lock>,
}

impl Table {
    #[must_use]
    pub fn text(&self) -> String {
        self.rows
            .iter()
            .map(Row::text)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub span: Span,
    /// The `w:trPr` element.
    pub trpr: Option<Span>,
    pub props_key: u64,
    pub cells: Vec<Cell>,
    pub revision: Option<Revision>,
}

impl Row {
    #[must_use]
    pub fn text(&self) -> String {
        self.cells
            .iter()
            .map(Cell::text)
            .collect::<Vec<_>>()
            .join("\t")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub span: Span,
    /// Hash of `w:tcPr`, which holds widths, merges and shading.
    pub props_key: u64,
    /// `w:gridSpan`.
    pub grid_span: usize,
    /// `w:vMerge`: `Some(true)` restarts a vertical merge, `Some(false)` continues it.
    pub vmerge: Option<bool>,
    pub blocks: Vec<Block>,
}

impl Cell {
    #[must_use]
    pub fn text(&self) -> String {
        self.blocks
            .iter()
            .map(Block::text)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A header or footer reference in a section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderRef {
    pub footer: bool,
    pub kind: String,
    pub part: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionBreak {
    /// The `w:sectPr` element.
    pub span: Span,
    /// Everything but header and footer references, normalised.
    pub props: PropMap,
    pub headers: Vec<HeaderRef>,
    /// True for the body's final `w:sectPr`, false for one inside a paragraph.
    pub is_final: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opaque {
    pub span: Span,
    pub key: u64,
    /// Any text inside, for summaries.
    pub text: String,
}
