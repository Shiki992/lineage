// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! XML reading and byte-range editing.
//!
//! Parts are parsed with roxmltree, which gives the byte range of every node and
//! attribute in the original text. Changes are made as [`Edits`]: byte ranges of
//! the original replaced by new text. Nothing outside those ranges is ever
//! re-serialised, so unknown elements, attributes, namespace prefixes, comments and
//! whitespace stay exactly as they were.

pub mod ns;

use std::borrow::Cow;
use std::ops::Range;

use crate::error::{Error, Result};

/// Most nodes one part may have. Generous for real documents, small enough to
/// stop a hostile file using all memory.
pub const NODES_LIMIT: u32 = 8_000_000;

/// The part's text, if it is UTF-8. UTF-16 parts are valid XML but are left
/// untouched (and unreadable to the engine) in this version.
///
/// # Errors
/// [`Error::Unsupported`] for UTF-16, [`Error::BadXml`] for anything else that is not UTF-8.
pub fn text<'a>(part: &str, bytes: &'a [u8]) -> Result<&'a str> {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err(Error::Unsupported(format!("{part} is UTF-16")));
    }
    std::str::from_utf8(bytes).map_err(|_| Error::BadXml {
        part: part.to_owned(),
        detail: "not UTF-8".to_owned(),
    })
}

/// Parses XML text. DTDs are refused (no entity expansion) and the node count is capped.
///
/// # Errors
/// [`Error::BadXml`] when the text is not well-formed or breaks a limit.
pub fn parse<'a>(part: &str, text: &'a str) -> Result<roxmltree::Document<'a>> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: false,
        nodes_limit: NODES_LIMIT,
        ..roxmltree::ParsingOptions::default()
    };
    roxmltree::Document::parse_with_options(text, options).map_err(|e| Error::BadXml {
        part: part.to_owned(),
        detail: e.to_string(),
    })
}

/// Escapes text content.
#[must_use]
pub fn escape_text(text: &str) -> Cow<'_, str> {
    if !text.contains(['&', '<', '>']) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(ch),
        }
    }
    Cow::Owned(out)
}

/// Escapes an attribute value for use inside double quotes.
#[must_use]
pub fn escape_attr(text: &str) -> Cow<'_, str> {
    if !text.contains(['&', '<', '>', '"', '\t', '\n', '\r']) {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len() + 8);
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            _ => out.push(ch),
        }
    }
    Cow::Owned(out)
}

/// One change to a part: replace `range` of the original text with `text`.
/// An empty range is an insertion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
}

/// A set of edits to one part, applied together against the original text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Edits {
    list: Vec<Edit>,
}

impl Edits {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    /// Replaces `range` with `text`.
    pub fn replace(&mut self, range: Range<usize>, text: impl Into<String>) {
        self.list.push(Edit {
            range,
            text: text.into(),
        });
    }

    /// Inserts `text` at `at`. Insertions at the same place keep the order they were added in.
    pub fn insert(&mut self, at: usize, text: impl Into<String>) {
        self.replace(at..at, text);
    }

    /// Deletes `range`.
    pub fn delete(&mut self, range: Range<usize>) {
        self.replace(range, String::new());
    }

    /// The index of the most recently added edit.
    #[must_use]
    pub fn last_index(&self) -> Option<usize> {
        self.list.len().checked_sub(1)
    }

    /// The edit at `index`, to change its range or text before applying.
    pub fn get_mut(&mut self, index: usize) -> Option<&mut Edit> {
        self.list.get_mut(index)
    }

    /// The replacement text of the edit at `index`.
    #[must_use]
    pub fn list_text(&self, index: usize) -> Option<String> {
        self.list.get(index).map(|e| e.text.clone())
    }

    /// True when an insertion (with text) falls within `range`.
    #[must_use]
    pub fn inserts_within(&self, range: &std::ops::RangeInclusive<usize>) -> bool {
        self.list
            .iter()
            .any(|e| e.range.is_empty() && !e.text.is_empty() && range.contains(&e.range.start))
    }

    /// The index of the replacement whose range is exactly `range` (never an insertion).
    #[must_use]
    pub fn find_replacement(&self, range: &Range<usize>) -> Option<usize> {
        self.list
            .iter()
            .position(|e| !e.range.is_empty() && e.range == *range)
    }

    /// Applies every edit to `source`.
    ///
    /// # Errors
    /// [`Error::CannotEdit`] when edits overlap, fall outside the text, or split a
    /// character. Nothing is applied in that case.
    pub fn apply(&self, source: &str) -> Result<String> {
        let mut order: Vec<&Edit> = self.list.iter().collect();
        // Stable: insertions at one point stay in the order they were added, and
        // come before a replacement that starts at the same point.
        order.sort_by_key(|edit| (edit.range.start, !edit.range.is_empty()));
        let mut out = String::with_capacity(source.len() + 64);
        let mut cursor = 0;
        for edit in order {
            let Range { start, end } = edit.range.clone();
            if start < cursor || end < start || end > source.len() {
                return Err(Error::CannotEdit("overlapping or invalid edits".to_owned()));
            }
            if !source.is_char_boundary(start) || !source.is_char_boundary(end) {
                return Err(Error::CannotEdit("edit splits a character".to_owned()));
            }
            out.push_str(&source[cursor..start]);
            out.push_str(&edit.text);
            cursor = end;
        }
        out.push_str(&source[cursor..]);
        Ok(out)
    }
}

/// Byte offset just after the `>` that ends `node`'s start tag.
#[must_use]
pub fn start_tag_end(source: &str, node: roxmltree::Node<'_, '_>) -> usize {
    let bytes = source.as_bytes();
    let mut quote = None;
    let mut index = node.range().start;
    while index < bytes.len() {
        let byte = bytes[index];
        match quote {
            Some(q) if byte == q => quote = None,
            Some(_) => {}
            None if byte == b'"' || byte == b'\'' => quote = Some(byte),
            None if byte == b'>' => return index + 1,
            None => {}
        }
        index += 1;
    }
    node.range().end
}

/// True when `node` is written as `<x/>`.
#[must_use]
pub fn is_self_closing(source: &str, node: roxmltree::Node<'_, '_>) -> bool {
    let end = start_tag_end(source, node);
    end >= 2 && &source[end - 2..end] == "/>"
}

/// Byte offset of the `</` that starts `node`'s end tag, or `None` for `<x/>`.
#[must_use]
pub fn end_tag_start(source: &str, node: roxmltree::Node<'_, '_>) -> Option<usize> {
    if is_self_closing(source, node) {
        return None;
    }
    let range = node.range();
    source[range.clone()]
        .rfind("</")
        .map(|offset| range.start + offset)
}

/// The element's qualified name as written in the source, such as `w:p`.
#[must_use]
pub fn qname<'s>(source: &'s str, node: roxmltree::Node<'_, '_>) -> &'s str {
    let start = node.range().start + 1;
    let rest = &source[start..];
    let len = rest
        .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
        .unwrap_or(rest.len());
    &rest[..len]
}

/// The prefix of the element's qualified name (`w` for `w:p`), or "" if none.
#[must_use]
pub fn prefix_of<'s>(source: &'s str, node: roxmltree::Node<'_, '_>) -> &'s str {
    qname(source, node).split_once(':').map_or("", |(p, _)| p)
}

/// Adds `fragment` as the last child of `node`, expanding `<x/>` if needed.
pub fn append_child(
    edits: &mut Edits,
    source: &str,
    node: roxmltree::Node<'_, '_>,
    fragment: &str,
) {
    if let Some(at) = end_tag_start(source, node) {
        edits.insert(at, fragment);
    } else {
        let end = start_tag_end(source, node);
        let name = qname(source, node);
        // Drop "/>" (and any space before it) and close the element properly.
        let from = source[..end - 2].trim_end().len();
        edits.replace(from..end, format!(">{fragment}</{name}>"));
    }
}

/// Adds `fragment` as the first child of `node`, expanding `<x/>` if needed.
pub fn prepend_child(
    edits: &mut Edits,
    source: &str,
    node: roxmltree::Node<'_, '_>,
    fragment: &str,
) {
    if is_self_closing(source, node) {
        append_child(edits, source, node, fragment);
    } else {
        edits.insert(start_tag_end(source, node), fragment);
    }
}

/// The first child element with this namespace and local name.
#[must_use]
pub fn child<'a, 'i>(
    node: roxmltree::Node<'a, 'i>,
    namespace: &str,
    name: &str,
) -> Option<roxmltree::Node<'a, 'i>> {
    node.children().find(|c| is(*c, namespace, name))
}

/// True when `node` is an element with this namespace and local name.
#[must_use]
pub fn is(node: roxmltree::Node<'_, '_>, namespace: &str, name: &str) -> bool {
    node.is_element()
        && node.tag_name().name() == name
        && node.tag_name().namespace() == Some(namespace)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_leave_everything_else_untouched() {
        let source = r#"<a x="1"><!-- keep --><b/>text<c y='2'>more</c></a>"#;
        let mut edits = Edits::new();
        edits.replace(22..26, "<b>new</b>");
        edits.insert(source.len() - 4, "<d/>");
        let out = edits.apply(source).expect("applies");
        assert_eq!(
            out,
            r#"<a x="1"><!-- keep --><b>new</b>text<c y='2'>more</c><d/></a>"#
        );
    }

    #[test]
    fn overlapping_edits_are_refused() {
        let mut edits = Edits::new();
        edits.replace(0..4, "x");
        edits.replace(2..6, "y");
        assert!(matches!(edits.apply("abcdefgh"), Err(Error::CannotEdit(_))));

        let mut inside = Edits::new();
        inside.replace(0..4, "x");
        inside.insert(2, "y");
        assert!(inside.apply("abcdefgh").is_err());
    }

    #[test]
    fn insertions_at_one_point_keep_their_order() {
        let mut edits = Edits::new();
        edits.insert(1, "1");
        edits.insert(1, "2");
        edits.replace(1..2, "B");
        assert_eq!(edits.apply("abc").expect("applies"), "a12Bc");

        // Added after the replacement, an insertion at its start still goes first.
        let mut later = Edits::new();
        later.replace(1..2, "B");
        later.insert(1, "1");
        later.insert(2, "2");
        assert_eq!(later.apply("abc").expect("applies"), "a1B2c");
    }

    #[test]
    fn edits_never_split_characters() {
        let mut edits = Edits::new();
        edits.insert(1, "x");
        assert!(edits.apply("é").is_err());
    }

    #[test]
    fn tag_helpers_find_boundaries() {
        let source = r#"<w:p xmlns:w="urn:w" xmlns:x="urn:x" a=">"><w:r/><x:y  /></w:p>"#;
        let doc = parse("t", source).expect("parses");
        let p = doc.root_element();
        assert_eq!(start_tag_end(source, p), 43);
        assert_eq!(qname(source, p), "w:p");
        assert_eq!(prefix_of(source, p), "w");
        assert_eq!(end_tag_start(source, p), Some(source.len() - 6));
        let empty = p.children().nth(1).expect("second child");
        assert!(is_self_closing(source, empty));
        let mut edits = Edits::new();
        append_child(&mut edits, source, empty, "<z/>");
        assert_eq!(
            edits.apply(source).expect("applies"),
            r#"<w:p xmlns:w="urn:w" xmlns:x="urn:x" a=">"><w:r/><x:y><z/></x:y></w:p>"#
        );
    }

    #[test]
    fn dtds_are_refused() {
        let hostile = r#"<?xml version="1.0"?><!DOCTYPE a [<!ENTITY x "y">]><a>&x;</a>"#;
        assert!(matches!(parse("t", hostile), Err(Error::BadXml { .. })));
    }

    #[test]
    fn escaping() {
        assert_eq!(escape_text("a<b & c>"), "a&lt;b &amp; c&gt;");
        assert_eq!(escape_attr("say \"hi\"\n"), "say &quot;hi&quot;&#10;");
        assert!(matches!(escape_text("plain"), Cow::Borrowed(_)));
    }

    #[test]
    fn utf16_parts_are_reported() {
        assert!(matches!(
            text("p", &[0xFF, 0xFE, b'<', 0]),
            Err(Error::Unsupported(_))
        ));
        assert!(matches!(text("p", &[0xC3]), Err(Error::BadXml { .. })));
    }
}
