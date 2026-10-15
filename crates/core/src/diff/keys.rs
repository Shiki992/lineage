// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Content and formatting keys for blocks.
//!
//! Two blocks with the same content key say the same thing: same text, same
//! pictures and fields, regardless of how Word split the runs. Two blocks with
//! the same formatting key also look the same: same paragraph properties and the
//! same run properties on every character.

use crate::docx::{Block, Inline, ParaProps, Paragraph, RunProps, Table};
use crate::hash::Fnv;

/// Content and formatting keys of one block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Keys {
    pub content: u64,
    pub format: u64,
}

impl Keys {
    /// One key for both, for exact matching.
    #[must_use]
    pub fn full(self) -> (u64, u64) {
        (self.content, self.format)
    }
}

fn hash_para_props(h: &mut Fnv, props: &ParaProps) {
    h.str(props.style.as_deref().unwrap_or(""));
    match &props.numbering {
        Some(num) => h.str(&num.num_id).str(&num.level),
        None => h.str("-"),
    };
    for (k, v) in &props.direct.0 {
        h.str(k).str(v);
    }
}

/// Hash of run properties plus the container (hyperlink) a token sits in.
#[must_use]
pub fn run_props_key(props: &RunProps, container: Option<&str>) -> u64 {
    let mut h = Fnv::new();
    h.str(props.style.as_deref().unwrap_or(""));
    for (k, v) in &props.direct.0 {
        h.str(k).str(v);
    }
    h.str(container.unwrap_or(""));
    h.finish()
}

/// The identity of a container for comparison: its kind and what it points to.
#[must_use]
pub fn container_key(p: &Paragraph, index: Option<usize>, source: Option<&str>) -> Option<String> {
    let container = &p.containers[index?];
    let start_tag = source.map_or("", |s| s.get(container.start_tag.clone()).unwrap_or(""));
    // The start tag carries the target (r:id or w:anchor); rsids are rare here.
    Some(format!("{}|{}", container.qname, start_tag))
}

/// Keys of a paragraph. `source` is the part text, used to key containers by
/// their start tag; pass `None` to ignore containers.
#[must_use]
pub fn paragraph_keys(p: &Paragraph, source: Option<&str>) -> Keys {
    let mut content = Fnv::new();
    let mut format = Fnv::new();
    content.str("p");
    hash_para_props(&mut format, &p.props);
    for inline in &p.inlines {
        match inline {
            Inline::Text { text, run } => {
                content.bytes(text.as_bytes());
                let run = &p.runs[*run];
                let props = run_props_key(
                    &run.props,
                    container_key(p, run.container, source).as_deref(),
                );
                for _ in text.chars() {
                    format.u64(props);
                }
            }
            Inline::Atom(atom) => {
                content.bytes(&[0xFE]).u64(atom.key);
                let props = atom
                    .run
                    .map_or(0, |r| run_props_key(&p.runs[r].props, None));
                format.u64(props);
            }
            Inline::Marker(_) => {}
        }
    }
    Keys {
        content: content.finish(),
        format: format.finish(),
    }
}

fn table_keys(t: &Table, source: Option<&str>) -> Keys {
    let mut content = Fnv::new();
    let mut format = Fnv::new();
    content.str("tbl").u64(t.columns as u64);
    format.u64(t.props_key);
    for row in &t.rows {
        content.str("tr");
        format.u64(row.props_key);
        for cell in &row.cells {
            content.str("tc").u64(cell.grid_span as u64);
            format.u64(cell.props_key);
            content.u64(u64::from(
                cell.vmerge.map_or(0_u8, |restart| 1 + u8::from(restart)),
            ));
            for block in &cell.blocks {
                let keys = block_keys(block, source);
                content.u64(keys.content);
                format.u64(keys.format);
            }
        }
    }
    Keys {
        content: content.finish(),
        format: format.finish(),
    }
}

/// Keys of any block.
#[must_use]
pub fn block_keys(block: &Block, source: Option<&str>) -> Keys {
    match block {
        Block::Paragraph(p) => paragraph_keys(p, source),
        Block::Table(t) => table_keys(t, source),
        Block::SectionBreak(s) => {
            let mut format = Fnv::new();
            for (k, v) in &s.props.0 {
                format.str(k).str(v);
            }
            for header in &s.headers {
                format
                    .str(&header.kind)
                    .str(if header.footer { "f" } else { "h" });
            }
            Keys {
                content: Fnv::new()
                    .str("sect")
                    .str(if s.is_final { "final" } else { "break" })
                    .finish(),
                format: format.finish(),
            }
        }
        Block::Opaque(o) => Keys {
            content: Fnv::new().str("opaque").u64(o.key).finish(),
            format: 0,
        },
    }
}

/// The kind of a block, for pairing: only blocks of one kind are paired.
#[must_use]
pub fn block_kind(block: &Block) -> u8 {
    match block {
        Block::Paragraph(_) => 0,
        Block::Table(_) => 1,
        Block::SectionBreak(_) => 2,
        Block::Opaque(_) => 3,
    }
}
