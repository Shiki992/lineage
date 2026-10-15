// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What the writer needs to know about each story part: namespace prefixes
//! declared on the root, and the part's relationships.

use std::collections::BTreeMap;

use crate::docx::Docx;
use crate::package::{Relationships, rels_part_for, resolve_target};
use crate::xml::{self, ns};

/// Namespaces and relationships of one part.
#[derive(Clone, Debug, Default)]
pub struct PartInfo {
    /// Prefix → URI, as declared on the root element.
    pub namespaces: BTreeMap<String, String>,
    pub rels: Relationships,
}

impl PartInfo {
    /// The prefix bound to WordprocessingML.
    #[must_use]
    pub fn w_prefix(&self) -> Option<&str> {
        self.namespaces
            .iter()
            .find(|(prefix, uri)| uri.as_str() == ns::W && !prefix.is_empty())
            .map(|(prefix, _)| prefix.as_str())
    }

    #[must_use]
    pub fn r_prefix(&self) -> Option<&str> {
        self.namespaces
            .iter()
            .find(|(prefix, uri)| uri.as_str() == ns::R && !prefix.is_empty())
            .map(|(prefix, _)| prefix.as_str())
    }
}

/// Part information for every story part of a document.
#[derive(Clone, Debug, Default)]
pub struct DocInfo {
    pub parts: BTreeMap<String, PartInfo>,
}

impl DocInfo {
    #[must_use]
    pub fn new(docx: &Docx) -> Self {
        let mut parts = BTreeMap::new();
        for story in &docx.stories {
            if parts.contains_key(&story.part) {
                continue;
            }
            let mut info = PartInfo::default();
            if let Ok(text) = docx.part_text(&story.part)
                && let Ok(doc) = xml::parse(&story.part, text)
            {
                for namespace in doc.root_element().namespaces() {
                    info.namespaces.insert(
                        namespace.name().unwrap_or_default().to_owned(),
                        namespace.uri().to_owned(),
                    );
                }
            }
            let rels_part = rels_part_for(&story.part);
            if let Some(bytes) = docx.package.part(&rels_part)
                && let Ok(rels) = Relationships::parse(&rels_part, bytes)
            {
                info.rels = rels;
            }
            parts.insert(story.part.clone(), info);
        }
        Self { parts }
    }

    #[must_use]
    pub fn part(&self, part: &str) -> Option<&PartInfo> {
        self.parts.get(part)
    }
}

/// Prefixes used in an XML fragment (element and attribute names).
#[must_use]
pub fn prefixes_used(fragment: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let bytes = fragment.as_bytes();
    let mut in_tag = false;
    let mut quote: Option<u8> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'<' => {
                in_tag = true;
                i += 1;
                if i < bytes.len() && (bytes[i] == b'/' || bytes[i] == b'?' || bytes[i] == b'!') {
                    i += 1;
                }
                read_name(bytes, &mut i, &mut out);
                continue;
            }
            b'>' => in_tag = false,
            b'"' | b'\'' if in_tag => quote = Some(c),
            c if in_tag && c.is_ascii_whitespace() => {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                read_name(bytes, &mut i, &mut out);
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    out.retain(|p| p != "xmlns" && p != "xml");
    out
}

fn read_name(bytes: &[u8], i: &mut usize, out: &mut Vec<String>) {
    let start = *i;
    while *i < bytes.len()
        && !matches!(bytes[*i], b' ' | b'\t' | b'\r' | b'\n' | b'>' | b'/' | b'=')
    {
        *i += 1;
    }
    let name = String::from_utf8_lossy(&bytes[start..*i]);
    if let Some((prefix, _)) = name.split_once(':')
        && !prefix.is_empty()
        && !out.iter().any(|p| p == prefix)
    {
        out.push(prefix.to_owned());
    }
}

/// True when every prefix the fragment uses means the same in both parts.
#[must_use]
pub fn prefixes_compatible(fragment: &str, from: &PartInfo, to: &PartInfo) -> bool {
    prefixes_used(fragment)
        .iter()
        .all(|prefix| from.namespaces.get(prefix) == to.namespaces.get(prefix))
}

/// The part a relationship points to, resolved against its source part.
#[must_use]
pub fn rel_target(part: &str, rels: &Relationships, id: &str) -> Option<(String, bool, String)> {
    let rel = rels.by_id(id)?;
    let target = if rel.external {
        rel.target.clone()
    } else {
        resolve_target(part, &rel.target)
    };
    Some((rel.rel_type.clone(), rel.external, target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_prefixes_in_names_not_values() {
        let fragment = r#"<w:r w14:x="a:b"><w:rPr/><w:t xml:space="preserve">c:d</w:t><a:graphic r:embed='e:f'/></w:r>"#;
        assert_eq!(prefixes_used(fragment), ["w", "w14", "a", "r"]);
    }
}
