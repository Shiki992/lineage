// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The canonical model of a deck: slides → shapes → text.
//!
//! Like the Word model, it is for comparing and describing, never for writing:
//! every slide and shape keeps its part name and byte span, and writers splice
//! into those spans. Keys are FNV-1a hashes of a canonical form of the XML, which
//! leaves out what PowerPoint rewrites on its own (spell-check and language
//! marks, shape ids) and replaces relationship ids with what they point at.

use std::ops::Range;

/// A byte range in a part's text.
pub type Span = Range<usize>;

/// Position and size in EMU, as written in `a:xfrm` / `p:xfrm`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Xfrm {
    pub x: i64,
    pub y: i64,
    pub cx: i64,
    pub cy: i64,
}

impl Xfrm {
    /// The area two rectangles share, in EMU², saturating.
    #[must_use]
    pub fn overlap(&self, other: &Self) -> i128 {
        let left = self.x.max(other.x);
        let right = self
            .x
            .saturating_add(self.cx)
            .min(other.x.saturating_add(other.cx));
        let top = self.y.max(other.y);
        let bottom = self
            .y
            .saturating_add(self.cy)
            .min(other.y.saturating_add(other.cy));
        if right <= left || bottom <= top {
            return 0;
        }
        i128::from(right - left) * i128::from(bottom - top)
    }

    /// The rectangle's area in EMU².
    #[must_use]
    pub fn area(&self) -> i128 {
        i128::from(self.cx.max(0)) * i128::from(self.cy.max(0))
    }
}

/// What a shape is, as far as merging and the panel care.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    /// A shape with text: a placeholder or a text box (`p:sp` with `p:txBody`).
    Text,
    /// A shape without text: a rectangle, arrow, line (`p:sp`, `p:cxnSp`).
    Drawing,
    Picture,
    Table,
    Chart,
    /// SmartArt (`dgm`): several parts that move together.
    Diagram,
    /// Video or audio.
    Media,
    /// An embedded or linked object (OLE).
    Ole,
    Group,
    /// Anything else (`mc:AlternateContent`, ink, unknown elements). Compared by
    /// its XML, never edited.
    Other,
}

impl ShapeKind {
    /// How the panel names one: "the chart", "the picture".
    #[must_use]
    pub fn noun(self) -> &'static str {
        match self {
            Self::Text => "the text",
            Self::Drawing => "a shape",
            Self::Picture => "the picture",
            Self::Table => "the table",
            Self::Chart => "the chart",
            Self::Diagram => "the SmartArt",
            Self::Media => "the video or audio",
            Self::Ole => "the embedded object",
            Self::Group => "the group",
            Self::Other => "an object",
        }
    }

    /// True for kinds the engine copies between decks in this version.
    #[must_use]
    pub fn is_writable(self) -> bool {
        matches!(
            self,
            Self::Text | Self::Drawing | Self::Picture | Self::Table | Self::Chart | Self::Group
        )
    }
}

/// A placeholder's type and index (`p:ph`). A missing type means "body" for
/// content placeholders; both are kept as written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Placeholder {
    pub kind: Option<String>,
    pub idx: Option<u32>,
}

impl Placeholder {
    /// True for the title placeholders (`title`, `ctrTitle`).
    #[must_use]
    pub fn is_title(&self) -> bool {
        matches!(self.kind.as_deref(), Some("title" | "ctrTitle"))
    }
}

/// One paragraph of text: its indent level and what a person reads.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Para {
    pub level: u8,
    pub text: String,
}

/// One shape on a slide (a direct child of the slide's shape tree).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shape {
    /// `p:cNvPr@id`, unique within the slide. 0 when the shape has none.
    pub id: u32,
    pub name: String,
    pub kind: ShapeKind,
    pub placeholder: Option<Placeholder>,
    pub geometry: Option<Xfrm>,
    /// The text, paragraph by paragraph. Tables give one paragraph per row,
    /// groups their members' text.
    pub paras: Vec<Para>,
    /// Text and object: equal when a person would say nothing changed but
    /// position, size or formatting.
    pub content: u64,
    /// Everything about the shape (ids and noise left out).
    pub full: u64,
    /// The element's byte span in the slide part.
    pub span: Span,
    /// Shape ids of the members, for groups; empty otherwise.
    pub members: Vec<u32>,
}

impl Shape {
    /// The shape's text, paragraphs joined by new lines.
    #[must_use]
    pub fn text(&self) -> String {
        self.paras
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// A slide's layout: the part, its name and its type.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct LayoutRef {
    pub part: String,
    pub name: String,
    pub kind: String,
}

/// A slide's speaker notes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notes {
    pub part: String,
    pub paras: Vec<Para>,
    /// The notes body's canonical key (text and formatting).
    pub key: u64,
    /// The span of the notes body placeholder's `p:txBody` in the notes part.
    pub body_span: Option<Span>,
}

impl Notes {
    #[must_use]
    pub fn text(&self) -> String {
        self.paras
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// One slide, in show order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slide {
    pub part: String,
    /// The slide's `r:id` in the presentation's relationships.
    pub rel_id: String,
    /// `p:sldId@id`: unique in the deck, kept when slides are reordered.
    pub sld_id: u32,
    /// `p14:creationId`, which PowerPoint gives every new slide.
    pub creation_id: Option<u32>,
    /// Hidden from the slide show (`show="0"`).
    pub hidden: bool,
    pub layout: Option<LayoutRef>,
    pub shapes: Vec<Shape>,
    pub notes: Option<Notes>,
    /// Slide-level settings: visibility, background, transition, colour mapping
    /// and layout. Equal when none of them changed.
    pub settings: u64,
    /// The animations (`p:timing`), 0 when there are none.
    pub timing: u64,
    /// Shape ids the animations refer to.
    pub animated: Vec<u32>,
    /// Everything on the slide (shapes, settings, notes, animations).
    pub full: u64,
    /// The `p:spTree` element's span, and where new shapes go (before its end tag).
    pub tree_span: Span,
    pub tree_end: usize,
}

impl Slide {
    /// The title placeholder's text, if the slide has one with text.
    #[must_use]
    pub fn title(&self) -> Option<String> {
        self.shapes
            .iter()
            .find(|s| s.placeholder.as_ref().is_some_and(Placeholder::is_title))
            .map(Shape::text)
            .map(|t| t.split('\n').next().unwrap_or_default().trim().to_owned())
            .filter(|t| !t.is_empty())
    }

    /// Every shape's text, in order, for matching slides by similarity.
    #[must_use]
    pub fn text(&self) -> String {
        self.shapes
            .iter()
            .map(Shape::text)
            .filter(|t| !t.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The shape with this id.
    #[must_use]
    pub fn shape(&self, id: u32) -> Option<&Shape> {
        self.shapes.iter().find(|s| s.id == id)
    }
}

/// A named section of slides (`p14:sectionLst`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Section {
    pub name: String,
    pub slides: Vec<u32>,
}
