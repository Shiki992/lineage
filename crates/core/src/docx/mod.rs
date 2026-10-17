// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Word documents: opening a .docx into the canonical model.

pub mod model;
mod read;

/// Custom properties are the same in every Office package; kept here under
/// their old path.
pub use crate::package::props;

use std::collections::BTreeMap;

pub use model::*;

use crate::error::{Error, Result};
use crate::package::{ContentTypes, Package, Relationships, rels_part_for, resolve_target};
use crate::xml::{self, ns};

/// Display names of styles, for summaries ("Heading 2" rather than "Heading2").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StyleNames(BTreeMap<String, String>);

impl StyleNames {
    /// The name a person sees for a style id. Word's built-in names are stored in
    /// lower case ("heading 1"), so each word gets a capital, as Word shows them.
    #[must_use]
    pub fn display(&self, id: &str) -> String {
        let name = self.0.get(id).map_or(id, String::as_str);
        name.split(' ')
            .map(|word| {
                let mut chars = word.chars();
                chars.next().map_or_else(String::new, |first| {
                    first.to_uppercase().chain(chars).collect()
                })
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn parse(bytes: &[u8]) -> Result<Self> {
        let text = xml::text("word/styles.xml", bytes)?;
        let doc = xml::parse("word/styles.xml", text)?;
        let mut names = BTreeMap::new();
        for style in doc
            .root_element()
            .children()
            .filter(|n| xml::is(*n, ns::W, "style"))
        {
            let Some(id) = style.attribute((ns::W, "styleId")) else {
                continue;
            };
            if let Some(name) =
                xml::child(style, ns::W, "name").and_then(|n| n.attribute((ns::W, "val")))
            {
                names.insert(id.to_owned(), name.to_owned());
            }
        }
        Ok(Self(names))
    }
}

/// An opened Word document: the package, the model of each story, and the
/// names needed to describe it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Docx {
    pub package: Package,
    /// The main document part, usually `word/document.xml`.
    pub main_part: String,
    pub main_rels: Relationships,
    pub stories: Vec<Story>,
    pub styles: StyleNames,
    pub view: View,
}

impl Docx {
    /// Opens a .docx in the accepted view.
    ///
    /// # Errors
    /// See [`Docx::open_view`].
    pub fn open(bytes: &[u8]) -> Result<Self> {
        Self::open_view(bytes, View::Accepted)
    }

    /// Opens a .docx in the given view.
    ///
    /// # Errors
    /// [`Error`] when the bytes are not a Word document this version can read.
    pub fn open_view(bytes: &[u8], view: View) -> Result<Self> {
        Self::from_package(Package::read(bytes)?, view)
    }

    /// Builds the model of an already-read package.
    ///
    /// # Errors
    /// [`Error`] when the package is not a Word document this version can read.
    pub fn from_package(package: Package, view: View) -> Result<Self> {
        let main_part = main_part(&package)?;
        let main_rels = read_rels(&package, &main_part)?;

        let mut notes_text = BTreeMap::new();
        let mut note_stories = Vec::new();
        for (kind, key) in [
            ("footnotes", StoryKey::Footnotes),
            ("endnotes", StoryKey::Endnotes),
        ] {
            let Some(rel) = main_rels.of_kind(kind).next() else {
                continue;
            };
            let part = resolve_target(&main_part, &rel.target);
            let Some(bytes) = package.part(&part) else {
                continue;
            };
            let text = xml::text(&part, bytes)?;
            let doc = xml::parse(&part, text)?;
            let note_kind = kind.trim_end_matches('s');
            let mut blocks = Vec::new();
            let empty = BTreeMap::new();
            let mut reader = read::Reader::new(text, &part, view, &empty, None);
            for note in doc
                .root_element()
                .children()
                .filter(|n| xml::is(*n, ns::W, note_kind))
            {
                let id = note.attribute((ns::W, "id")).unwrap_or_default();
                if note
                    .attribute((ns::W, "type"))
                    .is_some_and(|t| t != "normal")
                {
                    continue;
                }
                let mut note_blocks = reader.blocks(note, Some(Lock::Note));
                let mut note_text = String::new();
                for block in &mut note_blocks {
                    if let Block::Paragraph(p) = block {
                        p.note_id = Some(id.to_owned());
                    }
                    note_text.push_str(&block.text());
                    note_text.push('\n');
                }
                notes_text.insert(format!("{note_kind}:{id}"), note_text);
                blocks.extend(note_blocks);
            }
            note_stories.push(Story { key, part, blocks });
        }

        let main_bytes = package
            .part(&main_part)
            .ok_or_else(|| Error::NotAWordDocument("main part missing".to_owned()))?;
        let text = xml::text(&main_part, main_bytes).map_err(|e| match e {
            Error::Unsupported(_) => Error::Unsupported("the main document is UTF-16".to_owned()),
            other => other,
        })?;
        let doc = xml::parse(&main_part, text)?;
        let root = doc.root_element();
        if xml::is(root, ns::W_STRICT, "document") {
            return Err(Error::Unsupported("Strict Open XML".to_owned()));
        }
        if !xml::is(root, ns::W, "document") {
            return Err(Error::NotAWordDocument("no w:document".to_owned()));
        }
        let body = xml::child(root, ns::W, "body")
            .ok_or_else(|| Error::NotAWordDocument("no w:body".to_owned()))?;
        let mut reader = read::Reader::new(text, &main_part, view, &notes_text, Some(&main_rels));
        let body_blocks = reader.blocks(body, None);

        let mut stories = vec![Story {
            key: StoryKey::Body,
            part: main_part.clone(),
            blocks: body_blocks,
        }];

        // Headers and footers, keyed by the section that first references them.
        let mut seen_parts: Vec<String> = Vec::new();
        let sections: Vec<SectionBreak> = stories[0]
            .blocks
            .iter()
            .filter_map(|b| match b {
                Block::SectionBreak(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        for (section, sect) in sections.iter().enumerate() {
            for header in &sect.headers {
                if seen_parts.contains(&header.part) {
                    continue;
                }
                let Some(bytes) = package.part(&header.part) else {
                    continue;
                };
                seen_parts.push(header.part.clone());
                let part = package
                    .canonical_name(&header.part)
                    .unwrap_or(&header.part)
                    .to_owned();
                let text = xml::text(&part, bytes)?;
                let doc = xml::parse(&part, text)?;
                let part_rels = read_rels(&package, &part)?;
                let mut reader =
                    read::Reader::new(text, &part, view, &notes_text, Some(&part_rels));
                let blocks = reader.blocks(doc.root_element(), None);
                let key = if header.footer {
                    StoryKey::Footer {
                        section,
                        kind: header.kind.clone(),
                    }
                } else {
                    StoryKey::Header {
                        section,
                        kind: header.kind.clone(),
                    }
                };
                stories.push(Story { key, part, blocks });
            }
        }
        stories.extend(note_stories);

        let styles = main_rels
            .of_kind("styles")
            .next()
            .map(|rel| resolve_target(&main_part, &rel.target))
            .and_then(|part| package.part(&part).map(StyleNames::parse))
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            package,
            main_part,
            main_rels,
            stories,
            styles,
            view,
        })
    }

    /// The main body.
    #[must_use]
    pub fn body(&self) -> &Story {
        &self.stories[0]
    }

    /// A story by key.
    #[must_use]
    pub fn story(&self, key: &StoryKey) -> Option<&Story> {
        self.stories.iter().find(|s| &s.key == key)
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

    /// Every paragraph's text in the body, one per line, tables included (cells
    /// in reading order, cells continuing a vertical merge left out, as Word
    /// shows them). Used by tests and the Word smoke comparison.
    #[must_use]
    pub fn body_lines(&self) -> Vec<String> {
        self.body().lines()
    }
}

impl Story {
    /// Every paragraph's text, one per line, read the way [`Docx::body_lines`]
    /// reads the body.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        collect_lines(&self.blocks, &mut lines);
        lines
    }
}

fn collect_lines(blocks: &[Block], out: &mut Vec<String>) {
    for block in blocks {
        match block {
            Block::Paragraph(p) => out.push(p.text()),
            Block::Table(t) => {
                for row in &t.rows {
                    // A cell that continues a vertical merge shows nothing of its own.
                    for cell in row.cells.iter().filter(|c| c.vmerge != Some(false)) {
                        collect_lines(&cell.blocks, out);
                    }
                }
            }
            Block::SectionBreak(_) | Block::Opaque(_) => {}
        }
    }
}

fn read_rels(package: &Package, part: &str) -> Result<Relationships> {
    let rels_part = rels_part_for(part);
    match package.part(&rels_part) {
        Some(bytes) => Relationships::parse(&rels_part, bytes),
        None => Ok(Relationships::default()),
    }
}

/// Finds the main document part and checks it is a .docx (not .docm or .dotx).
fn main_part(package: &Package) -> Result<String> {
    let types = package
        .part(ContentTypes::PART)
        .ok_or_else(|| Error::NotAWordDocument("no content types".to_owned()))
        .and_then(ContentTypes::parse)?;
    let root_rels = read_rels(package, "")?;
    let rel = root_rels
        .of_kind("officeDocument")
        .next()
        .ok_or_else(|| Error::NotAWordDocument("no main document".to_owned()))?;
    let part = resolve_target("", &rel.target);
    match types.of(&part) {
        Some(ns::CT_DOCUMENT) => {}
        Some(ns::CT_DOCUMENT_MACRO) => {
            return Err(Error::Unsupported("macro-enabled document".to_owned()));
        }
        Some(ns::CT_TEMPLATE) => return Err(Error::Unsupported("Word template".to_owned())),
        _ => {
            return Err(Error::NotAWordDocument(
                "main part is not a Word document".to_owned(),
            ));
        }
    }
    Ok(package.canonical_name(&part).unwrap_or(&part).to_owned())
}

#[cfg(test)]
mod tests;
