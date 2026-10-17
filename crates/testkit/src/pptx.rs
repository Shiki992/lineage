// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Builds complete .pptx packages from hand-written PresentationML, independently
//! of the engine. Element order follows ECMA-376 Part 1, which PowerPoint
//! enforces. Every deck has one slide master with five layouts, a theme, a notes
//! master, and the parts PowerPoint itself writes (presProps, viewProps,
//! tableStyles), so PowerPoint and LibreOffice open them without repair.
//!
//! A [`Deck`] is plain data: tests clone one and change slides, shapes or text to
//! make the "master" and "tailored version" of a scenario.

use crate::docx::{FIXED_DATE, esc};
use crate::zip_writer;

const DECL: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\r\n";
pub const NS_P: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
pub const NS_A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
pub const NS_R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
pub const NS_C: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
pub const NS_P14: &str = "http://schemas.microsoft.com/office/powerpoint/2010/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const CT_PML: &str = "application/vnd.openxmlformats-officedocument.presentationml";
const CT_XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet";
/// The extension URI PowerPoint uses for a slide's `p14:creationId`.
pub const CREATION_ID_EXT: &str = "{BB962C8B-B14F-4D97-AF65-F5344CB8AC3E}";
/// The extension URI PowerPoint uses for sections.
pub const SECTIONS_EXT: &str = "{521415D9-36F7-43E2-AB2F-B90AF26B5E84}";

/// Slide size: 16:9, in EMU.
pub const SLIDE_CX: i64 = 12_192_000;
pub const SLIDE_CY: i64 = 6_858_000;

/// The five layouts every generated deck has, in the master's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layout {
    Title,
    TitleAndContent,
    TwoContent,
    TitleOnly,
    Blank,
}

impl Layout {
    pub const ALL: [Self; 5] = [
        Self::Title,
        Self::TitleAndContent,
        Self::TwoContent,
        Self::TitleOnly,
        Self::Blank,
    ];

    fn number(self) -> usize {
        Self::ALL.iter().position(|l| *l == self).unwrap_or(0) + 1
    }

    /// The layout's `type` attribute.
    pub fn kind(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::TitleAndContent => "obj",
            Self::TwoContent => "twoObj",
            Self::TitleOnly => "titleOnly",
            Self::Blank => "blank",
        }
    }

    /// The layout's name, as PowerPoint shows it.
    pub fn name(self) -> &'static str {
        match self {
            Self::Title => "Title Slide",
            Self::TitleAndContent => "Title and Content",
            Self::TwoContent => "Two Content",
            Self::TitleOnly => "Title Only",
            Self::Blank => "Blank",
        }
    }
}

/// A run of text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    /// An external hyperlink on the run.
    pub link: Option<String>,
}

/// A paragraph: an indent level and runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Para {
    pub level: u8,
    pub runs: Vec<Run>,
}

/// A paragraph of plain text at level 0.
pub fn p(text: &str) -> Para {
    p_at(0, text)
}

/// A paragraph of plain text at an indent level.
pub fn p_at(level: u8, text: &str) -> Para {
    Para {
        level,
        runs: vec![Run {
            text: text.to_owned(),
            bold: false,
            italic: false,
            link: None,
        }],
    }
}

/// A paragraph made of several runs.
pub fn p_runs(runs: Vec<Run>) -> Para {
    Para { level: 0, runs }
}

/// A plain run.
pub fn r(text: &str) -> Run {
    Run {
        text: text.to_owned(),
        bold: false,
        italic: false,
        link: None,
    }
}

/// A bold run.
pub fn r_bold(text: &str) -> Run {
    Run {
        bold: true,
        ..r(text)
    }
}

/// A run linking to `url`.
pub fn r_link(text: &str, url: &str) -> Run {
    Run {
        link: Some(url.to_owned()),
        ..r(text)
    }
}

/// Position and size, in EMU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i64,
    pub y: i64,
    pub cx: i64,
    pub cy: i64,
}

/// A rectangle in EMU.
pub const fn rect(x: i64, y: i64, cx: i64, cy: i64) -> Rect {
    Rect { x, y, cx, cy }
}

/// A bar chart's data: categories and whole-number values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChartData {
    pub series: String,
    pub categories: Vec<String>,
    pub values: Vec<i64>,
}

/// One shape on a slide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Shape {
    /// The layout's title placeholder (`ctrTitle` on a title slide).
    Title(Vec<Para>),
    /// The title slide's subtitle placeholder.
    Subtitle(Vec<Para>),
    /// A content placeholder: `idx` 1, or 2 for the right side of two content.
    Body { idx: u32, paras: Vec<Para> },
    /// A free text box.
    TextBox {
        name: String,
        at: Rect,
        paras: Vec<Para>,
    },
    /// A picture of a generated PNG.
    Picture {
        name: String,
        descr: String,
        at: Rect,
        png: Vec<u8>,
    },
    /// A table of text cells.
    Table {
        name: String,
        at: Rect,
        rows: Vec<Vec<String>>,
    },
    /// A column chart with its data in an embedded workbook.
    Chart {
        name: String,
        at: Rect,
        data: ChartData,
    },
    /// A group of shapes.
    Group { name: String, shapes: Vec<Shape> },
    /// Raw shape XML, inserted as is (it must use the `p`/`a`/`r` prefixes).
    Raw(String),
}

/// A shape with an explicit `cNvPr` id; otherwise ids count up from 2.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placed {
    pub id: Option<u32>,
    pub shape: Shape,
}

impl From<Shape> for Placed {
    fn from(shape: Shape) -> Self {
        Self { id: None, shape }
    }
}

/// One slide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Slide {
    /// `p:sldId@id` (256 and up).
    pub id: u32,
    /// `p14:creationId`, which PowerPoint writes on every slide.
    pub creation_id: Option<u32>,
    /// The part's number: `ppt/slides/slide{file}.xml`. PowerPoint keeps a slide's
    /// file when slides are reordered, so tests set it to the slide's identity.
    pub file: u32,
    pub layout: Layout,
    pub hidden: bool,
    pub shapes: Vec<Placed>,
    /// Speaker notes, one string per paragraph.
    pub notes: Option<Vec<String>>,
    /// Legacy comments, as (author index into `Deck::authors`, text).
    pub comments: Vec<(usize, String)>,
    /// Raw `p:timing` (animations), inserted after `p:clrMapOvr`.
    pub timing: Option<String>,
}

impl Slide {
    /// A slide with this id, its file number equal to `id - 255`, and a creation
    /// id derived from `id`.
    pub fn new(id: u32, layout: Layout, shapes: Vec<Shape>) -> Self {
        Self {
            id,
            creation_id: Some(creation_id_for(id)),
            file: id - 255,
            layout,
            hidden: false,
            shapes: shapes.into_iter().map(Placed::from).collect(),
            notes: None,
            comments: Vec::new(),
            timing: None,
        }
    }

    pub fn with_notes(mut self, notes: &[&str]) -> Self {
        self.notes = Some(notes.iter().map(|n| (*n).to_owned()).collect());
        self
    }

    pub fn hidden(mut self) -> Self {
        self.hidden = true;
        self
    }

    /// The title placeholder's text, if any.
    pub fn title(&self) -> Option<String> {
        self.shapes.iter().find_map(|s| match &s.shape {
            Shape::Title(paras) => Some(
                paras
                    .iter()
                    .flat_map(|p| p.runs.iter().map(|r| r.text.as_str()))
                    .collect(),
            ),
            _ => None,
        })
    }
}

/// A deterministic, PowerPoint-looking creation id for a slide id.
pub fn creation_id_for(id: u32) -> u32 {
    // Nine or ten digits, as PowerPoint's are; any value works.
    1_000_000_000_u32.wrapping_add(id.wrapping_mul(7_919_u32))
}

/// How the zip is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zip {
    Stored,
    Deflated,
    Streamed,
}

/// One synthetic deck.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deck {
    pub title: String,
    pub slides: Vec<Slide>,
    /// Sections, as (name, slide ids).
    pub sections: Vec<(String, Vec<u32>)>,
    /// Comment authors, as (name, initials).
    pub authors: Vec<(String, String)>,
    /// Custom properties, as (name, value). Empty: no custom properties part.
    pub custom: Vec<(String, String)>,
    /// Writes an empty, self-closing `<Properties/>` custom part.
    pub empty_custom: bool,
    pub zip: Zip,
}

impl Deck {
    pub fn new(title: &str, slides: Vec<Slide>) -> Self {
        Self {
            title: title.to_owned(),
            slides,
            sections: Vec::new(),
            authors: Vec::new(),
            custom: Vec::new(),
            empty_custom: false,
            zip: Zip::Stored,
        }
    }

    /// The slide with this `p:sldId@id`.
    pub fn slide_mut(&mut self, id: u32) -> &mut Slide {
        self.slides
            .iter_mut()
            .find(|s| s.id == id)
            .unwrap_or_else(|| panic!("no slide {id}"))
    }

    /// The finished .pptx bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        let parts = self.parts();
        match self.zip {
            Zip::Stored => zip_writer::write_stored(&parts),
            Zip::Deflated => zip_writer::write_deflated(&parts, false),
            Zip::Streamed => zip_writer::write_deflated(&parts, true),
        }
    }

    /// The package parts, in a fixed order.
    pub fn parts(&self) -> Vec<(String, Vec<u8>)> {
        let mut out = Parts::default();
        out.override_type(
            "docProps/core.xml",
            "application/vnd.openxmlformats-package.core-properties+xml",
        );
        out.override_type(
            "docProps/app.xml",
            "application/vnd.openxmlformats-officedocument.extended-properties+xml",
        );
        out.override_type(
            "ppt/presentation.xml",
            &format!("{CT_PML}.presentation.main+xml"),
        );

        let mut root_rels = vec![
            rel("rId1", "officeDocument", "ppt/presentation.xml", false),
            (
                "rId2".to_owned(),
                "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties"
                    .to_owned(),
                "docProps/core.xml".to_owned(),
                false,
            ),
            rel("rId3", "extended-properties", "docProps/app.xml", false),
        ];
        if !self.custom.is_empty() || self.empty_custom {
            root_rels.push(rel(
                "rId4",
                "custom-properties",
                "docProps/custom.xml",
                false,
            ));
            out.override_type(
                "docProps/custom.xml",
                "application/vnd.openxmlformats-officedocument.custom-properties+xml",
            );
        }

        // Presentation relationships: master, slides, notes master, then the rest.
        let mut pres_rels = vec![rel(
            "rId1",
            "slideMaster",
            "slideMasters/slideMaster1.xml",
            false,
        )];
        let mut slide_rids = Vec::new();
        for (n, slide) in self.slides.iter().enumerate() {
            let rid = format!("rId{}", n + 2);
            pres_rels.push(rel(
                &rid,
                "slide",
                &format!("slides/slide{}.xml", slide.file),
                false,
            ));
            slide_rids.push(rid);
        }
        let next = self.slides.len() + 2;
        pres_rels.push(rel(
            &format!("rId{next}"),
            "notesMaster",
            "notesMasters/notesMaster1.xml",
            false,
        ));
        pres_rels.push(rel(
            &format!("rId{}", next + 1),
            "presProps",
            "presProps.xml",
            false,
        ));
        pres_rels.push(rel(
            &format!("rId{}", next + 2),
            "viewProps",
            "viewProps.xml",
            false,
        ));
        pres_rels.push(rel(
            &format!("rId{}", next + 3),
            "theme",
            "theme/theme1.xml",
            false,
        ));
        pres_rels.push(rel(
            &format!("rId{}", next + 4),
            "tableStyles",
            "tableStyles.xml",
            false,
        ));
        if !self.authors.is_empty() {
            pres_rels.push(rel(
                &format!("rId{}", next + 5),
                "commentAuthors",
                "commentAuthors.xml",
                false,
            ));
            out.override_type(
                "ppt/commentAuthors.xml",
                &format!("{CT_PML}.commentAuthors+xml"),
            );
        }

        let presentation = self.presentation_xml(&slide_rids, &format!("rId{next}"));
        let mut parts: Vec<(String, Vec<u8>)> = vec![
            ("_rels/.rels".to_owned(), rels_xml(&root_rels).into_bytes()),
            (
                "docProps/core.xml".to_owned(),
                core_xml(&self.title).into_bytes(),
            ),
            (
                "docProps/app.xml".to_owned(),
                app_xml(self.slides.len()).into_bytes(),
            ),
        ];
        if self.empty_custom {
            parts.push((
                "docProps/custom.xml".to_owned(),
                format!(
                    "{DECL}<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/custom-properties\" xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\"/>"
                )
                .into_bytes(),
            ));
        } else if !self.custom.is_empty() {
            parts.push((
                "docProps/custom.xml".to_owned(),
                custom_xml(&self.custom).into_bytes(),
            ));
        }
        parts.push(("ppt/presentation.xml".to_owned(), presentation.into_bytes()));
        parts.push((
            "ppt/_rels/presentation.xml.rels".to_owned(),
            rels_xml(&pres_rels).into_bytes(),
        ));

        // Master, layouts, themes, notes master, and the small parts.
        out.override_type(
            "ppt/slideMasters/slideMaster1.xml",
            &format!("{CT_PML}.slideMaster+xml"),
        );
        parts.push((
            "ppt/slideMasters/slideMaster1.xml".to_owned(),
            master_xml().into_bytes(),
        ));
        let mut master_rels = Vec::new();
        for layout in Layout::ALL {
            master_rels.push(rel(
                &format!("rId{}", layout.number()),
                "slideLayout",
                &format!("../slideLayouts/slideLayout{}.xml", layout.number()),
                false,
            ));
        }
        master_rels.push(rel("rId6", "theme", "../theme/theme1.xml", false));
        parts.push((
            "ppt/slideMasters/_rels/slideMaster1.xml.rels".to_owned(),
            rels_xml(&master_rels).into_bytes(),
        ));
        for layout in Layout::ALL {
            let name = format!("ppt/slideLayouts/slideLayout{}.xml", layout.number());
            out.override_type(&name, &format!("{CT_PML}.slideLayout+xml"));
            parts.push((name, layout_xml(layout).into_bytes()));
            parts.push((
                format!(
                    "ppt/slideLayouts/_rels/slideLayout{}.xml.rels",
                    layout.number()
                ),
                rels_xml(&[rel(
                    "rId1",
                    "slideMaster",
                    "../slideMasters/slideMaster1.xml",
                    false,
                )])
                .into_bytes(),
            ));
        }
        for n in 1..=2 {
            let name = format!("ppt/theme/theme{n}.xml");
            out.override_type(
                &name,
                "application/vnd.openxmlformats-officedocument.theme+xml",
            );
            parts.push((name, theme_xml().into_bytes()));
        }
        out.override_type(
            "ppt/notesMasters/notesMaster1.xml",
            &format!("{CT_PML}.notesMaster+xml"),
        );
        parts.push((
            "ppt/notesMasters/notesMaster1.xml".to_owned(),
            notes_master_xml().into_bytes(),
        ));
        parts.push((
            "ppt/notesMasters/_rels/notesMaster1.xml.rels".to_owned(),
            rels_xml(&[rel("rId1", "theme", "../theme/theme2.xml", false)]).into_bytes(),
        ));
        for (name, kind, xml) in [
            ("ppt/presProps.xml", "presProps", pres_props_xml()),
            ("ppt/viewProps.xml", "viewProps", view_props_xml()),
        ] {
            out.override_type(name, &format!("{CT_PML}.{kind}+xml"));
            parts.push((name.to_owned(), xml.into_bytes()));
        }
        out.override_type("ppt/tableStyles.xml", &format!("{CT_PML}.tableStyles+xml"));
        parts.push((
            "ppt/tableStyles.xml".to_owned(),
            format!(
                "{DECL}<a:tblStyleLst xmlns:a=\"{NS_A}\" def=\"{{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}}\"/>"
            )
            .into_bytes(),
        ));
        if !self.authors.is_empty() {
            parts.push((
                "ppt/commentAuthors.xml".to_owned(),
                self.authors_xml().into_bytes(),
            ));
        }

        // Slides, with their notes, comments, pictures and charts.
        let mut media = 0;
        let mut charts = 0;
        let mut comments = 0;
        for slide in &self.slides {
            let mut slide_rels = vec![rel(
                "rId1",
                "slideLayout",
                &format!("../slideLayouts/slideLayout{}.xml", slide.layout.number()),
                false,
            )];
            let mut ctx = SlideCtx {
                layout: slide.layout,
                rels: &mut slide_rels,
                extra: Vec::new(),
                media: &mut media,
                charts: &mut charts,
                out: &mut out,
            };
            let tree = shapes_xml(&slide.shapes, &mut ctx);
            let extra = std::mem::take(&mut ctx.extra);
            let name = format!("ppt/slides/slide{}.xml", slide.file);
            out.override_type(&name, &format!("{CT_PML}.slide+xml"));
            if let Some(notes) = &slide.notes {
                let n = slide.file;
                let rid = format!("rId{}", slide_rels.len() + 1);
                slide_rels.push(rel(
                    &rid,
                    "notesSlide",
                    &format!("../notesSlides/notesSlide{n}.xml"),
                    false,
                ));
                let notes_name = format!("ppt/notesSlides/notesSlide{n}.xml");
                out.override_type(&notes_name, &format!("{CT_PML}.notesSlide+xml"));
                parts.push((notes_name, notes_slide_xml(notes).into_bytes()));
                parts.push((
                    format!("ppt/notesSlides/_rels/notesSlide{n}.xml.rels"),
                    rels_xml(&[
                        rel(
                            "rId1",
                            "notesMaster",
                            "../notesMasters/notesMaster1.xml",
                            false,
                        ),
                        rel("rId2", "slide", &format!("../slides/slide{n}.xml"), false),
                    ])
                    .into_bytes(),
                ));
            }
            if !slide.comments.is_empty() {
                comments += 1;
                let rid = format!("rId{}", slide_rels.len() + 1);
                slide_rels.push(rel(
                    &rid,
                    "comments",
                    &format!("../comments/comment{comments}.xml"),
                    false,
                ));
                let comments_name = format!("ppt/comments/comment{comments}.xml");
                out.override_type(&comments_name, &format!("{CT_PML}.comments+xml"));
                parts.push((comments_name, comments_xml(&slide.comments).into_bytes()));
            }
            parts.push((name, slide_xml(slide, &tree).into_bytes()));
            parts.push((
                format!("ppt/slides/_rels/slide{}.xml.rels", slide.file),
                rels_xml(&slide_rels).into_bytes(),
            ));
            parts.extend(extra);
        }

        let mut all = vec![(
            "[Content_Types].xml".to_owned(),
            out.content_types().into_bytes(),
        )];
        all.extend(parts);
        all
    }

    fn presentation_xml(&self, slide_rids: &[String], notes_rid: &str) -> String {
        let mut ids = String::new();
        for (slide, rid) in self.slides.iter().zip(slide_rids) {
            ids.push_str(&format!(r#"<p:sldId id="{}" r:id="{rid}"/>"#, slide.id));
        }
        let mut ext = String::new();
        if !self.sections.is_empty() {
            let mut sections = String::new();
            for (n, (name, slide_ids)) in self.sections.iter().enumerate() {
                let list: String = slide_ids
                    .iter()
                    .map(|id| format!(r#"<p14:sldId id="{id}"/>"#))
                    .collect();
                sections.push_str(&format!(
                    r#"<p14:section name="{}" id="{{8C1F2A{n:02}-1E2D-4C3B-9A8F-7E6D5C4B3A{n:02}}}"><p14:sldIdLst>{list}</p14:sldIdLst></p14:section>"#,
                    esc(name)
                ));
            }
            ext = format!(
                r#"<p:extLst><p:ext uri="{SECTIONS_EXT}"><p14:sectionLst xmlns:p14="{NS_P14}">{sections}</p14:sectionLst></p:ext></p:extLst>"#
            );
        }
        format!(
            r#"{DECL}<p:presentation xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}" saveSubsetFonts="1"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:notesMasterIdLst><p:notesMasterId r:id="{notes_rid}"/></p:notesMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx="{SLIDE_CX}" cy="{SLIDE_CY}"/><p:notesSz cx="6858000" cy="9144000"/><p:defaultTextStyle><a:lvl1pPr><a:defRPr sz="1800"/></a:lvl1pPr></p:defaultTextStyle>{ext}</p:presentation>"#
        )
    }

    fn authors_xml(&self) -> String {
        let mut list = String::new();
        for (n, (name, initials)) in self.authors.iter().enumerate() {
            let last = self
                .slides
                .iter()
                .flat_map(|s| &s.comments)
                .filter(|(a, _)| *a == n)
                .count();
            list.push_str(&format!(
                r#"<p:cmAuthor id="{n}" name="{}" initials="{}" lastIdx="{last}" clrIdx="{n}"/>"#,
                esc(name),
                esc(initials)
            ));
        }
        format!(
            r#"{DECL}<p:cmAuthorLst xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}">{list}</p:cmAuthorLst>"#
        )
    }
}

#[derive(Default)]
struct Parts {
    overrides: Vec<(String, String)>,
    defaults: Vec<(String, String)>,
}

impl Parts {
    fn override_type(&mut self, part: &str, content_type: &str) {
        if !self.overrides.iter().any(|(p, _)| p == part) {
            self.overrides
                .push((part.to_owned(), content_type.to_owned()));
        }
    }

    fn default_type(&mut self, ext: &str, content_type: &str) {
        if !self.defaults.iter().any(|(e, _)| e == ext) {
            self.defaults
                .push((ext.to_owned(), content_type.to_owned()));
        }
    }

    fn content_types(&self) -> String {
        let mut out = format!(
            r#"{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/>"#
        );
        for (ext, ct) in &self.defaults {
            out.push_str(&format!(
                r#"<Default Extension="{ext}" ContentType="{ct}"/>"#
            ));
        }
        for (part, ct) in &self.overrides {
            out.push_str(&format!(
                r#"<Override PartName="/{part}" ContentType="{ct}"/>"#
            ));
        }
        out.push_str("</Types>");
        out
    }
}

type RelRow = (String, String, String, bool);

fn rel(id: &str, kind: &str, target: &str, external: bool) -> RelRow {
    (
        id.to_owned(),
        format!("{REL}/{kind}"),
        target.to_owned(),
        external,
    )
}

fn rels_xml(rels: &[RelRow]) -> String {
    let mut out = format!(
        r#"{DECL}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#
    );
    for (id, kind, target, external) in rels {
        let mode = if *external {
            r#" TargetMode="External""#
        } else {
            ""
        };
        out.push_str(&format!(
            r#"<Relationship Id="{id}" Type="{kind}" Target="{}"{mode}/>"#,
            esc(target)
        ));
    }
    out.push_str("</Relationships>");
    out
}

fn core_xml(title: &str) -> String {
    format!(
        r#"{DECL}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:dcmitype="http://purl.org/dc/dcmitype/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>{}</dc:title><dc:creator>Lineage fixture generator</dc:creator><cp:lastModifiedBy>Lineage fixture generator</cp:lastModifiedBy><dcterms:created xsi:type="dcterms:W3CDTF">{FIXED_DATE}</dcterms:created><dcterms:modified xsi:type="dcterms:W3CDTF">{FIXED_DATE}</dcterms:modified></cp:coreProperties>"#,
        esc(title)
    )
}

fn app_xml(slides: usize) -> String {
    format!(
        r#"{DECL}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes"><Application>Lineage fixture generator</Application><PresentationFormat>Widescreen</PresentationFormat><Slides>{slides}</Slides><Company>Lumen Ledger</Company></Properties>"#
    )
}

fn custom_xml(props: &[(String, String)]) -> String {
    let mut out = format!(
        r#"{DECL}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/custom-properties" xmlns:vt="http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes">"#
    );
    for (n, (name, value)) in props.iter().enumerate() {
        out.push_str(&format!(
            r#"<property fmtid="{{D5CDD505-2E9C-101B-9397-08002B2CF9AE}}" pid="{}" name="{}"><vt:lpwstr>{}</vt:lpwstr></property>"#,
            n + 2,
            esc(name),
            esc(value)
        ));
    }
    out.push_str("</Properties>");
    out
}

const GROUP_PROPS: &str = r#"<p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>"#;

fn placeholder(id: u32, name: &str, ph: &str, at: Option<Rect>, text: &str) -> String {
    placeholder_styled(id, name, ph, at, "", text)
}

fn placeholder_styled(
    id: u32,
    name: &str,
    ph: &str,
    at: Option<Rect>,
    lst_style: &str,
    text: &str,
) -> String {
    let xfrm = at.map_or_else(String::new, |at| {
        format!(
            r#"<a:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom>"#,
            at.x, at.y, at.cx, at.cy
        )
    });
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph {ph}/></p:nvPr></p:nvSpPr><p:spPr>{xfrm}</p:spPr><p:txBody><a:bodyPr/><a:lstStyle>{lst_style}</a:lstStyle><a:p><a:r><a:rPr lang="en-US"/><a:t>{text}</a:t></a:r></a:p></p:txBody></p:sp>"#
    )
}

const TITLE_AT: Rect = rect(838_200, 365_125, 10_515_600, 1_325_563);
const BODY_AT: Rect = rect(838_200, 1_825_625, 10_515_600, 4_351_338);
const LEFT_AT: Rect = rect(838_200, 1_825_625, 5_181_600, 4_351_338);
const RIGHT_AT: Rect = rect(6_172_200, 1_825_625, 5_181_600, 4_351_338);
const CTR_TITLE_AT: Rect = rect(1_524_000, 1_122_363, 9_144_000, 2_387_600);
const SUBTITLE_AT: Rect = rect(1_524_000, 3_602_038, 9_144_000, 1_655_762);

fn master_xml() -> String {
    let mut layouts = String::new();
    for layout in Layout::ALL {
        layouts.push_str(&format!(
            r#"<p:sldLayoutId id="{}" r:id="rId{}"/>"#,
            2_147_483_648_u32 + u32::try_from(layout.number()).unwrap_or(0),
            layout.number()
        ));
    }
    format!(
        r#"{DECL}<p:sldMaster xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree>{GROUP_PROPS}{}{}</p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst>{layouts}</p:sldLayoutIdLst><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz="4400"/></a:lvl1pPr></p:titleStyle><p:bodyStyle><a:lvl1pPr marL="228600" indent="-228600"><a:buFont typeface="Arial"/><a:buChar char="&#8226;"/><a:defRPr sz="2800"/></a:lvl1pPr><a:lvl2pPr marL="685800" indent="-228600"><a:buFont typeface="Arial"/><a:buChar char="&#8226;"/><a:defRPr sz="2400"/></a:lvl2pPr></p:bodyStyle><p:otherStyle><a:lvl1pPr><a:defRPr sz="1800"/></a:lvl1pPr></p:otherStyle></p:txStyles></p:sldMaster>"#,
        placeholder(
            2,
            "Title Placeholder 1",
            r#"type="title""#,
            Some(TITLE_AT),
            "Click to edit Master title style"
        ),
        placeholder(
            3,
            "Text Placeholder 2",
            r#"type="body" idx="1""#,
            Some(BODY_AT),
            "Click to edit Master text styles"
        ),
    )
}

fn layout_xml(layout: Layout) -> String {
    let shapes = match layout {
        Layout::Title => [
            placeholder(
                2,
                "Title 1",
                r#"type="ctrTitle""#,
                Some(CTR_TITLE_AT),
                "Click to edit Master title style",
            ),
            placeholder(
                3,
                "Subtitle 2",
                r#"type="subTitle" idx="1""#,
                Some(SUBTITLE_AT),
                "Click to edit Master subtitle style",
            ),
        ]
        .concat(),
        Layout::TitleAndContent => [
            placeholder(
                2,
                "Title 1",
                r#"type="title""#,
                None,
                "Click to edit Master title style",
            ),
            placeholder(
                3,
                "Content Placeholder 2",
                r#"idx="1""#,
                None,
                "Click to edit Master text styles",
            ),
        ]
        .concat(),
        Layout::TwoContent => [
            placeholder(
                2,
                "Title 1",
                r#"type="title""#,
                None,
                "Click to edit Master title style",
            ),
            placeholder(
                3,
                "Content Placeholder 2",
                r#"sz="half" idx="1""#,
                Some(LEFT_AT),
                "Click to edit Master text styles",
            ),
            placeholder(
                4,
                "Content Placeholder 3",
                r#"sz="half" idx="2""#,
                Some(RIGHT_AT),
                "Click to edit Master text styles",
            ),
        ]
        .concat(),
        Layout::TitleOnly => placeholder(
            2,
            "Title 1",
            r#"type="title""#,
            None,
            "Click to edit Master title style",
        ),
        Layout::Blank => String::new(),
    };
    format!(
        r#"{DECL}<p:sldLayout xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}" type="{}" preserve="1"><p:cSld name="{}"><p:spTree>{GROUP_PROPS}{shapes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#,
        layout.kind(),
        layout.name()
    )
}

fn theme_xml() -> String {
    let three = |x: &str| x.repeat(3);
    format!(
        r#"{DECL}<a:theme xmlns:a="{NS_A}" name="Lineage"><a:themeElements><a:clrScheme name="Lineage"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1><a:dk2><a:srgbClr val="18212B"/></a:dk2><a:lt2><a:srgbClr val="F4F5F7"/></a:lt2><a:accent1><a:srgbClr val="0E7C7B"/></a:accent1><a:accent2><a:srgbClr val="4F46B5"/></a:accent2><a:accent3><a:srgbClr val="5B6573"/></a:accent3><a:accent4><a:srgbClr val="9A3412"/></a:accent4><a:accent5><a:srgbClr val="6B3D06"/></a:accent5><a:accent6><a:srgbClr val="2E2880"/></a:accent6><a:hlink><a:srgbClr val="0B4F4E"/></a:hlink><a:folHlink><a:srgbClr val="2E2880"/></a:folHlink></a:clrScheme><a:fontScheme name="Lineage"><a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:cs typeface=""/></a:majorFont><a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:cs typeface=""/></a:minorFont></a:fontScheme><a:fmtScheme name="Lineage"><a:fillStyleLst>{}</a:fillStyleLst><a:lnStyleLst>{}</a:lnStyleLst><a:effectStyleLst>{}</a:effectStyleLst><a:bgFillStyleLst>{}</a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#,
        three(r#"<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>"#),
        three(r#"<a:ln w="6350"><a:solidFill><a:schemeClr val="phClr"/></a:solidFill></a:ln>"#),
        three("<a:effectStyle><a:effectLst/></a:effectStyle>"),
        three(r#"<a:solidFill><a:schemeClr val="phClr"/></a:solidFill>"#),
    )
}

fn notes_master_xml() -> String {
    format!(
        r#"{DECL}<p:notesMaster xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree>{GROUP_PROPS}<p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg" idx="2"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm><a:off x="685800" y="1143000"/><a:ext cx="5486400" cy="3086100"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:sp>{}</p:spTree></p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:notesStyle><a:lvl1pPr><a:defRPr sz="1200"/></a:lvl1pPr></p:notesStyle></p:notesMaster>"#,
        placeholder(
            3,
            "Notes Placeholder 2",
            r#"type="body" sz="quarter" idx="3""#,
            Some(rect(685_800, 4_400_550, 5_486_400, 3_600_450)),
            "Click to edit Master text styles"
        ),
    )
}

fn notes_slide_xml(notes: &[String]) -> String {
    let paras: String = notes
        .iter()
        .map(|n| para_xml(&p(n), &mut |_| String::new()))
        .collect();
    format!(
        r#"{DECL}<p:notes xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"><p:cSld><p:spTree>{GROUP_PROPS}<p:sp><p:nvSpPr><p:cNvPr id="2" name="Slide Image Placeholder 1"/><p:cNvSpPr><a:spLocks noGrp="1" noRot="1" noChangeAspect="1"/></p:cNvSpPr><p:nvPr><p:ph type="sldImg"/></p:nvPr></p:nvSpPr><p:spPr/></p:sp><p:sp><p:nvSpPr><p:cNvPr id="3" name="Notes Placeholder 2"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph type="body" idx="1"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>{paras}</p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>"#
    )
}

fn comments_xml(comments: &[(usize, String)]) -> String {
    let mut list = String::new();
    for (n, (author, text)) in comments.iter().enumerate() {
        list.push_str(&format!(
            r#"<p:cm authorId="{author}" dt="2026-01-01T00:00:00.000" idx="{}"><p:pos x="10" y="10"/><p:text>{}</p:text></p:cm>"#,
            n + 1,
            esc(text)
        ));
    }
    format!(r#"{DECL}<p:cmLst xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}">{list}</p:cmLst>"#)
}

fn pres_props_xml() -> String {
    format!(r#"{DECL}<p:presentationPr xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"/>"#)
}

fn view_props_xml() -> String {
    format!(
        r#"{DECL}<p:viewPr xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"><p:normalViewPr><p:restoredLeft sz="15620"/><p:restoredTop sz="94660"/></p:normalViewPr><p:gridSpacing cx="76200" cy="76200"/></p:viewPr>"#
    )
}

fn slide_xml(slide: &Slide, tree: &str) -> String {
    let show = if slide.hidden { r#" show="0""# } else { "" };
    let ext = slide.creation_id.map_or_else(String::new, |id| {
        format!(
            r#"<p:extLst><p:ext uri="{CREATION_ID_EXT}"><p14:creationId xmlns:p14="{NS_P14}" val="{id}"/></p:ext></p:extLst>"#
        )
    });
    let timing = slide.timing.clone().unwrap_or_default();
    format!(
        r#"{DECL}<p:sld xmlns:a="{NS_A}" xmlns:r="{NS_R}" xmlns:p="{NS_P}"{show}><p:cSld><p:spTree>{GROUP_PROPS}{tree}</p:spTree>{ext}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{timing}</p:sld>"#
    )
}

struct SlideCtx<'a> {
    layout: Layout,
    rels: &'a mut Vec<RelRow>,
    extra: Vec<(String, Vec<u8>)>,
    media: &'a mut u32,
    charts: &'a mut u32,
    out: &'a mut Parts,
}

impl SlideCtx<'_> {
    fn add_rel(&mut self, kind: &str, target: &str, external: bool) -> String {
        let id = format!("rId{}", self.rels.len() + 1);
        self.rels.push(rel(&id, kind, target, external));
        id
    }
}

fn run_xml(run: &Run, link: &mut dyn FnMut(&str) -> String) -> String {
    let mut attrs = String::from(r#" lang="en-US""#);
    if run.bold {
        attrs.push_str(r#" b="1""#);
    }
    if run.italic {
        attrs.push_str(r#" i="1""#);
    }
    let rpr = match &run.link {
        Some(url) => {
            let rid = link(url);
            format!(r#"<a:rPr{attrs}><a:hlinkClick r:id="{rid}"/></a:rPr>"#)
        }
        None => format!("<a:rPr{attrs}/>"),
    };
    format!("<a:r>{rpr}<a:t>{}</a:t></a:r>", esc(&run.text))
}

fn para_xml(para: &Para, link: &mut dyn FnMut(&str) -> String) -> String {
    let ppr = if para.level > 0 {
        format!(r#"<a:pPr lvl="{}"/>"#, para.level)
    } else {
        String::new()
    };
    let runs: String = para.runs.iter().map(|r| run_xml(r, link)).collect();
    if runs.is_empty() {
        format!(r#"<a:p>{ppr}<a:endParaRPr lang="en-US"/></a:p>"#)
    } else {
        format!("<a:p>{ppr}{runs}</a:p>")
    }
}

fn paras_xml(paras: &[Para], ctx: &mut SlideCtx<'_>) -> String {
    let mut link = |url: &str| ctx.add_rel("hyperlink", url, true);
    let body: String = paras.iter().map(|p| para_xml(p, &mut link)).collect();
    if body.is_empty() {
        r#"<a:p><a:endParaRPr lang="en-US"/></a:p>"#.to_owned()
    } else {
        body
    }
}

fn xfrm(at: Rect) -> String {
    format!(
        r#"<a:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></a:xfrm>"#,
        at.x, at.y, at.cx, at.cy
    )
}

fn shapes_xml(shapes: &[Placed], ctx: &mut SlideCtx<'_>) -> String {
    let mut next = 2;
    let mut out = String::new();
    for placed in shapes {
        let id = placed.id.unwrap_or(next);
        next = next.max(id + 1);
        out.push_str(&shape_xml(id, &placed.shape, ctx, &mut next));
    }
    out
}

fn ph_sp(id: u32, name: &str, ph: &str, paras: &[Para], ctx: &mut SlideCtx<'_>) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr><p:ph {ph}/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/>{}</p:txBody></p:sp>"#,
        paras_xml(paras, ctx)
    )
}

#[allow(clippy::too_many_lines)]
fn shape_xml(id: u32, shape: &Shape, ctx: &mut SlideCtx<'_>, next: &mut u32) -> String {
    let n = id - 1;
    match shape {
        Shape::Title(paras) => {
            let ph = if ctx.layout == Layout::Title {
                r#"type="ctrTitle""#
            } else {
                r#"type="title""#
            };
            ph_sp(id, &format!("Title {n}"), ph, paras, ctx)
        }
        Shape::Subtitle(paras) => ph_sp(
            id,
            &format!("Subtitle {n}"),
            r#"type="subTitle" idx="1""#,
            paras,
            ctx,
        ),
        Shape::Body { idx, paras } => ph_sp(
            id,
            &format!("Content Placeholder {n}"),
            &format!(r#"idx="{idx}""#),
            paras,
            ctx,
        ),
        Shape::TextBox { name, at, paras } => format!(
            r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{}"/><p:cNvSpPr txBox="1"/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap="square" rtlCol="0"><a:spAutoFit/></a:bodyPr><a:lstStyle/>{}</p:txBody></p:sp>"#,
            esc(name),
            xfrm(*at),
            paras_xml(paras, ctx)
        ),
        Shape::Picture {
            name,
            descr,
            at,
            png,
        } => {
            *ctx.media += 1;
            let file = format!("ppt/media/image{}.png", ctx.media);
            ctx.out.default_type("png", "image/png");
            ctx.extra.push((file, png.clone()));
            let rid = ctx.add_rel("image", &format!("../media/image{}.png", ctx.media), false);
            format!(
                r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="{}" descr="{}"/><p:cNvPicPr><a:picLocks noChangeAspect="1"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="{rid}"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#,
                esc(name),
                esc(descr),
                xfrm(*at)
            )
        }
        Shape::Table { name, at, rows } => {
            let columns = rows.first().map_or(1, Vec::len).max(1);
            let width = at.cx / i64::try_from(columns).unwrap_or(1);
            let grid: String = (0..columns)
                .map(|_| format!(r#"<a:gridCol w="{width}"/>"#))
                .collect();
            let mut body = String::new();
            for row in rows {
                body.push_str(r#"<a:tr h="370840">"#);
                for cell in row {
                    body.push_str(&format!(
                        r#"<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:rPr lang="en-US"/><a:t>{}</a:t></a:r></a:p></a:txBody><a:tcPr/></a:tc>"#,
                        esc(cell)
                    ));
                }
                body.push_str("</a:tr>");
            }
            format!(
                r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{id}" name="{}"/><p:cNvGraphicFramePr><a:graphicFrameLocks noGrp="1"/></p:cNvGraphicFramePr><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr firstRow="1" bandRow="1"/><a:tblGrid>{grid}</a:tblGrid>{body}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#,
                esc(name),
                at.x,
                at.y,
                at.cx,
                at.cy
            )
        }
        Shape::Chart { name, at, data } => {
            *ctx.charts += 1;
            let n = *ctx.charts;
            let chart = format!("ppt/charts/chart{n}.xml");
            let book = format!("ppt/embeddings/Microsoft_Excel_Worksheet{n}.xlsx");
            ctx.out.override_type(
                &chart,
                "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
            );
            ctx.out.default_type("xlsx", CT_XLSX);
            ctx.extra
                .push((chart.clone(), chart_xml(data).into_bytes()));
            ctx.extra.push((
                format!("ppt/charts/_rels/chart{n}.xml.rels"),
                rels_xml(&[rel(
                    "rId1",
                    "package",
                    &format!("../embeddings/Microsoft_Excel_Worksheet{n}.xlsx"),
                    false,
                )])
                .into_bytes(),
            ));
            ctx.extra.push((book, chart_workbook(data)));
            let rid = ctx.add_rel("chart", &format!("../charts/chart{n}.xml"), false);
            format!(
                r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="{id}" name="{}"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="{}" y="{}"/><a:ext cx="{}" cy="{}"/></p:xfrm><a:graphic><a:graphicData uri="{NS_C}"><c:chart xmlns:c="{NS_C}" r:id="{rid}"/></a:graphicData></a:graphic></p:graphicFrame>"#,
                esc(name),
                at.x,
                at.y,
                at.cx,
                at.cy
            )
        }
        Shape::Group { name, shapes } => {
            let mut inner = String::new();
            for child in shapes {
                let child_id = *next;
                *next += 1;
                inner.push_str(&shape_xml(child_id, child, ctx, next));
            }
            format!(
                r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="{id}" name="{}"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="838200" y="1825625"/><a:ext cx="10515600" cy="4351338"/><a:chOff x="838200" y="1825625"/><a:chExt cx="10515600" cy="4351338"/></a:xfrm></p:grpSpPr>{inner}</p:grpSp>"#,
                esc(name)
            )
        }
        Shape::Raw(xml) => xml.replace("{id}", &id.to_string()),
    }
}

fn chart_xml(data: &ChartData) -> String {
    let count = data.categories.len();
    let last = count + 1;
    let cats: String = data
        .categories
        .iter()
        .enumerate()
        .map(|(i, c)| format!(r#"<c:pt idx="{i}"><c:v>{}</c:v></c:pt>"#, esc(c)))
        .collect();
    let vals: String = data
        .values
        .iter()
        .enumerate()
        .map(|(i, v)| format!(r#"<c:pt idx="{i}"><c:v>{v}</c:v></c:pt>"#))
        .collect();
    format!(
        r#"{DECL}<c:chartSpace xmlns:c="{NS_C}" xmlns:a="{NS_A}" xmlns:r="{NS_R}"><c:roundedCorners val="0"/><c:chart><c:autoTitleDeleted val="1"/><c:plotArea><c:layout/><c:barChart><c:barDir val="col"/><c:grouping val="clustered"/><c:varyColors val="0"/><c:ser><c:idx val="0"/><c:order val="0"/><c:tx><c:strRef><c:f>Sheet1!$B$1</c:f><c:strCache><c:ptCount val="1"/><c:pt idx="0"><c:v>{}</c:v></c:pt></c:strCache></c:strRef></c:tx><c:cat><c:strRef><c:f>Sheet1!$A$2:$A${last}</c:f><c:strCache><c:ptCount val="{count}"/>{cats}</c:strCache></c:strRef></c:cat><c:val><c:numRef><c:f>Sheet1!$B$2:$B${last}</c:f><c:numCache><c:formatCode>General</c:formatCode><c:ptCount val="{count}"/>{vals}</c:numCache></c:numRef></c:val></c:ser><c:gapWidth val="150"/><c:axId val="111111111"/><c:axId val="222222222"/></c:barChart><c:catAx><c:axId val="111111111"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="b"/><c:crossAx val="222222222"/></c:catAx><c:valAx><c:axId val="222222222"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:delete val="0"/><c:axPos val="l"/><c:crossAx val="111111111"/></c:valAx></c:plotArea><c:plotVisOnly val="1"/></c:chart><c:externalData r:id="rId1"><c:autoUpdate val="0"/></c:externalData></c:chartSpace>"#,
        esc(&data.series)
    )
}

/// The chart's data as a small, valid .xlsx (inline strings, no styles).
fn chart_workbook(data: &ChartData) -> Vec<u8> {
    let cell_str =
        |r: &str, s: &str| format!(r#"<c r="{r}" t="inlineStr"><is><t>{}</t></is></c>"#, esc(s));
    let mut rows = format!(
        r#"<row r="1">{}{}</row>"#,
        cell_str("A1", " "),
        cell_str("B1", &data.series)
    );
    for (i, (cat, value)) in data.categories.iter().zip(&data.values).enumerate() {
        let r = i + 2;
        rows.push_str(&format!(
            r#"<row r="{r}">{}<c r="B{r}"><v>{value}</v></c></row>"#,
            cell_str(&format!("A{r}"), cat)
        ));
    }
    let ss = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    let parts = vec![
        (
            "[Content_Types].xml".to_owned(),
            format!(
                r#"{DECL}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#
            )
            .into_bytes(),
        ),
        (
            "_rels/.rels".to_owned(),
            rels_xml(&[rel("rId1", "officeDocument", "xl/workbook.xml", false)]).into_bytes(),
        ),
        (
            "xl/workbook.xml".to_owned(),
            format!(
                r#"{DECL}<workbook xmlns="{ss}" xmlns:r="{NS_R}"><sheets><sheet name="Sheet1" sheetId="1" r:id="rId1"/></sheets></workbook>"#
            )
            .into_bytes(),
        ),
        (
            "xl/_rels/workbook.xml.rels".to_owned(),
            rels_xml(&[rel("rId1", "worksheet", "worksheets/sheet1.xml", false)]).into_bytes(),
        ),
        (
            "xl/worksheets/sheet1.xml".to_owned(),
            format!(r#"{DECL}<worksheet xmlns="{ss}"><sheetData>{rows}</sheetData></worksheet>"#)
                .into_bytes(),
        ),
    ];
    zip_writer::write_stored(&parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Read};

    fn names(deck: &Deck) -> Vec<String> {
        deck.parts().into_iter().map(|(n, _)| n).collect()
    }

    fn sample() -> Deck {
        Deck::new(
            "Sample",
            vec![
                Slide::new(
                    256,
                    Layout::Title,
                    vec![
                        Shape::Title(vec![p("Lumen Ledger")]),
                        Shape::Subtitle(vec![p("Seed round")]),
                    ],
                )
                .with_notes(&["Open with the problem."]),
                Slide::new(
                    257,
                    Layout::TitleAndContent,
                    vec![
                        Shape::Title(vec![p("Traction")]),
                        Shape::Chart {
                            name: "Chart 3".to_owned(),
                            at: rect(838_200, 1_825_625, 6_000_000, 4_000_000),
                            data: ChartData {
                                series: "ARR ($k)".to_owned(),
                                categories: vec!["Q1".to_owned(), "Q2".to_owned()],
                                values: vec![400, 1200],
                            },
                        },
                    ],
                ),
            ],
        )
    }

    #[test]
    fn every_part_is_well_formed_and_has_a_content_type() {
        let deck = sample();
        let bytes = deck.to_bytes();
        let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).expect("a zip");
        let mut types = String::new();
        zip.by_name("[Content_Types].xml")
            .expect("types")
            .read_to_string(&mut types)
            .expect("utf-8");
        for name in names(&deck) {
            if name.ends_with(".xml") && name != "[Content_Types].xml" {
                assert!(
                    types.contains(&format!("\"/{name}\"")),
                    "{name} has no override"
                );
            }
        }
        assert!(types.contains(r#"Extension="xlsx""#));
        assert!(names(&deck).contains(&"ppt/notesSlides/notesSlide1.xml".to_owned()));
        assert!(
            names(&deck).contains(&"ppt/embeddings/Microsoft_Excel_Worksheet1.xlsx".to_owned())
        );
    }

    #[test]
    fn output_is_deterministic() {
        assert_eq!(sample().to_bytes(), sample().to_bytes());
    }

    #[test]
    fn slides_keep_their_files_and_ids() {
        let mut deck = sample();
        deck.slides.swap(0, 1);
        let parts = deck.parts();
        let presentation = parts
            .iter()
            .find(|(n, _)| n == "ppt/presentation.xml")
            .map(|(_, b)| String::from_utf8(b.clone()).expect("utf-8"))
            .expect("presentation");
        let first = presentation.find(r#"id="257""#).expect("257");
        let second = presentation.find(r#"id="256""#).expect("256");
        assert!(first < second);
        assert!(names(&deck).contains(&"ppt/slides/slide1.xml".to_owned()));
        assert_eq!(sample().slides[1].title().as_deref(), Some("Traction"));
    }
}
