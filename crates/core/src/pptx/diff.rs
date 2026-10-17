// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What changed between two decks.
//!
//! Slides are matched by creation id, then by slide id (only when nothing says
//! they are different slides), then by layout, title and text similarity.
//! Shapes on matched slides are matched by shape id (confirmed by kind and
//! content or position), then placeholder, then name, then position and text.
//! A duplicated slide has a new id and creation id, so it is always an added
//! slide, never a match for its original.

use std::collections::BTreeSet;

use super::Deck;
use super::model::{Shape, ShapeKind, Slide};
use crate::diff::lcs::{lcs, longest_increasing};
use crate::diff::similarity::similarity;
use crate::diff::{ByHand, ChangeKind, Segment, SegmentKind};
use crate::hash::{Fnv, sha256_hex};

/// Below this, two slides (or shapes) with nothing else in common are not paired.
pub const PAIR_SIMILARITY: u8 = 50;

/// What a change is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Unit {
    /// A whole slide: added, removed or moved.
    Slide,
    /// One shape on a slide.
    Shape,
    /// A slide's speaker notes.
    Notes,
    /// A slide's settings: hidden, layout, background, transition.
    Settings,
    /// A slide's animations.
    Animations,
    /// The slide masters, layouts and theme.
    Design,
}

impl Unit {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Slide => "slide",
            Self::Shape => "shape",
            Self::Notes => "notes",
            Self::Settings => "settings",
            Self::Animations => "animations",
            Self::Design => "design",
        }
    }
}

/// A place in one deck: a slide (by index in show order) and maybe a shape on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Loc {
    pub slide: usize,
    pub shape: Option<usize>,
}

/// Where inserted (or moved) content goes, in the earlier deck's terms: after
/// a slide (`None`: first), or after a shape on a slide (`None`: first).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Gap {
    Slide(Option<usize>),
    Shape { slide: usize, after: Option<usize> },
}

/// What exactly changed in a slide's settings.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SettingsChange {
    /// `Some(true)`: hidden; `Some(false)`: shown again.
    pub hidden: Option<bool>,
    /// The new layout's name, when it changed.
    pub layout: Option<String>,
    /// Background, transition or colours changed.
    pub other: bool,
}

/// One change between two decks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    /// `c` and 16 hex digits, from the change's content: the same two decks
    /// always give the same ids.
    pub id: String,
    pub kind: ChangeKind,
    pub unit: Unit,
    /// Where it was, in the earlier deck.
    pub before: Option<Loc>,
    /// Where it is, in the later deck.
    pub after: Option<Loc>,
    /// For insertions and moves.
    pub gap: Option<Gap>,
    /// The slide's number (1-based) and title, in the later deck when it has
    /// the slide, else the earlier one: "Slide 6 · Traction".
    pub slide_number: usize,
    pub slide_title: Option<String>,
    pub shape_kind: Option<ShapeKind>,
    pub before_text: Option<String>,
    pub after_text: Option<String>,
    pub segments: Vec<Segment>,
    pub formatting_only: bool,
    /// Only the position or size changed.
    pub geometry_only: bool,
    pub settings: Option<SettingsChange>,
    /// Whole percentage: how sure the matching is.
    pub confidence: u8,
    pub by_hand: Option<ByHand>,
    /// The key of the content the change leads to (the later slide or shape),
    /// for telling whether another deck already has it.
    pub after_key: u64,
    pub after_content: u64,
}

impl Change {
    /// "Slide 6 · Traction", or "Slide 6".
    #[must_use]
    pub fn place(&self) -> String {
        match &self.slide_title {
            Some(title) => format!("Slide {} · {title}", self.slide_number),
            None => format!("Slide {}", self.slide_number),
        }
    }
}

/// A matched pair of slides (indices in the earlier and later deck).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlidePair {
    pub before: usize,
    pub after: usize,
    pub confidence: u8,
    /// Matched shapes (indices on each slide).
    pub shapes: Vec<(usize, usize)>,
}

/// Two decks matched, and what changed between them.
#[derive(Clone, Debug)]
pub struct DeckDiff {
    pub pairs: Vec<SlidePair>,
    pub changes: Vec<Change>,
}

impl DeckDiff {
    /// The later deck's slide matched to the earlier deck's `before`.
    #[must_use]
    pub fn after_of(&self, before: usize) -> Option<usize> {
        self.pairs
            .iter()
            .find(|p| p.before == before)
            .map(|p| p.after)
    }

    /// The earlier deck's slide matched to the later deck's `after`.
    #[must_use]
    pub fn before_of(&self, after: usize) -> Option<usize> {
        self.pairs
            .iter()
            .find(|p| p.after == after)
            .map(|p| p.before)
    }

    /// The matched shapes of the pair whose earlier slide is `before`.
    #[must_use]
    pub fn shapes_of(&self, before: usize) -> &[(usize, usize)] {
        self.pairs
            .iter()
            .find(|p| p.before == before)
            .map_or(&[], |p| p.shapes.as_slice())
    }
}

fn unique<T: Ord + Copy>(values: impl Iterator<Item = Option<T>>) -> BTreeSet<T> {
    let mut seen = BTreeSet::new();
    let mut twice = BTreeSet::new();
    for value in values.flatten() {
        if !seen.insert(value) {
            twice.insert(value);
        }
    }
    seen.difference(&twice).copied().collect()
}

/// Pairs the slides of two decks: `(before, after, confidence)`.
#[must_use]
pub fn match_slides(a: &[Slide], b: &[Slide]) -> Vec<(usize, usize, u8)> {
    let mut pairs = Vec::new();
    let mut used_a = vec![false; a.len()];
    let mut used_b = vec![false; b.len()];

    // 1. Creation ids, when each is unique on its side.
    let ua = unique(a.iter().map(|s| s.creation_id));
    let ub = unique(b.iter().map(|s| s.creation_id));
    for (i, slide) in a.iter().enumerate() {
        let Some(cid) = slide
            .creation_id
            .filter(|c| ua.contains(c) && ub.contains(c))
        else {
            continue;
        };
        if let Some(j) = b.iter().position(|s| s.creation_id == Some(cid)) {
            pairs.push((i, j, 100));
            used_a[i] = true;
            used_b[j] = true;
        }
    }

    // 2. Slide ids, unless both creation ids are known and differ (PowerPoint
    //    can give a new slide a deleted slide's id), and the slides look related.
    for (i, slide) in a.iter().enumerate() {
        if used_a[i] {
            continue;
        }
        let Some(j) = b
            .iter()
            .position(|s| s.sld_id == slide.sld_id)
            .filter(|&j| !used_b[j])
        else {
            continue;
        };
        let other = &b[j];
        if let (Some(x), Some(y)) = (slide.creation_id, other.creation_id)
            && x != y
        {
            continue;
        }
        let sim = similarity(&slide.text(), &other.text());
        let related = sim >= 30
            || (slide.title().is_some() && slide.title() == other.title())
            || (slide.text().is_empty() && other.text().is_empty());
        if related {
            pairs.push((i, j, 95));
            used_a[i] = true;
            used_b[j] = true;
        }
    }

    // 3. The rest by content: title, layout and text.
    let mut candidates = Vec::new();
    for (i, x) in a.iter().enumerate().filter(|(i, _)| !used_a[*i]) {
        for (j, y) in b.iter().enumerate().filter(|(j, _)| !used_b[*j]) {
            let sim = similarity(&x.text(), &y.text());
            let same_title = x.title().is_some() && x.title() == y.title();
            let same_layout = x.layout.as_ref().map(|l| (&l.name, &l.kind))
                == y.layout.as_ref().map(|l| (&l.name, &l.kind));
            let score =
                u16::from(sim) + if same_title { 20 } else { 0 } + if same_layout { 10 } else { 0 };
            if sim >= PAIR_SIMILARITY || (same_title && sim >= 20) {
                candidates.push((score, i, j, sim));
            }
        }
    }
    // Highest score first; ties by order, so the result is deterministic.
    candidates.sort_by(|p, q| q.0.cmp(&p.0).then(p.1.cmp(&q.1)).then(p.2.cmp(&q.2)));
    for (_, i, j, sim) in candidates {
        if !used_a[i] && !used_b[j] {
            pairs.push((i, j, sim.max(60)));
            used_a[i] = true;
            used_b[j] = true;
        }
    }
    pairs.sort_unstable();
    pairs
}

fn overlap_share(x: &Shape, y: &Shape) -> u8 {
    match (x.geometry, y.geometry) {
        (Some(g), Some(h)) => {
            let smaller = g.area().min(h.area());
            if smaller <= 0 {
                return 0;
            }
            u8::try_from((g.overlap(&h) * 100 / smaller).clamp(0, 100)).unwrap_or(0)
        }
        _ => 0,
    }
}

/// Pairs the shapes of two matched slides: `(before, after)`.
#[must_use]
pub fn match_shapes(a: &[Shape], b: &[Shape]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    let mut used_a = vec![false; a.len()];
    let mut used_b = vec![false; b.len()];
    let ua = unique(a.iter().map(|s| Some(s.id).filter(|id| *id != 0)));
    let ub = unique(b.iter().map(|s| Some(s.id).filter(|id| *id != 0)));

    // 1. Shape ids, confirmed: same kind, and the same placeholder, related
    //    text, or most of the same place on the slide.
    for (i, x) in a.iter().enumerate() {
        if !ua.contains(&x.id) || !ub.contains(&x.id) {
            continue;
        }
        let Some(j) = b.iter().position(|y| y.id == x.id) else {
            continue;
        };
        let y = &b[j];
        let confirmed = x.kind == y.kind
            && ((x.placeholder.is_some() && x.placeholder == y.placeholder)
                || x.content == y.content
                || similarity(&x.text(), &y.text()) >= 30
                || overlap_share(x, y) >= 50
                || (x.text().is_empty() && y.text().is_empty()));
        if confirmed {
            pairs.push((i, j));
            used_a[i] = true;
            used_b[j] = true;
        }
    }
    // 2. Placeholders, by type and index.
    for (i, x) in a.iter().enumerate() {
        let Some(ph) = x.placeholder.as_ref().filter(|_| !used_a[i]) else {
            continue;
        };
        if let Some(j) = (0..b.len()).find(|&j| !used_b[j] && b[j].placeholder.as_ref() == Some(ph))
        {
            pairs.push((i, j));
            used_a[i] = true;
            used_b[j] = true;
        }
    }
    // 3. Kind, then content, name, or position and text.
    let mut candidates = Vec::new();
    for (i, x) in a.iter().enumerate().filter(|(i, _)| !used_a[*i]) {
        for (j, y) in b.iter().enumerate().filter(|(j, _)| !used_b[*j]) {
            if x.kind != y.kind {
                continue;
            }
            let score = if x.content == y.content {
                300
            } else {
                let sim = u16::from(similarity(&x.text(), &y.text()));
                let place = u16::from(overlap_share(x, y));
                let named = if !x.name.is_empty() && x.name == y.name {
                    50
                } else {
                    0
                };
                if sim < u16::from(PAIR_SIMILARITY) && place < 50 && named == 0 {
                    continue;
                }
                sim + place + named
            };
            candidates.push((score, i, j));
        }
    }
    candidates.sort_by(|p, q| q.0.cmp(&p.0).then(p.1.cmp(&q.1)).then(p.2.cmp(&q.2)));
    for (_, i, j) in candidates {
        if !used_a[i] && !used_b[j] {
            pairs.push((i, j));
            used_a[i] = true;
            used_b[j] = true;
        }
    }
    pairs.sort_unstable();
    pairs
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Class {
    Word,
    Space,
    Other,
}

fn class(c: char) -> Class {
    if c.is_alphanumeric() {
        Class::Word
    } else if c.is_whitespace() && c != '\n' {
        Class::Space
    } else {
        Class::Other
    }
}

fn tokens(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut k = 0;
    while k < chars.len() {
        let c = class(chars[k]);
        let mut end = k + 1;
        if c != Class::Other {
            loop {
                if end < chars.len() && class(chars[end]) == c {
                    end += 1;
                } else if c == Class::Word
                    && end + 1 < chars.len()
                    && matches!(chars[end], '.' | ',' | '\'' | '\u{2019}')
                    && chars[end + 1].is_alphanumeric()
                {
                    // Figures and contractions stay one word: "1.2M", "4,800", "don't".
                    end += 2;
                } else {
                    break;
                }
            }
        }
        out.push(chars[k..end].iter().collect());
        k = end;
    }
    out
}

fn push(out: &mut Vec<Segment>, kind: SegmentKind, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = out.last_mut()
        && last.kind == kind
    {
        last.text.push_str(text);
        return;
    }
    out.push(Segment {
        kind,
        text: text.to_owned(),
    });
}

/// Removed and added text, word by word.
#[must_use]
pub fn word_segments(before: &str, after: &str) -> Vec<Segment> {
    let a = tokens(before);
    let b = tokens(after);
    let ka: Vec<u64> = a.iter().map(|t| Fnv::new().str(t).finish()).collect();
    let kb: Vec<u64> = b.iter().map(|t| Fnv::new().str(t).finish()).collect();
    let common = lcs(&ka, &kb);
    let mut out = Vec::new();
    let (mut i, mut j) = (0, 0);
    for (x, y) in common
        .into_iter()
        .chain(std::iter::once((a.len(), b.len())))
    {
        let removed: String = a[i..x].concat();
        let added: String = b[j..y].concat();
        push(&mut out, SegmentKind::Removed, &removed);
        push(&mut out, SegmentKind::Added, &added);
        if x < a.len() {
            push(&mut out, SegmentKind::Same, &a[x]);
        }
        i = x + 1;
        j = y + 1;
    }
    out
}

fn whole(kind: SegmentKind, text: &str) -> Vec<Segment> {
    if text.is_empty() {
        Vec::new()
    } else {
        vec![Segment {
            kind,
            text: text.to_owned(),
        }]
    }
}

fn some_text(text: String) -> Option<String> {
    Some(text)
}

/// Kinds the engine can't write, as a by-hand reason.
fn object_reason(kind: ShapeKind) -> Option<ByHand> {
    if kind.is_writable() {
        None
    } else {
        Some(ByHand::Object)
    }
}

fn slide_reason(slide: &Slide) -> Option<ByHand> {
    slide.shapes.iter().find_map(|s| object_reason(s.kind))
}

struct Ctx<'d> {
    a: &'d Deck,
    b: &'d Deck,
    changes: Vec<Change>,
}

impl Ctx<'_> {
    fn push(
        &mut self,
        kind: ChangeKind,
        unit: Unit,
        before: Option<Loc>,
        after: Option<Loc>,
    ) -> &mut Change {
        let (deck, loc) = match (after, before) {
            (Some(loc), _) => (self.b, loc),
            (None, Some(loc)) => (self.a, loc),
            (None, None) => (
                self.a,
                Loc {
                    slide: 0,
                    shape: None,
                },
            ),
        };
        let slide = deck.slides.get(loc.slide);
        self.changes.push(Change {
            id: String::new(),
            kind,
            unit,
            before,
            after,
            gap: None,
            slide_number: if unit == Unit::Design {
                0
            } else {
                loc.slide + 1
            },
            slide_title: slide.and_then(Slide::title),
            shape_kind: None,
            before_text: None,
            after_text: None,
            segments: Vec::new(),
            formatting_only: false,
            geometry_only: false,
            settings: None,
            confidence: 100,
            by_hand: None,
            after_key: 0,
            after_content: 0,
        });
        self.changes.last_mut().expect("just pushed")
    }
}

/// The later deck's predecessor of `index` that is matched, in the earlier deck.
fn slide_gap(pairs: &[(usize, usize, u8)], index: usize, skip: &BTreeSet<usize>) -> Option<usize> {
    (0..index)
        .rev()
        .filter(|j| !skip.contains(j))
        .find_map(|j| pairs.iter().find(|p| p.1 == j).map(|p| p.0))
}

/// What changed from `a` to `b`.
#[must_use]
pub fn diff(a: &Deck, b: &Deck) -> DeckDiff {
    let matched = match_slides(&a.slides, &b.slides);
    let mut ctx = Ctx {
        a,
        b,
        changes: Vec::new(),
    };
    let mut pairs = Vec::new();

    // Moves: matched slides off the longest common order.
    let by_before: Vec<(usize, usize)> = matched.iter().map(|p| (p.0, p.1)).collect();
    let stayed: BTreeSet<(usize, usize)> = longest_increasing(&by_before).into_iter().collect();
    let moved_after: BTreeSet<usize> = matched
        .iter()
        .filter(|p| !stayed.contains(&(p.0, p.1)))
        .map(|p| p.1)
        .collect();

    // Slides in the later deck's order: added, moved, changed.
    for (j, slide) in b.slides.iter().enumerate() {
        let Some(&(i, _, confidence)) = matched.iter().find(|p| p.1 == j) else {
            let change = ctx.push(
                ChangeKind::Insert,
                Unit::Slide,
                None,
                Some(Loc {
                    slide: j,
                    shape: None,
                }),
            );
            change.gap = Some(Gap::Slide(slide_gap(&matched, j, &moved_after)));
            change.after_text = some_text(slide.text());
            change.segments = whole(SegmentKind::Added, &slide.text());
            change.by_hand = slide_reason(slide);
            change.after_key = slide.full;
            change.after_content = slide_content(slide);
            continue;
        };
        let before_slide = &a.slides[i];
        if moved_after.contains(&j) {
            let change = ctx.push(
                ChangeKind::Move,
                Unit::Slide,
                Some(Loc {
                    slide: i,
                    shape: None,
                }),
                Some(Loc {
                    slide: j,
                    shape: None,
                }),
            );
            change.gap = Some(Gap::Slide(slide_gap(&matched, j, &moved_after)));
            change.before_text = some_text(before_slide.text());
            change.after_text = some_text(slide.text());
            change.confidence = confidence;
            change.after_key = slide.full;
        }
        let shapes = slide_changes(&mut ctx, i, j, confidence);
        pairs.push(SlidePair {
            before: i,
            after: j,
            confidence,
            shapes,
        });
    }
    // Removed slides, in the earlier deck's order.
    for (i, slide) in a.slides.iter().enumerate() {
        if matched.iter().any(|p| p.0 == i) {
            continue;
        }
        let change = ctx.push(
            ChangeKind::Delete,
            Unit::Slide,
            Some(Loc {
                slide: i,
                shape: None,
            }),
            None,
        );
        change.before_text = some_text(slide.text());
        change.segments = whole(SegmentKind::Removed, &slide.text());
    }
    if a.design != b.design {
        let change = ctx.push(ChangeKind::Modify, Unit::Design, None, None);
        change.formatting_only = true;
        change.by_hand = Some(ByHand::Design);
        change.before_text = Some(String::new());
        change.after_text = Some(String::new());
        change.after_key = b.design;
    }
    let mut changes = ctx.changes;
    assign_ids(&mut changes, a, b);
    DeckDiff { pairs, changes }
}

/// A slide's content, ignoring formatting: its shapes' content keys and notes text.
#[must_use]
pub fn slide_content(slide: &Slide) -> u64 {
    let mut h = Fnv::new();
    for shape in &slide.shapes {
        h.u64(shape.content);
    }
    if let Some(notes) = &slide.notes {
        h.str(&notes.text());
    }
    h.finish()
}

fn slide_changes(ctx: &mut Ctx<'_>, i: usize, j: usize, confidence: u8) -> Vec<(usize, usize)> {
    let (sa, sb) = (&ctx.a.slides[i], &ctx.b.slides[j]);
    let shape_pairs = match_shapes(&sa.shapes, &sb.shapes);
    if sa.full == sb.full {
        return shape_pairs;
    }
    let (sa, sb) = (sa.clone(), sb.clone());

    if sa.settings != sb.settings {
        let settings = SettingsChange {
            hidden: (sa.hidden != sb.hidden).then_some(sb.hidden),
            layout: (sa.layout.as_ref().map(|l| &l.name) != sb.layout.as_ref().map(|l| &l.name))
                .then(|| {
                    sb.layout
                        .as_ref()
                        .map(|l| l.name.clone())
                        .unwrap_or_default()
                }),
            other: false,
        };
        let other = settings.hidden.is_none() && settings.layout.is_none();
        let change = ctx.push(
            ChangeKind::Modify,
            Unit::Settings,
            Some(Loc {
                slide: i,
                shape: None,
            }),
            Some(Loc {
                slide: j,
                shape: None,
            }),
        );
        change.formatting_only = settings.hidden.is_none();
        change.settings = Some(SettingsChange { other, ..settings });
        change.before_text = Some(String::new());
        change.after_text = Some(String::new());
        change.confidence = confidence;
        change.after_key = sb.settings;
    }

    for (x, y) in &shape_pairs {
        let (p, q) = (&sa.shapes[*x], &sb.shapes[*y]);
        if p.full == q.full {
            continue;
        }
        let (before, after) = (p.text(), q.text());
        let kind = if p.content == q.content {
            ChangeKind::Format
        } else {
            ChangeKind::Modify
        };
        let change = ctx.push(
            kind,
            Unit::Shape,
            Some(Loc {
                slide: i,
                shape: Some(*x),
            }),
            Some(Loc {
                slide: j,
                shape: Some(*y),
            }),
        );
        change.shape_kind = Some(q.kind);
        change.formatting_only = kind == ChangeKind::Format;
        change.geometry_only = kind == ChangeKind::Format && p.geometry != q.geometry;
        change.segments = if before == after {
            whole(SegmentKind::Same, &after)
        } else {
            word_segments(&before, &after)
        };
        change.before_text = Some(before);
        change.after_text = Some(after);
        change.confidence = confidence;
        change.by_hand = object_reason(q.kind).or_else(|| object_reason(p.kind));
        change.after_key = q.full;
        change.after_content = q.content;
        if sa.animated.contains(&p.id) && change.by_hand.is_none() && q.kind != p.kind {
            change.by_hand = Some(ByHand::Animation);
        }
    }
    for (y, shape) in sb.shapes.iter().enumerate() {
        if shape_pairs.iter().any(|p| p.1 == y) {
            continue;
        }
        let after_of = (0..y)
            .rev()
            .find_map(|k| shape_pairs.iter().find(|p| p.1 == k).map(|p| p.0));
        let change = ctx.push(
            ChangeKind::Insert,
            Unit::Shape,
            None,
            Some(Loc {
                slide: j,
                shape: Some(y),
            }),
        );
        change.gap = Some(Gap::Shape {
            slide: i,
            after: after_of,
        });
        change.shape_kind = Some(shape.kind);
        change.after_text = Some(shape.text());
        change.segments = whole(SegmentKind::Added, &shape.text());
        change.by_hand = object_reason(shape.kind);
        change.after_key = shape.full;
        change.after_content = shape.content;
    }
    for (x, shape) in sa.shapes.iter().enumerate() {
        if shape_pairs.iter().any(|p| p.0 == x) {
            continue;
        }
        let change = ctx.push(
            ChangeKind::Delete,
            Unit::Shape,
            Some(Loc {
                slide: i,
                shape: Some(x),
            }),
            None,
        );
        change.slide_number = j + 1;
        change.slide_title = sb.title().or_else(|| sa.title());
        change.shape_kind = Some(shape.kind);
        change.before_text = Some(shape.text());
        change.segments = whole(SegmentKind::Removed, &shape.text());
        change.by_hand = object_reason(shape.kind)
            .or_else(|| sa.animated.contains(&shape.id).then_some(ByHand::Animation));
    }

    let notes_a = sa.notes.as_ref().map(|n| (n.key, n.text()));
    let notes_b = sb.notes.as_ref().map(|n| (n.key, n.text()));
    if notes_a.as_ref().map(|n| n.0) != notes_b.as_ref().map(|n| n.0) {
        let before = notes_a.map(|n| n.1).unwrap_or_default();
        let after = notes_b.as_ref().map(|n| n.1.clone()).unwrap_or_default();
        let kind = if before == after {
            ChangeKind::Format
        } else {
            ChangeKind::Modify
        };
        let change = ctx.push(
            kind,
            Unit::Notes,
            Some(Loc {
                slide: i,
                shape: None,
            }),
            Some(Loc {
                slide: j,
                shape: None,
            }),
        );
        change.formatting_only = kind == ChangeKind::Format;
        change.segments = word_segments(&before, &after);
        change.before_text = Some(before);
        change.after_text = Some(after);
        change.confidence = confidence;
        change.after_key = notes_b.map_or(0, |n| n.0);
    }
    if sa.timing != sb.timing {
        let change = ctx.push(
            ChangeKind::Modify,
            Unit::Animations,
            Some(Loc {
                slide: i,
                shape: None,
            }),
            Some(Loc {
                slide: j,
                shape: None,
            }),
        );
        change.formatting_only = true;
        change.by_hand = Some(ByHand::Animation);
        change.before_text = Some(String::new());
        change.after_text = Some(String::new());
        change.after_key = sb.timing;
    }
    shape_pairs
}

/// Gives every change its id: `c` and 16 hex digits of SHA-256 over what the
/// change is and where, by identities that don't depend on the other deck's
/// numbering. Repeats get a counter.
fn assign_ids(changes: &mut [Change], a: &Deck, b: &Deck) {
    let mut seen: Vec<String> = Vec::new();
    for change in changes.iter_mut() {
        let slide_identity = |deck: &Deck, loc: Option<Loc>| {
            loc.and_then(|l| deck.slides.get(l.slide))
                .map_or_else(String::new, |s| {
                    format!("{}/{}", s.creation_id.unwrap_or(0), s.sld_id)
                })
        };
        let shape_identity = |deck: &Deck, loc: Option<Loc>| {
            loc.and_then(|l| deck.slides.get(l.slide)?.shapes.get(l.shape?))
                .map_or(0, |s| s.full)
        };
        let mut h = Fnv::new();
        h.str(change.kind.as_str())
            .str(change.unit.as_str())
            .str(&slide_identity(a, change.before))
            .str(&slide_identity(b, change.after))
            .u64(shape_identity(a, change.before))
            .u64(shape_identity(b, change.after))
            .str(change.before_text.as_deref().unwrap_or("\u{0}"))
            .str(change.after_text.as_deref().unwrap_or("\u{0}"))
            .u64(change.after_key);
        let base = format!("c{}", &sha256_hex(&h.finish().to_le_bytes())[..16]);
        let mut id = base.clone();
        let mut n = 1;
        while seen.contains(&id) {
            n += 1;
            id = format!("{}{n}", &base[..15]);
        }
        seen.push(id.clone());
        change.id = id;
    }
}

#[cfg(test)]
mod tests;
