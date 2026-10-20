// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Power Query queries (Phase 5B, decision 0030): the M code stored in the
//! workbook's DataMashup item ([MS-QDEFF]), read as text and compared as
//! code, one query at a time. Lineage never evaluates M.
//!
//! The item is base64 of a binary stream: a version, then length-prefixed
//! Package Parts (an OPC package holding `Formulas/Section1.m`),
//! Permissions, Metadata (per-query XML entries: its id, where it loads,
//! when it last loaded) and Permission Bindings, which Lineage ignores. All
//! of it is untrusted, with limits.
//!
//! [MS-QDEFF]: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-qdeff/22557f6d-7c29-4554-8fe4-7b7a54ac7a2b

use crate::code::{self, CodeLine};
use crate::error::{Error, Result};
use crate::hash::Fnv;
use crate::package::Package;
use crate::xml;

pub const NS_MASHUP: &str = "http://schemas.microsoft.com/DataMashup";
/// Largest base64 text Lineage decodes.
pub const MAX_BASE64: usize = 64 * 1024 * 1024;
/// Most queries in one workbook.
pub const MAX_QUERIES: usize = 10_000;

fn damaged(detail: &str) -> Error {
    Error::Damaged(format!("Power Query: {detail}"))
}

/// Where a query's result goes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Load {
    /// A table on a sheet, by the name Excel records.
    pub sheet: Option<String>,
    pub model: bool,
}

impl Load {
    #[must_use]
    pub fn label(&self) -> String {
        match (&self.sheet, self.model) {
            (Some(s), true) => format!("loads to {s} and the data model"),
            (Some(s), false) => format!("loads to {s}"),
            (None, true) => "loads to the data model".to_owned(),
            (None, false) => "is connection only".to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Query {
    pub name: String,
    /// The M expression, one entry per line.
    pub lines: Vec<String>,
    /// `QueryID`, which survives a rename.
    pub id: Option<String>,
    pub load: Load,
    /// What a refresh renews: when it last loaded, and how many rows.
    pub loaded: Option<String>,
    pub rows: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mashup {
    pub part: String,
    pub queries: Vec<Query>,
}

/// The value of one base64 digit.
fn b64(c: u8) -> Option<u32> {
    match c {
        b'A'..=b'Z' => Some(u32::from(c - b'A')),
        b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Decodes standard base64, ignoring whitespace.
///
/// # Errors
/// [`Error::Damaged`] on a character that isn't base64, [`Error::TooLarge`]
/// past [`MAX_BASE64`].
pub fn base64(text: &str) -> Result<Vec<u8>> {
    if text.len() > MAX_BASE64 {
        return Err(Error::TooLarge("Power Query data too large".to_owned()));
    }
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let mut acc = 0u32;
    let mut bits = 0;
    for &c in text.as_bytes() {
        if c.is_ascii_whitespace() {
            continue;
        }
        if c == b'=' {
            break;
        }
        let v = b64(c).ok_or_else(|| damaged("not base64"))?;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((acc >> bits) & 0xFF).unwrap_or(0));
        }
    }
    Ok(out)
}

/// The text of an XML part in UTF-8 or UTF-16, by its byte order mark.
fn xml_text(bytes: &[u8]) -> Result<String> {
    let units = |b: &[u8], le: bool| -> Vec<u16> {
        b.as_chunks::<2>()
            .0
            .iter()
            .map(|c| {
                if le {
                    u16::from_le_bytes(*c)
                } else {
                    u16::from_be_bytes(*c)
                }
            })
            .collect()
    };
    let text = match bytes {
        [0xFF, 0xFE, rest @ ..] => String::from_utf16(&units(rest, true)).ok(),
        [0xFE, 0xFF, rest @ ..] => String::from_utf16(&units(rest, false)).ok(),
        [0xEF, 0xBB, 0xBF, rest @ ..] => std::str::from_utf8(rest).ok().map(str::to_owned),
        _ => std::str::from_utf8(bytes).ok().map(str::to_owned),
    };
    let text = text.ok_or_else(|| damaged("the item's text"))?;
    // roxmltree reads the text it is given; drop an encoding declaration
    // that no longer matches.
    Ok(match text.strip_prefix("<?xml") {
        Some(rest) => rest
            .split_once("?>")
            .map_or(text.clone(), |(_, r)| r.to_owned()),
        None => text,
    })
}

/// Is this custom XML item Power Query's?
#[must_use]
pub fn is_mashup(bytes: &[u8]) -> bool {
    xml_text(bytes).is_ok_and(|t| t.contains("DataMashup"))
}

fn u32_at(b: &[u8], at: usize) -> Result<usize> {
    let s = b.get(at..at + 4).ok_or_else(|| damaged("cut short"))?;
    usize::try_from(u32::from_le_bytes([s[0], s[1], s[2], s[3]])).map_err(|_| damaged("length"))
}

/// The four length-prefixed fields after the version.
fn fields(stream: &[u8]) -> Result<[&[u8]; 4]> {
    let mut at = 4;
    let mut out: [&[u8]; 4] = [&[]; 4];
    for slot in &mut out {
        let len = u32_at(stream, at)?;
        at += 4;
        let end = at.checked_add(len).ok_or_else(|| damaged("length"))?;
        *slot = stream
            .get(at..end)
            .ok_or_else(|| damaged("a field runs past the end"))?;
        at = end;
    }
    Ok(out)
}

/// Splits `Section1.m` into its members: (name, expression).
#[must_use]
pub fn members(source: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = source.chars().collect();
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut statements = Vec::new();
    while i < chars.len() {
        match chars[i] {
            '"' => {
                i += 1;
                while i < chars.len() {
                    if chars[i] == '"' {
                        if chars.get(i + 1) == Some(&'"') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 1;
            }
            ';' => {
                statements.push(chars[start..i].iter().collect::<String>());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    for s in statements {
        let t = strip_leading_trivia(&s);
        if t.starts_with("section") && !t.contains('=') {
            continue;
        }
        let t = t.strip_prefix("shared").map_or(t, str::trim_start);
        let (name, rest) = if let Some(rest) = t.strip_prefix("#\"") {
            // A quoted name: "" is a quote.
            let mut name = String::new();
            let mut it = rest.char_indices().peekable();
            let mut end = rest.len();
            while let Some((k, c)) = it.next() {
                if c == '"' {
                    if it.peek().map(|p| p.1) == Some('"') {
                        name.push('"');
                        it.next();
                        continue;
                    }
                    end = k + 1;
                    break;
                }
                name.push(c);
            }
            (name, &rest[end..])
        } else {
            let end = t
                .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '.'))
                .unwrap_or(t.len());
            (t[..end].to_owned(), &t[end..])
        };
        let Some(expr) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        out.push((name, expr.trim().to_owned()));
    }
    out
}

/// Skips whitespace and comments at the start.
fn strip_leading_trivia(s: &str) -> &str {
    let mut t = s.trim_start();
    loop {
        if let Some(rest) = t.strip_prefix("//") {
            t = rest.split_once('\n').map_or("", |(_, r)| r).trim_start();
        } else if let Some(rest) = t.strip_prefix("/*") {
            t = rest.split_once("*/").map_or("", |(_, r)| r).trim_start();
        } else {
            return t;
        }
    }
}

fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim_end_matches('\r').to_owned())
        .collect()
}

fn unescape_path(p: &str) -> String {
    let bytes = p.as_bytes();
    let hex = |b: u8| char::from(b).to_digit(16);
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push(u8::try_from(h * 16 + l).unwrap_or(b'?'));
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[derive(Default)]
struct Meta {
    id: Option<String>,
    fill: bool,
    model: bool,
    target: Option<String>,
    loaded: Option<String>,
    rows: Option<String>,
}

/// Per-query metadata, by query name.
fn metadata(stream: &[u8]) -> Vec<(String, Meta)> {
    let Ok(len) = u32_at(stream, 4) else {
        return Vec::new();
    };
    let Some(xml_bytes) = stream.get(8..8 + len) else {
        return Vec::new();
    };
    let Ok(text) = xml_text(xml_bytes) else {
        return Vec::new();
    };
    let Ok(doc) = roxmltree::Document::parse(&text) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for item in doc.descendants().filter(|n| n.has_tag_name("Item")) {
        let loc = item.children().find(|n| n.has_tag_name("ItemLocation"));
        let kind = loc
            .and_then(|l| l.children().find(|n| n.has_tag_name("ItemType")))
            .and_then(|n| n.text());
        let path = loc
            .and_then(|l| l.children().find(|n| n.has_tag_name("ItemPath")))
            .and_then(|n| n.text());
        let (Some("Formula"), Some(path)) = (kind, path) else {
            continue;
        };
        let Some(name) = path.strip_prefix("Section1/") else {
            continue;
        };
        let mut m = Meta::default();
        for e in item.descendants().filter(|n| n.has_tag_name("Entry")) {
            let value = e.attribute("Value").unwrap_or("");
            // Values carry a type letter: l (number), s (text), d (date).
            let v = value.get(1..).unwrap_or("").to_owned();
            match e.attribute("Type") {
                Some("QueryID") => m.id = Some(v),
                Some("FillEnabled") => m.fill = v == "1",
                Some("FillToDataModelEnabled") => m.model = v == "1",
                Some("FillTarget") => m.target = Some(v),
                Some("FillLastUpdated") => m.loaded = Some(v),
                Some("FillCount") => m.rows = Some(v),
                _ => {}
            }
        }
        out.push((unescape_path(name), m));
    }
    out
}

/// Reads the queries in a DataMashup custom XML item.
///
/// # Errors
/// [`Error::Damaged`] when the item, the stream or its package is
/// malformed.
pub fn read(part: &str, bytes: &[u8]) -> Result<Mashup> {
    let text = xml_text(bytes)?;
    let doc = roxmltree::Document::parse(&text).map_err(|e| Error::BadXml {
        part: part.to_owned(),
        detail: e.to_string(),
    })?;
    let root = doc.root_element();
    if !xml::is(root, NS_MASHUP, "DataMashup") {
        return Err(damaged("not a DataMashup item"));
    }
    let stream = base64(root.text().unwrap_or(""))?;
    let [package, _permissions, meta, _bindings] = fields(&stream)?;
    let package = Package::read(package)?;
    let source = package
        .part("Formulas/Section1.m")
        .ok_or_else(|| damaged("no Section1.m"))?;
    let source = String::from_utf8_lossy(source.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(source));
    let meta = metadata(meta);
    let mut queries = Vec::new();
    for (name, expr) in members(&source) {
        if queries.len() == MAX_QUERIES {
            return Err(Error::TooLarge("too many queries".to_owned()));
        }
        let m = meta.iter().find(|(n, _)| *n == name).map(|(_, m)| m);
        queries.push(Query {
            lines: lines(&expr),
            id: m.and_then(|m| m.id.clone()),
            load: Load {
                sheet: m
                    .filter(|m| m.fill)
                    .map(|m| m.target.clone().unwrap_or_else(|| "a sheet".to_owned())),
                model: m.is_some_and(|m| m.model),
            },
            loaded: m.and_then(|m| m.loaded.clone()),
            rows: m.and_then(|m| m.rows.clone()),
            name,
        });
    }
    Ok(Mashup {
        part: part.to_owned(),
        queries,
    })
}

// --- What changed ------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryChangeKind {
    Added,
    Removed,
    Renamed {
        from: String,
    },
    Changed,
    /// Only its result was loaded again: never an edit.
    Refreshed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryChange {
    pub id: String,
    pub name: String,
    pub kind: QueryChangeKind,
    pub lines: usize,
    pub code: Vec<CodeLine>,
    pub details: Vec<String>,
    pub summary: String,
    /// The query's whole M code in the later version, for "Copy code"
    /// (`None` for a removed query).
    pub text: Option<String>,
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "line" } else { "lines" }
}

fn change_id(parts: &[&str]) -> String {
    crate::hash::sha256_hex(parts.join("\u{1f}").as_bytes())[..16].to_owned()
}

/// What changed in the queries from `a` to `b`.
#[must_use]
pub fn diff(a: Option<&Mashup>, b: Option<&Mashup>) -> Vec<QueryChange> {
    let none: Vec<Query> = Vec::new();
    let qa = a.map_or(&none, |m| &m.queries);
    let qb = b.map_or(&none, |m| &m.queries);
    let mut used_a = vec![false; qa.len()];
    let mut pairs: Vec<(Option<usize>, usize)> = Vec::new();
    for (j, q) in qb.iter().enumerate() {
        let by_id = q.id.as_ref().and_then(|id| {
            qa.iter()
                .enumerate()
                .position(|(i, p)| !used_a[i] && p.id.as_ref() == Some(id))
        });
        let by_name = || {
            qa.iter()
                .enumerate()
                .position(|(i, p)| !used_a[i] && p.name == q.name)
        };
        let found = by_id.or_else(by_name);
        if let Some(i) = found {
            used_a[i] = true;
        }
        pairs.push((found, j));
    }
    // Renames without ids: alike code.
    for pair in &mut pairs {
        if pair.0.is_some() {
            continue;
        }
        let q = &qb[pair.1];
        let best = qa
            .iter()
            .enumerate()
            .filter(|(i, _)| !used_a[*i])
            .map(|(i, p)| (i, code::similar(&p.lines, &q.lines)))
            .filter(|(_, s)| *s >= crate::vba::diff::RENAME_SIMILARITY)
            .max_by_key(|(_, s)| *s);
        if let Some((i, _)) = best {
            used_a[i] = true;
            pair.0 = Some(i);
        }
    }
    let mut out = Vec::new();
    for (i, j) in pairs {
        let q = &qb[j];
        let Some(i) = i else {
            let d = code::line_diff(&[], &q.lines);
            out.push(QueryChange {
                id: change_id(&["query", &q.name, "added"]),
                name: q.name.clone(),
                kind: QueryChangeKind::Added,
                lines: q.lines.len(),
                code: code::view(&[], &q.lines, &d),
                details: vec![format!("It {}", q.load.label())],
                summary: format!(
                    "Added query {} ({} {})",
                    q.name,
                    q.lines.len(),
                    plural(q.lines.len())
                ),
                text: Some(q.lines.join("\r\n")),
            });
            continue;
        };
        let p = &qa[i];
        let d = code::line_diff(&p.lines, &q.lines);
        let lines = d.changed();
        let mut details = Vec::new();
        if p.load != q.load {
            details.push(format!("It now {} (it {})", q.load.label(), p.load.label()));
        }
        let renamed = p.name != q.name;
        let refreshed = p.loaded != q.loaded || p.rows != q.rows;
        let kind = if renamed {
            QueryChangeKind::Renamed {
                from: p.name.clone(),
            }
        } else if lines > 0 || !details.is_empty() {
            QueryChangeKind::Changed
        } else if refreshed {
            QueryChangeKind::Refreshed
        } else {
            continue;
        };
        if refreshed && kind != QueryChangeKind::Refreshed {
            details.push("Its result was loaded again".to_owned());
        }
        let summary = match &kind {
            QueryChangeKind::Renamed { from } if lines == 0 => {
                format!("Renamed query {from} to {}", q.name)
            }
            QueryChangeKind::Renamed { from } => format!(
                "Renamed query {from} to {}, and {lines} {} changed",
                q.name,
                plural(lines)
            ),
            QueryChangeKind::Changed if lines > 0 => {
                format!("Query {}: {lines} {} changed", q.name, plural(lines))
            }
            QueryChangeKind::Changed => format!("Query {} now {}", q.name, q.load.label()),
            _ => match (&q.loaded, &q.rows) {
                (Some(when), Some(rows)) => format!(
                    "Query {} was refreshed ({rows} rows, {})",
                    q.name,
                    when.split('T').next().unwrap_or(when)
                ),
                _ => format!("Query {} was refreshed", q.name),
            },
        };
        let key = Fnv::new().str(&q.lines.join("\n")).finish();
        out.push(QueryChange {
            id: change_id(&["query", &q.name, &format!("{kind:?}"), &key.to_string()]),
            name: q.name.clone(),
            kind,
            lines,
            code: code::view(&p.lines, &q.lines, &d),
            details,
            summary,
            text: Some(q.lines.join("\r\n")),
        });
    }
    for (i, p) in qa.iter().enumerate() {
        if !used_a[i] {
            out.push(QueryChange {
                id: change_id(&["query", &p.name, "removed"]),
                name: p.name.clone(),
                kind: QueryChangeKind::Removed,
                lines: p.lines.len(),
                code: code::view(&p.lines, &[], &code::line_diff(&p.lines, &[])),
                details: Vec::new(),
                summary: format!("Removed query {}", p.name),
                text: None,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests;
