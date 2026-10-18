// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Pull-parsing helpers on quick-xml (decision 0025): namespaces resolved,
//! positions tracked, entity references resolved without a DTD, and
//! subtrees hashed into canonical keys.

use std::borrow::Cow;

use quick_xml::events::{BytesRef, BytesStart, Event};
use quick_xml::name::{QName, ResolveResult};
use quick_xml::reader::NsReader;

use super::model::{Span, Str};
use crate::error::{Error, Result};
use crate::hash::Fnv;

/// SpreadsheetML's namespace (transitional), and the Strict one this version
/// doesn't read.
pub const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
pub const MAIN_STRICT: &str = "http://purl.oclc.org/ooxml/spreadsheetml/main";
pub const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// One event, with where it started and whether its element is in the main
/// namespace.
pub struct Ev<'i> {
    pub event: Event<'i>,
    /// Byte offset where the event starts.
    pub at: usize,
    /// Byte offset just after it.
    pub end: usize,
    pub main: bool,
    pub strict: bool,
}

pub struct Pull<'i> {
    pub reader: NsReader<&'i [u8]>,
    pub part: String,
    pub text: &'i str,
}

impl<'i> Pull<'i> {
    pub fn new(part: &str, text: &'i str) -> Self {
        let mut reader = NsReader::from_str(text);
        let config = reader.config_mut();
        config.trim_text(false);
        config.check_end_names = true;
        config.expand_empty_elements = false;
        Self {
            reader,
            part: part.to_owned(),
            text,
        }
    }

    pub fn bad(&self, detail: impl Into<String>) -> Error {
        Error::BadXml {
            part: self.part.clone(),
            detail: detail.into(),
        }
    }

    fn pos(&self) -> usize {
        usize::try_from(self.reader.buffer_position()).unwrap_or(usize::MAX)
    }

    /// The next event. DTDs are refused, as for every part.
    pub fn next(&mut self) -> Result<Ev<'i>> {
        let at = self.pos();
        let (res, event) = self
            .reader
            .read_resolved_event()
            .map_err(|e| Error::BadXml {
                part: self.part.clone(),
                detail: e.to_string(),
            })?;
        let (main, strict) = match res {
            ResolveResult::Bound(ns) => (ns.0 == MAIN, ns.0 == MAIN_STRICT),
            _ => (false, false),
        };
        if matches!(event, Event::DocType(_)) {
            return Err(self.bad("a DTD"));
        }
        let end = self.pos();
        Ok(Ev {
            event,
            at,
            end,
            main,
            strict,
        })
    }

    /// The namespace an attribute's prefix stands for, or "" for none.
    pub fn attr_ns(&self, key: QName<'_>) -> bool {
        let (res, _) = self.reader.resolver().resolve_attribute(key);
        matches!(res, ResolveResult::Bound(ns) if ns.0 == REL)
    }

    /// Reads an element's text up to its end tag: a span when nothing in it
    /// needed unescaping, owned text otherwise. Child elements are an error.
    pub fn text_until_end(&mut self, start_end: usize) -> Result<Str> {
        // Stays `None` while everything so far was plain text, which is then
        // exactly the source between the tags; filled lazily otherwise.
        let mut owned: Option<String> = None;
        loop {
            let ev = self.next()?;
            let before = &self.text[start_end..ev.at];
            match ev.event {
                Event::Text(t) => {
                    let raw = t.into_inner();
                    if raw.contains('\r') {
                        fill(&mut owned, before)
                            .push_str(&raw.replace("\r\n", "\n").replace('\r', "\n"));
                    } else if let Some(s) = owned.as_mut() {
                        s.push_str(&raw);
                    }
                }
                Event::GeneralRef(r) => {
                    let ch = resolve_ref(&r).ok_or_else(|| self.bad("an unknown entity"))?;
                    fill(&mut owned, before).push_str(&ch);
                }
                Event::CData(c) => fill(&mut owned, before).push_str(&c.into_inner()),
                Event::Comment(_) | Event::PI(_) => {
                    fill(&mut owned, before);
                }
                Event::End(_) => {
                    return Ok(match owned {
                        Some(s) => Str::Own(s.into_boxed_str()),
                        None => Str::At(span(start_end, ev.at)?),
                    });
                }
                Event::Eof => return Err(self.bad("ends inside an element")),
                Event::Start(_) | Event::Empty(_) => {
                    return Err(self.bad("an element inside text"));
                }
                Event::Decl(_) | Event::DocType(_) => return Err(self.bad("misplaced markup")),
            }
        }
    }

    /// Skips the rest of an element whose start tag was just read, hashing
    /// it canonically into `h`.
    pub fn hash_rest(&mut self, h: &mut Fnv) -> Result<()> {
        let mut depth = 1usize;
        while depth > 0 {
            let ev = self.next()?;
            match &ev.event {
                Event::Start(e) => {
                    depth += 1;
                    self.hash_start(h, e);
                }
                Event::Empty(e) => {
                    self.hash_start(h, e);
                    h.str("/");
                }
                Event::End(_) => {
                    depth -= 1;
                    h.str("</");
                }
                Event::Text(t) => {
                    let t = t.xml10_content();
                    if !t.trim().is_empty() {
                        h.str("t").str(&t);
                    }
                }
                Event::GeneralRef(r) => {
                    h.str("t").str(&resolve_ref(r).unwrap_or_default());
                }
                Event::CData(c) => {
                    h.str("t").str(&c.xml10_content());
                }
                Event::Eof => return Err(self.bad("ends inside an element")),
                _ => {}
            }
        }
        Ok(())
    }

    /// Hashes a start tag: its local name and its attributes in a fixed
    /// order, leaving out what Excel renews on every save (revision ids) and
    /// namespace declarations.
    pub fn hash_start(&self, h: &mut Fnv, e: &BytesStart<'_>) {
        h.str("<").str(e.local_name().as_ref());
        let mut attrs: Vec<(String, String)> = e
            .attributes()
            .flatten()
            .filter(|a| {
                let key = a.key.0;
                !(key.starts_with("xmlns") || ignorable_attr(key))
            })
            .map(|a| {
                let value = value_of(&a);
                (a.key.0.to_owned(), value)
            })
            .collect();
        attrs.sort();
        for (k, v) in attrs {
            h.str(&k).str("=").str(&v);
        }
    }
}

/// True for attributes that change on every save without anyone changing
/// anything: revision ids and Excel's row-height hints.
#[must_use]
pub fn ignorable_attr(key: &str) -> bool {
    key.split_once(':')
        .is_some_and(|(_, local)| matches!(local, "uid" | "dyDescent"))
}

/// The owned text so far, starting it with the plain text before.
fn fill<'s>(owned: &'s mut Option<String>, before: &str) -> &'s mut String {
    owned.get_or_insert_with(|| before.to_owned())
}

/// An attribute's value with entities resolved and whitespace normalised.
fn value_of(a: &quick_xml::events::attributes::Attribute<'_>) -> String {
    a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
        .map_or_else(|_| a.value.to_string(), Cow::into_owned)
}

/// A span from byte offsets.
///
/// # Errors
/// [`Error::TooLarge`] past 4 GB.
pub fn span(start: usize, end: usize) -> Result<Span> {
    let start32 =
        u32::try_from(start).map_err(|_| Error::TooLarge("a part over 4 GB".to_owned()))?;
    let len = u32::try_from(end.saturating_sub(start))
        .map_err(|_| Error::TooLarge("a part over 4 GB".to_owned()))?;
    Ok(Span {
        start: start32,
        len,
    })
}

/// The text an entity or character reference stands for. Only XML's five
/// predefined entities exist without a DTD.
#[must_use]
pub fn resolve_ref(r: &BytesRef<'_>) -> Option<String> {
    if r.is_char_ref() {
        return r.resolve_char_ref().ok().flatten().map(String::from);
    }
    Some(
        match &**r {
            "amp" => "&",
            "lt" => "<",
            "gt" => ">",
            "quot" => "\"",
            "apos" => "'",
            _ => return None,
        }
        .to_owned(),
    )
}

/// An attribute's unescaped value, by its unprefixed name.
pub fn attr(e: &BytesStart<'_>, name: &str) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.0 == name)
        .map(|a| value_of(&a))
}

/// An attribute in the relationships namespace (`r:id`), by local name.
pub fn rel_attr(pull: &Pull<'_>, e: &BytesStart<'_>, local: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        let key = a.key;
        let is_local = key.0.rsplit_once(':').is_some_and(|(_, l)| l == local);
        (is_local && pull.attr_ns(key)).then(|| a.value.to_string())
    })
}

/// "1" or "true".
#[must_use]
pub fn truthy(value: Option<&str>) -> bool {
    matches!(value, Some("1" | "true"))
}
