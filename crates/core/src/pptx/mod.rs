// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! PowerPoint decks: opening a .pptx into the slide model, and everything the
//! engine does with decks (what changed, merging, writing, clean copies).

pub mod model;
mod read;

pub use model::*;

use crate::error::{Error, Result};
use crate::hash::Fnv;
use crate::package::{ContentTypes, Package, Relationships, resolve_target};
use crate::xml::{self, ns};
use read::{LayoutCache, read_rels};

/// An opened deck: the package, its slides in show order, and its sections.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deck {
    pub package: Package,
    /// The presentation part, usually `ppt/presentation.xml`.
    pub main_part: String,
    pub main_rels: Relationships,
    pub slides: Vec<Slide>,
    pub sections: Vec<Section>,
    /// The design: slide masters, layouts and themes. Equal when none changed.
    pub design: u64,
}

impl Deck {
    /// Opens a .pptx.
    ///
    /// # Errors
    /// [`Error`] when the bytes are not a deck this version can read.
    pub fn open(bytes: &[u8]) -> Result<Self> {
        Self::from_package(Package::read(bytes)?)
    }

    /// Builds the model of an already-read package.
    ///
    /// # Errors
    /// [`Error`] when the package is not a deck this version can read.
    pub fn from_package(package: Package) -> Result<Self> {
        let main_part = main_part(&package)?;
        let main_rels = read_rels(&package, &main_part)?;
        let bytes = package
            .part(&main_part)
            .ok_or_else(|| Error::NotADeck("presentation part missing".to_owned()))?;
        let text = xml::text(&main_part, bytes).map_err(|e| match e {
            Error::Unsupported(_) => Error::Unsupported("the presentation is UTF-16".to_owned()),
            other => other,
        })?;
        let doc = xml::parse(&main_part, text)?;
        let root = doc.root_element();
        if xml::is(root, ns::P_STRICT, "presentation") {
            return Err(Error::Unsupported("Strict Open XML".to_owned()));
        }
        if !xml::is(root, ns::P, "presentation") {
            return Err(Error::NotADeck("no p:presentation".to_owned()));
        }

        let mut layouts = LayoutCache::new();
        let mut slides = Vec::new();
        if let Some(list) = xml::child(root, ns::P, "sldIdLst") {
            for entry in list.children().filter(|n| xml::is(*n, ns::P, "sldId")) {
                let sld_id = entry
                    .attribute("id")
                    .and_then(|v| v.parse::<u32>().ok())
                    .ok_or_else(|| Error::NotADeck("a slide without an id".to_owned()))?;
                let rel_id = entry
                    .attribute((ns::R, "id"))
                    .ok_or_else(|| Error::NotADeck("a slide without a relationship".to_owned()))?;
                let rel = main_rels
                    .by_id(rel_id)
                    .ok_or_else(|| Error::NotADeck(format!("no relationship {rel_id}")))?;
                let part = resolve_target(&main_part, &rel.target);
                let part = package.canonical_name(&part).unwrap_or(&part).to_owned();
                if slides
                    .iter()
                    .any(|s: &Slide| s.sld_id == sld_id || s.part == part)
                {
                    return Err(Error::Damaged(format!("slide {sld_id} listed twice")));
                }
                slides.push(read::slide(&package, &part, rel_id, sld_id, &mut layouts)?);
            }
        }

        let sections = root
            .descendants()
            .filter(|n| xml::is(*n, ns::P14, "section"))
            .map(|section| Section {
                name: section.attribute("name").unwrap_or_default().to_owned(),
                slides: section
                    .descendants()
                    .filter(|n| xml::is(*n, ns::P14, "sldId"))
                    .filter_map(|n| n.attribute("id").and_then(|v| v.parse().ok()))
                    .collect(),
            })
            .collect();

        let design = design_key(&package);
        Ok(Self {
            package,
            main_part,
            main_rels,
            slides,
            sections,
            design,
        })
    }

    /// The text of a part, as UTF-8.
    ///
    /// # Errors
    /// [`Error`] when the part is missing or not UTF-8.
    pub fn part_text(&self, part: &str) -> Result<&str> {
        let bytes = self
            .package
            .part(part)
            .ok_or_else(|| Error::Damaged(format!("{part} is missing")))?;
        xml::text(part, bytes)
    }

    /// The slide with this `p:sldId@id`.
    #[must_use]
    pub fn slide(&self, sld_id: u32) -> Option<&Slide> {
        self.slides.iter().find(|s| s.sld_id == sld_id)
    }

    /// Every slide's text, one line per paragraph, with a header line per slide
    /// ("# Slide 2 · Problem", "(hidden)" when hidden) and the notes after
    /// "Notes:". Used by tests and the PowerPoint smoke comparison.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut out = Vec::new();
        for (index, slide) in self.slides.iter().enumerate() {
            let mut header = format!("# Slide {}", index + 1);
            if let Some(title) = slide.title() {
                header.push_str(" · ");
                header.push_str(&title);
            }
            if slide.hidden {
                header.push_str(" (hidden)");
            }
            out.push(header);
            for shape in &slide.shapes {
                for para in &shape.paras {
                    out.push(para.text.clone());
                }
            }
            if let Some(notes) = &slide.notes {
                out.push("Notes:".to_owned());
                for para in &notes.paras {
                    out.push(para.text.clone());
                }
            }
        }
        out
    }
}

/// Every slide master, layout and theme, by bytes: what PowerPoint calls the design.
fn design_key(package: &Package) -> u64 {
    let mut names: Vec<&str> = package
        .part_names()
        .filter(|n| {
            let lower = n.to_ascii_lowercase();
            (lower.starts_with("ppt/slidemasters/")
                || lower.starts_with("ppt/slidelayouts/")
                || lower.starts_with("ppt/theme/"))
                && !lower.contains("/_rels/")
        })
        .collect();
    names.sort_unstable();
    let mut h = Fnv::new();
    for name in names {
        h.bytes(package.part(name).unwrap_or_default());
    }
    h.finish()
}

/// Finds the presentation part and checks it is a .pptx (not .pptm, .potx or .ppsx).
fn main_part(package: &Package) -> Result<String> {
    let types = package
        .part(ContentTypes::PART)
        .ok_or_else(|| Error::NotAnOfficeFile("no content types".to_owned()))
        .and_then(ContentTypes::parse)?;
    let root_rels = read_rels(package, "")?;
    let rel = root_rels
        .of_kind("officeDocument")
        .next()
        .ok_or_else(|| Error::NotADeck("no main part".to_owned()))?;
    let part = resolve_target("", &rel.target);
    match types.of(&part) {
        Some(ns::CT_PRESENTATION) => {}
        Some(ns::CT_PRESENTATION_MACRO) => {
            return Err(Error::Unsupported("macro-enabled presentation".to_owned()));
        }
        Some(ns::CT_PRESENTATION_TEMPLATE) => {
            return Err(Error::Unsupported("PowerPoint template".to_owned()));
        }
        Some(ns::CT_SLIDESHOW) => return Err(Error::Unsupported("PowerPoint show".to_owned())),
        _ => {
            return Err(Error::NotADeck(
                "main part is not a presentation".to_owned(),
            ));
        }
    }
    Ok(package.canonical_name(&part).unwrap_or(&part).to_owned())
}

/// Which kind of Office file a package is, by its main part.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Word,
    PowerPoint,
}

impl Kind {
    /// The family format this kind is recorded as.
    #[must_use]
    pub fn format(self) -> &'static str {
        match self {
            Self::Word => "docx",
            Self::PowerPoint => "pptx",
        }
    }
}

/// Tells a Word document from a deck, by the main part's content type.
///
/// # Errors
/// [`Error`] when the bytes are not an Office package, or neither kind.
pub fn kind_of(bytes: &[u8]) -> Result<Kind> {
    let package = Package::read(bytes)?;
    let types = package
        .part(ContentTypes::PART)
        .ok_or_else(|| Error::NotAnOfficeFile("no content types".to_owned()))
        .and_then(ContentTypes::parse)?;
    let rels = read_rels(&package, "")?;
    let part = rels
        .of_kind("officeDocument")
        .next()
        .map(|rel| resolve_target("", &rel.target))
        .ok_or_else(|| Error::NotAnOfficeFile("no main part".to_owned()))?;
    let content_type = types.of(&part).unwrap_or_default();
    if content_type.contains("presentationml") || content_type.contains("ms-powerpoint") {
        Ok(Kind::PowerPoint)
    } else if content_type.contains("wordprocessingml") || content_type.contains("ms-word") {
        Ok(Kind::Word)
    } else {
        Err(Error::NotAnOfficeFile(
            "neither a document nor a deck".to_owned(),
        ))
    }
}

#[cfg(test)]
mod tests;
