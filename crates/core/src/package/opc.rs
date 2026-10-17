// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Open Packaging Conventions: content types, relationships and part names.

use crate::error::{Error, Result};
use crate::xml::{self, ns};

/// The folder a part lives in: `word` for `word/document.xml`, "" at the root.
#[must_use]
pub fn part_dir(part: &str) -> &str {
    part.trim_start_matches('/')
        .rsplit_once('/')
        .map_or("", |(dir, _)| dir)
}

/// The relationships part for `part`, or the package relationships for "".
#[must_use]
pub fn rels_part_for(part: &str) -> String {
    let part = part.trim_start_matches('/');
    if part.is_empty() {
        return "_rels/.rels".to_owned();
    }
    match part.rsplit_once('/') {
        Some((dir, file)) => format!("{dir}/_rels/{file}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// Resolves a relationship target against the part that holds the relationship.
/// `source_part` is "" for package relationships.
#[must_use]
pub fn resolve_target(source_part: &str, target: &str) -> String {
    let joined = if let Some(absolute) = target.strip_prefix('/') {
        absolute.to_owned()
    } else {
        let dir = part_dir(source_part);
        if dir.is_empty() {
            target.to_owned()
        } else {
            format!("{dir}/{target}")
        }
    };
    let mut segments: Vec<&str> = Vec::new();
    for segment in joined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            other => segments.push(other),
        }
    }
    segments.join("/")
}

/// The target to write in `source_part`'s relationships so it reaches `part`:
/// relative to the source's folder, as Office writes them (`../media/a.png`).
#[must_use]
pub fn relative_target(source_part: &str, part: &str) -> String {
    let from: Vec<&str> = part_dir(source_part)
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let to: Vec<&str> = part
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let (to_dir, file) = to.split_at(to.len().saturating_sub(1));
    let common = from
        .iter()
        .zip(to_dir.iter())
        .take_while(|(a, b)| a.eq_ignore_ascii_case(b))
        .count();
    let mut out: Vec<&str> = std::iter::repeat_n("..", from.len() - common).collect();
    out.extend(&to_dir[common..]);
    out.extend(file);
    out.join("/")
}

/// `[Content_Types].xml`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ContentTypes {
    defaults: Vec<(String, String)>,
    overrides: Vec<(String, String)>,
}

impl ContentTypes {
    /// The part name of the content types stream.
    pub const PART: &'static str = "[Content_Types].xml";

    /// Parses `[Content_Types].xml`.
    ///
    /// # Errors
    /// [`Error::BadXml`] when the part is not readable.
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let text = xml::text(Self::PART, bytes)?;
        let doc = xml::parse(Self::PART, text)?;
        let root = doc.root_element();
        if !xml::is(root, ns::CONTENT_TYPES, "Types") {
            return Err(Error::NotAnOfficeFile("no content types".to_owned()));
        }
        let mut types = Self::default();
        for node in root.children().filter(roxmltree::Node::is_element) {
            let content_type = node.attribute("ContentType").unwrap_or_default().to_owned();
            if xml::is(node, ns::CONTENT_TYPES, "Default") {
                if let Some(ext) = node.attribute("Extension") {
                    types
                        .defaults
                        .push((ext.to_ascii_lowercase(), content_type));
                }
            } else if xml::is(node, ns::CONTENT_TYPES, "Override")
                && let Some(part) = node.attribute("PartName")
            {
                types.overrides.push((
                    part.trim_start_matches('/').to_ascii_lowercase(),
                    content_type,
                ));
            }
        }
        Ok(types)
    }

    /// The content type of a part: its override, else the default for its extension.
    #[must_use]
    pub fn of(&self, part: &str) -> Option<&str> {
        let part = part.trim_start_matches('/').to_ascii_lowercase();
        if let Some((_, ct)) = self.overrides.iter().find(|(name, _)| *name == part) {
            return Some(ct);
        }
        let ext = part.rsplit_once('.').map(|(_, ext)| ext)?;
        self.defaults
            .iter()
            .find(|(e, _)| e == ext)
            .map(|(_, ct)| ct.as_str())
    }

    /// True when the part has an `Override` entry.
    #[must_use]
    pub fn has_override(&self, part: &str) -> bool {
        let part = part.trim_start_matches('/').to_ascii_lowercase();
        self.overrides.iter().any(|(name, _)| *name == part)
    }

    /// Parts with an `Override` entry, lower-cased.
    pub fn override_parts(&self) -> impl Iterator<Item = &str> {
        self.overrides.iter().map(|(name, _)| name.as_str())
    }
}

/// One relationship from a `.rels` part.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Relationship {
    pub id: String,
    pub rel_type: String,
    pub target: String,
    pub external: bool,
}

/// The relationships of one part (or of the package).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Relationships {
    pub items: Vec<Relationship>,
}

impl Relationships {
    /// Parses a `.rels` part.
    ///
    /// # Errors
    /// [`Error::BadXml`] when the part is not readable.
    pub fn parse(part: &str, bytes: &[u8]) -> Result<Self> {
        let text = xml::text(part, bytes)?;
        let doc = xml::parse(part, text)?;
        let root = doc.root_element();
        if !xml::is(root, ns::PACKAGE_RELS, "Relationships") {
            return Err(Error::BadXml {
                part: part.to_owned(),
                detail: "not a relationships part".to_owned(),
            });
        }
        let items = root
            .children()
            .filter(|n| xml::is(*n, ns::PACKAGE_RELS, "Relationship"))
            .filter_map(|n| {
                Some(Relationship {
                    id: n.attribute("Id")?.to_owned(),
                    rel_type: n.attribute("Type")?.to_owned(),
                    target: n.attribute("Target")?.to_owned(),
                    external: n.attribute("TargetMode") == Some("External"),
                })
            })
            .collect();
        Ok(Self { items })
    }

    /// The relationship with this id.
    #[must_use]
    pub fn by_id(&self, id: &str) -> Option<&Relationship> {
        self.items.iter().find(|rel| rel.id == id)
    }

    /// Relationships whose type ends in `/{kind}`, such as `officeDocument` or `header`.
    pub fn of_kind<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Relationship> + 'a {
        self.items.iter().filter(move |rel| {
            rel.rel_type
                .rsplit_once('/')
                .is_some_and(|(_, tail)| tail == kind)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rels_names() {
        assert_eq!(rels_part_for(""), "_rels/.rels");
        assert_eq!(
            rels_part_for("word/document.xml"),
            "word/_rels/document.xml.rels"
        );
        assert_eq!(
            rels_part_for("/word/document.xml"),
            "word/_rels/document.xml.rels"
        );
        assert_eq!(rels_part_for("x.xml"), "_rels/x.xml.rels");
    }

    #[test]
    fn targets_resolve_relative_to_the_source_part() {
        assert_eq!(resolve_target("", "word/document.xml"), "word/document.xml");
        assert_eq!(
            resolve_target("word/document.xml", "media/a.png"),
            "word/media/a.png"
        );
        assert_eq!(
            resolve_target("word/document.xml", "../customXml/item1.xml"),
            "customXml/item1.xml"
        );
        assert_eq!(
            resolve_target("word/document.xml", "/word/styles.xml"),
            "word/styles.xml"
        );
        assert_eq!(
            resolve_target("word/document.xml", "./styles.xml"),
            "word/styles.xml"
        );
    }

    #[test]
    fn relative_targets_round_trip() {
        for (source, part, want) in [
            (
                "ppt/slides/slide1.xml",
                "ppt/media/image1.png",
                "../media/image1.png",
            ),
            (
                "ppt/slides/slide1.xml",
                "ppt/slides/slide2.xml",
                "slide2.xml",
            ),
            (
                "ppt/presentation.xml",
                "ppt/slides/slide3.xml",
                "slides/slide3.xml",
            ),
            ("", "docProps/custom.xml", "docProps/custom.xml"),
            (
                "ppt/charts/chart1.xml",
                "ppt/embeddings/Book1.xlsx",
                "../embeddings/Book1.xlsx",
            ),
        ] {
            let target = relative_target(source, part);
            assert_eq!(target, want);
            assert_eq!(resolve_target(source, &target), part);
        }
    }

    #[test]
    fn content_types_use_overrides_then_defaults() {
        let xml = br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="XML" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="doc"/></Types>"#;
        let types = ContentTypes::parse(xml).expect("parses");
        assert_eq!(types.of("word/document.xml"), Some("doc"));
        assert_eq!(types.of("/Word/Document.xml"), Some("doc"));
        assert_eq!(types.of("word/other.xml"), Some("application/xml"));
        assert_eq!(types.of("word/media/a.png"), None);
        assert!(types.has_override("word/document.xml"));
    }

    #[test]
    fn relationships_by_kind_and_id() {
        let xml = br#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/><Relationship Id="rId9" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/hyperlink" Target="https://example.com" TargetMode="External"/></Relationships>"#;
        let rels = Relationships::parse("_rels/.rels", xml).expect("parses");
        assert_eq!(rels.of_kind("officeDocument").count(), 1);
        assert!(rels.by_id("rId9").expect("rId9").external);
        assert!(rels.by_id("rId2").is_none());
    }
}
