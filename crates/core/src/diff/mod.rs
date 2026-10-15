// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What changed between two states of a document.
//!
//! [`diff`] aligns every story (blocks, then table rows and cells) and turns the
//! alignment into [`Change`]s: one per paragraph, table row, paragraph in a table
//! cell, section break or opaque block that differs. A change carries where it is
//! in both documents, the text before and after, which words were removed and
//! added, and what formatting changed. [`spec::change_list`] writes changes in
//! the open change format (`spec/format/v0`).

pub mod align;
pub mod inline;
pub mod keys;
pub mod lcs;
pub mod similarity;
pub mod spec;
mod tables;

use std::collections::BTreeMap;

pub use align::{Alignment, Fate};
pub use tables::{RowFate, TableDiff};

use crate::docx::{Block, Docx, Inline, Lock, Paragraph, RunProps, StoryKey};
use crate::hash::{Fnv, sha256_hex};
use inline::{InlineDiff, Step, diff_paragraphs};

/// The kind of a change, as in `spec/format/v0`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ChangeKind {
    Insert,
    Delete,
    Modify,
    Move,
    Format,
}

impl ChangeKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Insert => "insert",
            Self::Delete => "delete",
            Self::Modify => "modify",
            Self::Move => "move",
            Self::Format => "format",
        }
    }
}

/// One step into a table: a row, and optionally a cell and a block in it.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellStep {
    pub row: usize,
    /// `None` for a change to a whole row.
    pub column: Option<usize>,
    /// The block inside the cell. `None` with a column means the start of the cell.
    pub block: Option<usize>,
}

/// Where a block is: its index in the story, then steps into table cells.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Path {
    pub block: usize,
    pub cells: Vec<CellStep>,
}

impl Path {
    #[must_use]
    pub fn top(block: usize) -> Self {
        Self {
            block,
            cells: Vec::new(),
        }
    }

    /// True for a path to a top-level block of a story.
    #[must_use]
    pub fn is_top(&self) -> bool {
        self.cells.is_empty()
    }
}

/// What kind of thing a change is about. This is also the unit the merge treats
/// as one: two changes to one unit overlap.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Unit {
    Paragraph,
    Table,
    Row,
    SectionBreak,
    Opaque,
}

/// Formatting that changed, for plain-English summaries.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FormatNote {
    /// A run property switched on (`true`) or off: `b`, `i`, `u` or `strike`.
    Toggle {
        property: &'static str,
        on: bool,
    },
    ParagraphStyle {
        from: Option<String>,
        to: Option<String>,
    },
    CharacterStyle,
    List {
        added: bool,
    },
    ListLevel,
    Colour,
    Highlight,
    Size,
    Font,
    Layout,
    Table,
    Section,
    Other,
}

/// How a piece of text is shown in "what changed".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SegmentKind {
    Same,
    Removed,
    Added,
    /// The same text with different formatting.
    Reformatted,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Segment {
    pub kind: SegmentKind,
    pub text: String,
}

/// Why a change must be brought in by hand in this version.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ByHand {
    /// Footnote and endnote content.
    Note,
    /// Table columns, merged cells, table or row formatting.
    TableStructure,
    /// Section breaks and page layout.
    Section,
    /// Inside a table of contents, content control or similar.
    Locked,
    /// Something the engine does not model.
    Opaque,
    /// A header or footer that exists on one side only.
    Story,
    /// This version has unaccepted tracked changes there: accept or reject them
    /// in Word first.
    Revisions,
    /// It needs a picture or other part this version does not have.
    NewPart,
    /// The two files write their XML in ways that cannot be combined safely.
    Markup,
    /// The engine cannot tell where it goes.
    Placement,
}

/// One change between two states of a document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// Unique within one change list: `c` and 16 hex digits, derived from the
    /// change's content, so the same two documents always give the same ids.
    pub id: String,
    pub kind: ChangeKind,
    pub unit: Unit,
    pub story: StoryKey,
    /// The part the change is shown in (the earlier document's for a deletion).
    pub part: String,
    /// Where it was, in the earlier document.
    pub before: Option<Path>,
    /// Where it is, in the later document.
    pub after: Option<Path>,
    /// For insertions and moves: the earlier document's block (or row) the new
    /// content follows. `None` means the start of the story; a path ending at a
    /// cell with no block means the start of that cell, and a path ending at a
    /// table means its first row.
    pub gap: Option<Path>,
    pub before_text: Option<String>,
    pub after_text: Option<String>,
    pub formatting_only: bool,
    /// Whole percentage.
    pub confidence: u8,
    /// Changed characters in `after_text` (Unicode scalar values).
    pub char_range: Option<(usize, usize)>,
    /// `w14:paraId` of the paragraph, if Word wrote one.
    pub anchor: Option<String>,
    /// The heading the change sits under, for "Under 'Experience'".
    pub heading: Option<String>,
    pub segments: Vec<Segment>,
    pub format_notes: Vec<FormatNote>,
    pub by_hand: Option<ByHand>,
}

/// The aligned stories of two documents, and the changes between them.
#[derive(Clone, Debug)]
pub struct DocDiff {
    pub stories: Vec<StoryDiff>,
    pub changes: Vec<Change>,
}

impl DocDiff {
    #[must_use]
    pub fn story(&self, key: &StoryKey) -> Option<&StoryDiff> {
        self.stories.iter().find(|s| &s.key == key)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }

    #[must_use]
    pub fn change(&self, id: &str) -> Option<&Change> {
        self.changes.iter().find(|c| c.id == id)
    }
}

/// The alignment of one story, and of the tables and cells inside it.
#[derive(Clone, Debug)]
pub struct StoryDiff {
    pub key: StoryKey,
    pub before_part: Option<String>,
    pub after_part: Option<String>,
    pub blocks: Option<SeqDiff>,
}

/// The alignment of one block sequence (a story or a table cell), with the
/// table diffs of its tables, keyed by the earlier block index.
#[derive(Clone, Debug)]
pub struct SeqDiff {
    pub alignment: Alignment,
    pub tables: BTreeMap<usize, TableDiff>,
}

/// One side of a comparison: a block sequence and where it sits.
#[derive(Clone, Copy)]
pub(crate) struct Side<'d> {
    pub blocks: &'d [Block],
    pub src: Option<&'d str>,
    pub part: &'d str,
    /// For a cell: the top-level index of the outermost table.
    pub top: Option<usize>,
    /// For a cell: the steps from that table to this cell (last step's block unset).
    pub scope: &'d [CellStep],
}

impl Side<'_> {
    pub(crate) fn path(&self, index: usize) -> Path {
        match self.top {
            None => Path::top(index),
            Some(top) => {
                let mut cells = self.scope.to_vec();
                if let Some(last) = cells.last_mut() {
                    last.block = Some(index);
                }
                Path { block: top, cells }
            }
        }
    }

    /// The gap at the very start of this sequence.
    pub(crate) fn start(&self) -> Option<Path> {
        self.top.map(|top| Path {
            block: top,
            cells: self.scope.to_vec(),
        })
    }

    pub(crate) fn gap(&self, after: Option<usize>) -> Option<Path> {
        after.map_or_else(|| self.start(), |g| Some(self.path(g)))
    }
}

/// What changed from `before` to `after`.
#[must_use]
pub fn diff(before: &Docx, after: &Docx) -> DocDiff {
    let mut stories = Vec::new();
    let mut changes = Vec::new();
    let mut keys: Vec<&StoryKey> = before.stories.iter().map(|s| &s.key).collect();
    for story in &after.stories {
        if !keys.contains(&&story.key) {
            keys.push(&story.key);
        }
    }
    for key in keys {
        let (sa, sb) = (before.story(key), after.story(key));
        match (sa, sb) {
            (Some(sa), Some(sb)) => {
                let a = Side {
                    blocks: &sa.blocks,
                    src: before.part_text(&sa.part).ok(),
                    part: &sa.part,
                    top: None,
                    scope: &[],
                };
                let b = Side {
                    blocks: &sb.blocks,
                    src: after.part_text(&sb.part).ok(),
                    part: &sb.part,
                    top: None,
                    scope: &[],
                };
                let seq = sequence(key, a, b, &mut changes);
                stories.push(StoryDiff {
                    key: key.clone(),
                    before_part: Some(sa.part.clone()),
                    after_part: Some(sb.part.clone()),
                    blocks: Some(seq),
                });
            }
            (Some(only), None) | (None, Some(only)) => {
                let inserted = sa.is_none();
                for (index, block) in only.blocks.iter().enumerate() {
                    if matches!(block, Block::SectionBreak(_)) {
                        continue;
                    }
                    let text = block.text();
                    let mut change = blank(key, &only.part, unit_of(block));
                    change.kind = if inserted {
                        ChangeKind::Insert
                    } else {
                        ChangeKind::Delete
                    };
                    if inserted {
                        change.after = Some(Path::top(index));
                        change.after_text = Some(text.clone());
                    } else {
                        change.before = Some(Path::top(index));
                        change.before_text = Some(text.clone());
                    }
                    change.segments = vec![Segment {
                        kind: if inserted {
                            SegmentKind::Added
                        } else {
                            SegmentKind::Removed
                        },
                        text,
                    }];
                    change.by_hand = Some(ByHand::Story);
                    changes.push(change);
                }
                stories.push(StoryDiff {
                    key: key.clone(),
                    before_part: sa.map(|s| s.part.clone()),
                    after_part: sb.map(|s| s.part.clone()),
                    blocks: None,
                });
            }
            (None, None) => {}
        }
    }
    assign_ids(&mut changes);
    DocDiff { stories, changes }
}

fn blank(story: &StoryKey, part: &str, unit: Unit) -> Change {
    Change {
        id: String::new(),
        kind: ChangeKind::Modify,
        unit,
        story: story.clone(),
        part: part.to_owned(),
        before: None,
        after: None,
        gap: None,
        before_text: None,
        after_text: None,
        formatting_only: false,
        confidence: 100,
        char_range: None,
        anchor: None,
        heading: None,
        segments: Vec::new(),
        format_notes: Vec::new(),
        by_hand: None,
    }
}

pub(crate) fn unit_of(block: &Block) -> Unit {
    match block {
        Block::Paragraph(_) => Unit::Paragraph,
        Block::Table(_) => Unit::Table,
        Block::SectionBreak(_) => Unit::SectionBreak,
        Block::Opaque(_) => Unit::Opaque,
    }
}

/// True for a paragraph in a heading style (or with an outline level).
#[must_use]
pub fn is_heading(p: &Paragraph) -> bool {
    p.props
        .style
        .as_deref()
        .is_some_and(|s| s.to_ascii_lowercase().starts_with("heading"))
        || p.props.direct.get("outlineLvl").is_some()
}

/// The text of the nearest heading before `index`.
pub(crate) fn heading_before(blocks: &[Block], index: usize) -> Option<String> {
    blocks[..index.min(blocks.len())]
        .iter()
        .rev()
        .find_map(|b| match b {
            Block::Paragraph(p) if is_heading(p) && !p.text().trim().is_empty() => Some(p.text()),
            _ => None,
        })
}

fn lock_of(block: &Block) -> Option<Lock> {
    match block {
        Block::Paragraph(p) => p.lock,
        Block::Table(t) => t.lock,
        _ => None,
    }
}

fn anchor_of(block: &Block) -> Option<String> {
    match block {
        Block::Paragraph(p) => p.anchor.clone(),
        _ => None,
    }
}

fn by_hand_for(story: &StoryKey, block: &Block) -> Option<ByHand> {
    if matches!(story, StoryKey::Footnotes | StoryKey::Endnotes) {
        return Some(ByHand::Note);
    }
    if lock_of(block).is_some() {
        return Some(ByHand::Locked);
    }
    match block {
        Block::SectionBreak(_) => Some(ByHand::Section),
        Block::Opaque(_) => Some(ByHand::Opaque),
        _ => None,
    }
}

/// Aligns one block sequence and records its changes, sorted in reading order
/// of the later document (deletions just before the next surviving block).
pub(crate) fn sequence(
    story: &StoryKey,
    a: Side<'_>,
    b: Side<'_>,
    out: &mut Vec<Change>,
) -> SeqDiff {
    let alignment = align::align(a.blocks, b.blocks, a.src, b.src);
    let mut tables = BTreeMap::new();
    let mut local: Vec<(usize, u8, usize, Change)> = Vec::new();
    let next_surviving = |i: usize| -> usize {
        (i + 1..alignment.fates.len())
            .find_map(|k| alignment.fates[k].target())
            .unwrap_or(b.blocks.len())
    };
    let mut seq = 0;
    let mut push =
        |local: &mut Vec<(usize, u8, usize, Change)>, pos: usize, order: u8, change: Change| {
            local.push((pos, order, seq, change));
            seq += 1;
        };
    for (i, fate) in alignment.fates.iter().enumerate() {
        let block_a = &a.blocks[i];
        match *fate {
            Fate::Same(_) => {}
            Fate::Deleted => {
                if matches!(block_a, Block::SectionBreak(s) if s.is_final) {
                    continue;
                }
                let text = block_a.text();
                let mut change = blank(story, a.part, unit_of(block_a));
                change.kind = ChangeKind::Delete;
                change.before = Some(a.path(i));
                change.before_text = Some(text.clone());
                change.anchor = anchor_of(block_a);
                change.heading = heading_before(a.blocks, i);
                change.segments = vec![Segment {
                    kind: SegmentKind::Removed,
                    text,
                }];
                change.by_hand = by_hand_for(story, block_a);
                push(&mut local, next_surviving(i), 0, change);
            }
            Fate::Formatted(j) | Fate::Modified { to: j, .. } => {
                let block_b = &b.blocks[j];
                if let (Block::Table(ta), Block::Table(tb)) = (block_a, block_b) {
                    let mut table_changes = Vec::new();
                    let table = tables::diff_table(story, &a, &b, ta, tb, i, j, &mut table_changes);
                    for change in table_changes {
                        push(&mut local, j, 1, change);
                    }
                    tables.insert(i, table);
                    continue;
                }
                let similarity = match *fate {
                    Fate::Modified { similarity, .. } => similarity,
                    _ => 100,
                };
                if let Some(change) = pair_change(story, &a, &b, i, j, similarity) {
                    push(&mut local, j, 1, change);
                }
            }
            Fate::Moved {
                to: j, similarity, ..
            } => {
                let mut change = pair_change(story, &a, &b, i, j, similarity)
                    .unwrap_or_else(|| paired(story, &a, &b, i, j));
                change.kind = ChangeKind::Move;
                change.formatting_only = false;
                change.gap = b_gap_in_a(&alignment, &a, j);
                change.confidence = similarity;
                push(&mut local, j, 1, change);
            }
        }
    }
    for j in alignment.inserted().collect::<Vec<_>>() {
        let block_b = &b.blocks[j];
        if matches!(block_b, Block::SectionBreak(s) if s.is_final) {
            continue;
        }
        let text = block_b.text();
        let mut change = blank(story, b.part, unit_of(block_b));
        change.kind = ChangeKind::Insert;
        change.after = Some(b.path(j));
        change.gap = b_gap_in_a(&alignment, &a, j);
        change.after_text = Some(text.clone());
        change.anchor = anchor_of(block_b);
        change.heading = heading_before(b.blocks, j);
        change.segments = vec![Segment {
            kind: SegmentKind::Added,
            text,
        }];
        change.by_hand = by_hand_for(story, block_b);
        push(&mut local, j, 1, change);
    }
    local.sort_by_key(|(pos, order, seq, _)| (*pos, *order, *seq));
    out.extend(local.into_iter().map(|(_, _, _, c)| c));
    SeqDiff { alignment, tables }
}

/// The gap, in the earlier document, that later block `j` follows.
fn b_gap_in_a(alignment: &Alignment, a: &Side<'_>, j: usize) -> Option<Path> {
    a.gap(alignment.gap_before(j))
}

/// A modify change between two paired blocks, with nothing compared yet.
fn paired(story: &StoryKey, a: &Side<'_>, b: &Side<'_>, i: usize, j: usize) -> Change {
    let (block_a, block_b) = (&a.blocks[i], &b.blocks[j]);
    let after_text = block_b.text();
    let mut change = blank(story, b.part, unit_of(block_b));
    change.before = Some(a.path(i));
    change.after = Some(b.path(j));
    change.before_text = Some(block_a.text());
    change.segments = vec![Segment {
        kind: SegmentKind::Same,
        text: after_text.clone(),
    }];
    change.after_text = Some(after_text);
    change.anchor = anchor_of(block_b);
    change.heading = heading_before(b.blocks, j).or_else(|| heading_before(a.blocks, i));
    change.by_hand = by_hand_for(story, block_b).or_else(|| by_hand_for(story, block_a));
    change
}

/// A modify or format change between two paired blocks, or `None` when they turn
/// out to be identical once compared in detail.
fn pair_change(
    story: &StoryKey,
    a: &Side<'_>,
    b: &Side<'_>,
    i: usize,
    j: usize,
    similarity: u8,
) -> Option<Change> {
    let mut change = paired(story, a, b, i, j);
    change.confidence = similarity;
    match (&a.blocks[i], &b.blocks[j]) {
        (Block::Paragraph(pa), Block::Paragraph(pb)) => {
            let d = diff_paragraphs(pa, pb, a.src, b.src);
            let mut notes = run_format_notes(&d, pa, pb);
            notes.extend(para_format_notes(pa, pb));
            let text_changed = d.text_changed();
            if !text_changed && notes.is_empty() {
                return None;
            }
            change.kind = if text_changed {
                ChangeKind::Modify
            } else {
                ChangeKind::Format
            };
            change.formatting_only = !text_changed;
            change.char_range = d.changed_range();
            if !text_changed && change.char_range.is_none() {
                change.char_range = Some((0, pb.text().chars().count()));
            }
            change.segments = segments(&d);
            change.format_notes = notes;
        }
        (Block::SectionBreak(sa), Block::SectionBreak(sb)) => {
            if sa.props == sb.props
                && sa
                    .headers
                    .iter()
                    .map(|h| (&h.kind, h.footer))
                    .eq(sb.headers.iter().map(|h| (&h.kind, h.footer)))
            {
                return None;
            }
            change.kind = ChangeKind::Format;
            change.formatting_only = true;
            change.format_notes = vec![FormatNote::Section];
            change.by_hand = Some(ByHand::Section);
        }
        _ => {
            change.by_hand = change.by_hand.or(Some(ByHand::Opaque));
        }
    }
    Some(change)
}

/// "What changed" segments from a paragraph diff.
pub(crate) fn segments(d: &InlineDiff) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut push = |kind: SegmentKind, text: &str| {
        if text.is_empty() {
            return;
        }
        match out.last_mut() {
            Some(last) if last.kind == kind => last.text.push_str(text),
            _ => out.push(Segment {
                kind,
                text: text.to_owned(),
            }),
        }
    };
    for step in &d.steps {
        match *step {
            Step::Same {
                b, format_changed, ..
            } => push(
                if format_changed {
                    SegmentKind::Reformatted
                } else {
                    SegmentKind::Same
                },
                &d.b[b].text,
            ),
            Step::Removed { a } => push(SegmentKind::Removed, &d.a[a].text),
            Step::Added { b } => push(SegmentKind::Added, &d.b[b].text),
        }
    }
    out
}

fn token_props(p: &Paragraph, inline: usize) -> Option<&RunProps> {
    match &p.inlines[inline] {
        Inline::Text { run, .. } => Some(&p.runs[*run].props),
        Inline::Atom(atom) => atom.run.map(|r| &p.runs[r].props),
        Inline::Marker(_) => None,
    }
}

const TOGGLE_NAMES: [&str; 4] = ["b", "i", "u", "strike"];

fn toggled_on(props: &RunProps, name: &str) -> bool {
    props
        .direct
        .get(name)
        .is_some_and(|v| !(v.contains("val=0;") || v.contains("val=none;")))
}

fn run_format_notes(d: &InlineDiff, pa: &Paragraph, pb: &Paragraph) -> Vec<FormatNote> {
    let mut notes: Vec<FormatNote> = Vec::new();
    let mut add = |note: FormatNote| {
        if !notes.contains(&note) {
            notes.push(note);
        }
    };
    let default = RunProps::default();
    for step in &d.steps {
        let Step::Same {
            a,
            b,
            format_changed: true,
        } = *step
        else {
            continue;
        };
        let ra = token_props(pa, d.a[a].inline).unwrap_or(&default);
        let rb = token_props(pb, d.b[b].inline).unwrap_or(&default);
        let mut explained = false;
        for name in TOGGLE_NAMES {
            if toggled_on(ra, name) != toggled_on(rb, name) {
                add(FormatNote::Toggle {
                    property: name,
                    on: toggled_on(rb, name),
                });
                explained = true;
            }
        }
        for (name, note) in [
            ("color", FormatNote::Colour),
            ("highlight", FormatNote::Highlight),
            ("sz", FormatNote::Size),
            ("rFonts", FormatNote::Font),
        ] {
            if ra.direct.get(name) != rb.direct.get(name) {
                add(note);
                explained = true;
            }
        }
        if ra.style != rb.style {
            add(FormatNote::CharacterStyle);
            explained = true;
        }
        if !explained {
            add(FormatNote::Other);
        }
    }
    notes
}

fn para_format_notes(pa: &Paragraph, pb: &Paragraph) -> Vec<FormatNote> {
    let mut notes = Vec::new();
    if pa.props.style != pb.props.style {
        notes.push(FormatNote::ParagraphStyle {
            from: pa.props.style.clone(),
            to: pb.props.style.clone(),
        });
    }
    match (&pa.props.numbering, &pb.props.numbering) {
        (None, Some(_)) => notes.push(FormatNote::List { added: true }),
        (Some(_), None) => notes.push(FormatNote::List { added: false }),
        (Some(x), Some(y)) if x != y => notes.push(FormatNote::ListLevel),
        _ => {}
    }
    if pa.props.direct != pb.props.direct {
        notes.push(FormatNote::Layout);
    }
    notes
}

/// Gives every change an id derived from what it is. Equal changes (the same
/// paragraph added twice) are told apart by their order.
fn assign_ids(changes: &mut [Change]) {
    let mut seen: BTreeMap<u64, usize> = BTreeMap::new();
    for change in changes {
        let base = Fnv::new()
            .str(change.kind.as_str())
            .str(&format!("{:?}", change.story))
            .str(&format!("{:?}", change.unit))
            .str(change.before_text.as_deref().unwrap_or("\u{1}"))
            .str(change.after_text.as_deref().unwrap_or("\u{1}"))
            .str(&format!("{:?}", change.format_notes))
            .finish();
        let ordinal = seen.entry(base).or_insert(0);
        *ordinal += 1;
        let digest = sha256_hex(format!("{base:016x}/{ordinal}").as_bytes());
        change.id = format!("c{}", &digest[..16]);
    }
}

#[cfg(test)]
mod tests;
