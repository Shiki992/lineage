// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Cell formats. Each `cellXfs` entry becomes a key of what it looks like
//! (number format, font, fill, border, alignment, protection), resolved
//! through the tables it points at, so two workbooks with different style
//! tables still compare equal where the cells look the same.

use std::collections::BTreeMap;

use super::stream::MAIN;
use crate::error::Result;
use crate::hash::Fnv;
use crate::xml;

/// One cell format.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Xf {
    pub num_fmt: u32,
    /// What the format looks like, as a key.
    pub key: u64,
    /// `quotePrefix`: the cell's text was typed with a leading apostrophe.
    pub quote_prefix: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Styles {
    pub xfs: Vec<Xf>,
    /// Custom number formats by id.
    pub num_fmts: BTreeMap<u32, String>,
}

impl Styles {
    /// The number format code of a cell format: custom, or one of Excel's
    /// built-in ones.
    #[must_use]
    pub fn format_code(&self, style: u32) -> &str {
        let id = self.xfs.get(style as usize).map_or(0, |xf| xf.num_fmt);
        self.num_fmts
            .get(&id)
            .map_or_else(|| builtin_format(id), String::as_str)
    }

    /// The key of a cell format, or of the default when out of range.
    #[must_use]
    pub fn key(&self, style: u32) -> u64 {
        self.xfs
            .get(style as usize)
            .or_else(|| self.xfs.first())
            .map_or(0, |xf| xf.key)
    }
}

/// Excel's built-in number formats (ECMA-376 Part 1, 18.8.30). Ids 14 to 22
/// follow the computer's locale; these are their en-US forms.
#[must_use]
pub fn builtin_format(id: u32) -> &'static str {
    match id {
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "m/d/yyyy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yyyy h:mm",
        37 => "#,##0 ;(#,##0)",
        38 => "#,##0 ;[Red](#,##0)",
        39 => "#,##0.00;(#,##0.00)",
        40 => "#,##0.00;[Red](#,##0.00)",
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mmss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => "General",
    }
}

/// A canonical key of an element and everything in it: names, attributes in
/// order (revision ids left out), and text.
pub(super) fn node_key(h: &mut Fnv, node: roxmltree::Node<'_, '_>) {
    if node.is_element() {
        h.str("<")
            .str(node.tag_name().namespace().unwrap_or(""))
            .str(node.tag_name().name());
        let mut attrs: Vec<(String, &str)> = node
            .attributes()
            .filter(|a| {
                !(a.namespace().is_some() && matches!(a.name(), "uid" | "dyDescent" | "Ignorable"))
            })
            .map(|a| {
                (
                    format!("{}:{}", a.namespace().unwrap_or(""), a.name()),
                    a.value(),
                )
            })
            .collect();
        attrs.sort();
        for (k, v) in attrs {
            h.str(&k).str("=").str(v);
        }
        for child in node.children() {
            node_key(h, child);
        }
        h.str(">");
    } else if node.is_text() {
        let text = node.text().unwrap_or("");
        if !text.trim().is_empty() {
            h.str("t").str(text);
        }
    }
}

fn list_keys(root: roxmltree::Node<'_, '_>, list: &str, item: &str) -> Vec<u64> {
    xml::child(root, MAIN, list)
        .map(|l| {
            l.children()
                .filter(|n| xml::is(*n, MAIN, item))
                .map(|n| {
                    let mut h = Fnv::new();
                    node_key(&mut h, n);
                    h.finish()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Reads `xl/styles.xml`. A workbook without one uses Excel's defaults.
///
/// # Errors
/// [`crate::Error::BadXml`] when the part isn't readable.
pub(super) fn read(part: &str, bytes: Option<&[u8]>) -> Result<Styles> {
    let Some(bytes) = bytes else {
        return Ok(Styles::default());
    };
    let text = xml::text(part, bytes)?;
    let doc = xml::parse(part, text)?;
    let root = doc.root_element();
    let mut styles = Styles::default();
    if let Some(list) = xml::child(root, MAIN, "numFmts") {
        for n in list.children().filter(|n| xml::is(*n, MAIN, "numFmt")) {
            if let (Some(id), Some(code)) = (
                n.attribute("numFmtId").and_then(|v| v.parse().ok()),
                n.attribute("formatCode"),
            ) {
                styles.num_fmts.insert(id, code.to_owned());
            }
        }
    }
    let fonts = list_keys(root, "fonts", "font");
    let fills = list_keys(root, "fills", "fill");
    let borders = list_keys(root, "borders", "border");
    let style_xfs = list_keys(root, "cellStyleXfs", "xf");
    if let Some(list) = xml::child(root, MAIN, "cellXfs") {
        for n in list.children().filter(|n| xml::is(*n, MAIN, "xf")) {
            let index = |name: &str| -> usize {
                n.attribute(name)
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0)
            };
            let num_fmt = n
                .attribute("numFmtId")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let code = styles
                .num_fmts
                .get(&num_fmt)
                .map_or_else(|| builtin_format(num_fmt), String::as_str);
            let mut h = Fnv::new();
            h.str("fmt").str(code);
            h.str("font")
                .u64(fonts.get(index("fontId")).copied().unwrap_or(0));
            h.str("fill")
                .u64(fills.get(index("fillId")).copied().unwrap_or(0));
            h.str("border")
                .u64(borders.get(index("borderId")).copied().unwrap_or(0));
            h.str("style")
                .u64(style_xfs.get(index("xfId")).copied().unwrap_or(0));
            for child in n.children().filter(roxmltree::Node::is_element) {
                node_key(&mut h, child);
            }
            let quote_prefix = matches!(n.attribute("quotePrefix"), Some("1" | "true"));
            h.str("quote").u64(u64::from(quote_prefix));
            styles.xfs.push(Xf {
                num_fmt,
                key: h.finish(),
                quote_prefix,
            });
        }
    }
    Ok(styles)
}
