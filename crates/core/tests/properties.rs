// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Property tests for the engine's invariants, on random documents:
//!
//! - merge(base, x, base) == x: nothing to bring in, and nothing written.
//! - merge(base, base, x) == x: bringing every change in reproduces x (accepted
//!   view), and rejecting every tracked change gives the base back.
//! - diff(x, x) is empty.
//! - Edits to different paragraphs on both sides merge with no overlaps, and the
//!   result has both.
//! - Splices, zip writes and the reader on damaged input.

use lineage_core::docx::{Block, Docx, NumRef, View};
use lineage_core::merge::{self, Decision, Status};
use lineage_core::write::{Revision, apply};
use lineage_core::xml::Edits;
use lineage_core::{Package, diff::diff};
use lineage_testkit::docx::{Package as Builder, list_item, para, plain, run, run_with, styled};
use proptest::prelude::*;

const WORDS: &[&str] = &[
    "data",
    "engineer",
    "pipelines",
    "retail",
    "logistics",
    "team",
    "built",
    "warehouse",
    "analysts",
    "Python",
    "SQL",
    "reports",
    "faster",
    "nightly",
    "streaming",
    "Halden",
    "Corvia",
    "cut",
    "costs",
    "led",
];

#[derive(Clone, Debug, PartialEq, Eq)]
enum Style {
    Normal,
    Heading1,
    Heading2,
    Bullet,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Para {
    style: Style,
    words: Vec<&'static str>,
    /// Make the first word bold.
    bold: bool,
}

impl Para {
    fn xml(&self) -> String {
        let text = self.words.join(" ");
        let runs = if self.bold && self.words.len() > 1 {
            let (first, rest) = text.split_once(' ').expect("two words");
            [run_with("<w:b/>", first), run(&format!(" {rest}"))].concat()
        } else if self.bold {
            run_with("<w:b/>", &text)
        } else {
            run(&text)
        };
        match self.style {
            Style::Normal => para("", &runs),
            Style::Heading1 => para(r#"<w:pStyle w:val="Heading1"/>"#, &runs),
            Style::Heading2 => para(r#"<w:pStyle w:val="Heading2"/>"#, &runs),
            Style::Bullet => {
                let _ = list_item;
                para(
                    r#"<w:pStyle w:val="ListParagraph"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr>"#,
                    &runs,
                )
            }
        }
    }
}

/// A document: a title, the paragraphs, and a fixed last paragraph (Word
/// cannot delete a document's last paragraph mark, a case tested on its own).
fn build(paras: &[Para]) -> Vec<u8> {
    let body: String = std::iter::once(styled("Title", "Avery Quinn"))
        .chain(paras.iter().map(Para::xml))
        .chain(std::iter::once(plain("End of document.")))
        .collect();
    let mut builder = Builder::new("Property test", body);
    builder.numbering = true;
    builder.to_bytes()
}

fn style() -> impl Strategy<Value = Style> {
    prop_oneof![
        Just(Style::Normal),
        Just(Style::Heading1),
        Just(Style::Heading2),
        Just(Style::Bullet)
    ]
}

fn paragraph() -> impl Strategy<Value = Para> {
    (
        style(),
        proptest::collection::vec(proptest::sample::select(WORDS), 1..7),
        any::<bool>(),
    )
        .prop_map(|(style, words, bold)| Para { style, words, bold })
}

fn document() -> impl Strategy<Value = Vec<Para>> {
    proptest::collection::vec(paragraph(), 1..10)
}

#[derive(Clone, Debug)]
enum Edit {
    Insert(usize, Para),
    Delete(usize),
    ReplaceWord(usize, usize, &'static str),
    AppendWord(usize, &'static str),
    ToggleBold(usize),
    Restyle(usize, Style),
    Move(usize, usize),
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        (any::<usize>(), paragraph()).prop_map(|(i, p)| Edit::Insert(i, p)),
        any::<usize>().prop_map(Edit::Delete),
        (
            any::<usize>(),
            any::<usize>(),
            proptest::sample::select(WORDS)
        )
            .prop_map(|(i, w, x)| Edit::ReplaceWord(i, w, x)),
        (any::<usize>(), proptest::sample::select(WORDS)).prop_map(|(i, x)| Edit::AppendWord(i, x)),
        any::<usize>().prop_map(Edit::ToggleBold),
        (any::<usize>(), style()).prop_map(|(i, s)| Edit::Restyle(i, s)),
        (any::<usize>(), any::<usize>()).prop_map(|(i, j)| Edit::Move(i, j)),
    ]
}

fn apply_edits(mut paras: Vec<Para>, edits: &[Edit]) -> Vec<Para> {
    for edit in edits {
        let n = paras.len();
        match edit {
            Edit::Insert(i, p) => paras.insert(i % (n + 1), p.clone()),
            Edit::Delete(i) if n > 1 => {
                paras.remove(i % n);
            }
            Edit::ReplaceWord(i, w, x) if n > 0 => {
                let p = &mut paras[i % n];
                let k = w % p.words.len();
                p.words[k] = x;
            }
            Edit::AppendWord(i, x) if n > 0 => paras[i % n].words.push(x),
            Edit::ToggleBold(i) if n > 0 => paras[i % n].bold = !paras[i % n].bold,
            Edit::Restyle(i, s) if n > 0 => paras[i % n].style = s.clone(),
            Edit::Move(i, j) if n > 1 => {
                let p = paras.remove(i % n);
                paras.insert(j % n, p);
            }
            _ => {}
        }
    }
    paras
}

/// What a reader sees: style, list, text and bold for every character.
type Signature = Vec<(Option<String>, Option<NumRef>, String, Vec<bool>)>;

fn signature(docx: &Docx) -> Signature {
    docx.body()
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => {
                let mut bold = Vec::new();
                for inline in &p.inlines {
                    if let lineage_core::docx::Inline::Text { text, run } = inline {
                        let b = p.runs[*run].props.direct.get("b").is_some();
                        bold.extend(std::iter::repeat_n(b, text.chars().count()));
                    }
                }
                Some((
                    p.props.style.clone(),
                    p.props.numbering.clone(),
                    p.text(),
                    bold,
                ))
            }
            _ => None,
        })
        .collect()
}

fn bring_in_everything(base: &Docx, ours: &Docx, theirs: &Docx) -> Vec<u8> {
    let plan = merge::plan(base, ours, theirs);
    let decisions: Vec<(String, Decision)> = plan
        .items
        .iter()
        .filter(|i| i.status == Status::Clean)
        .map(|i| (i.change.id.clone(), Decision::BringIn))
        .collect();
    apply(
        &plan,
        &decisions,
        &Revision::lineage("2026-10-15T09:30:00Z"),
    )
    .expect("writes")
}

/// Cases per property: `PROPTEST_CASES` if set (for long local runs), else `default`.
fn cases(default: u32) -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(64), ..ProptestConfig::default() })]

    #[test]
    fn a_document_has_no_changes_against_itself(paras in document()) {
        let docx = Docx::open(&build(&paras)).expect("opens");
        prop_assert!(diff(&docx, &docx).is_empty());
    }

    #[test]
    fn merging_with_theirs_as_the_base_changes_nothing(paras in document(), edits in proptest::collection::vec(edit(), 0..6)) {
        let base = Docx::open(&build(&paras)).expect("opens");
        let x_bytes = build(&apply_edits(paras, &edits));
        let x = Docx::open(&x_bytes).expect("opens");
        let plan = merge::plan(&base, &x, &base);
        prop_assert!(plan.items.is_empty());
        prop_assert_eq!(apply(&plan, &[], &Revision::lineage("2026-10-15T09:30:00Z")).expect("writes"), x_bytes);
    }

    #[test]
    fn bringing_in_every_change_reproduces_the_target(paras in document(), edits in proptest::collection::vec(edit(), 1..6)) {
        let base_bytes = build(&paras);
        let base = Docx::open(&base_bytes).expect("opens");
        let target = Docx::open(&build(&apply_edits(paras, &edits))).expect("opens");
        let plan = merge::plan(&base, &base, &target);
        for item in &plan.items {
            prop_assert_eq!(&item.status, &Status::Clean, "{:?}", item.change);
        }
        let out = bring_in_everything(&base, &base, &target);
        let accepted = Docx::open_view(&out, View::Accepted).expect("opens");
        let rejected = Docx::open_view(&out, View::Rejected).expect("opens");
        prop_assert_eq!(signature(&accepted), signature(&target));
        prop_assert_eq!(signature(&rejected), signature(&base));
    }

    #[test]
    fn edits_to_different_paragraphs_merge_without_overlaps(
        paras in proptest::collection::vec(paragraph(), 4..10),
        mine in proptest::collection::vec((any::<usize>(), proptest::sample::select(WORDS)), 1..4),
        masters in proptest::collection::vec((any::<usize>(), proptest::sample::select(WORDS)), 1..4),
    ) {
        // Mine edits even paragraphs, the master's edits odd ones.
        let n = paras.len();
        let pick = |i: usize, parity: usize| (i % (n / 2)) * 2 + parity;
        let mut ours_paras = paras.clone();
        for (i, word) in &mine {
            ours_paras[pick(*i, 0)].words.push(word);
        }
        let mut theirs_paras = paras.clone();
        for (i, word) in &masters {
            let k = pick(*i, 1).min(n - 1);
            if k % 2 == 1 {
                theirs_paras[k].words.push(word);
            }
        }
        let mut expected = ours_paras.clone();
        for (k, p) in theirs_paras.iter().enumerate() {
            if k % 2 == 1 {
                expected[k] = p.clone();
            }
        }
        let base = Docx::open(&build(&paras)).expect("opens");
        let ours = Docx::open(&build(&ours_paras)).expect("opens");
        let theirs = Docx::open(&build(&theirs_paras)).expect("opens");
        let plan = merge::plan(&base, &ours, &theirs);
        for item in &plan.items {
            prop_assert!(!matches!(item.status, Status::Overlap(_)), "{:?}", item);
        }
        let out = bring_in_everything(&base, &ours, &theirs);
        let merged = Docx::open(&out).expect("opens");
        prop_assert_eq!(signature(&merged), signature(&Docx::open(&build(&expected)).expect("opens")));
    }

    #[test]
    fn splices_match_a_naive_rebuild(
        text in "[a-z<>&é ]{0,60}",
        cuts in proptest::collection::vec((any::<usize>(), any::<usize>(), "[A-Z]{0,4}"), 0..6),
    ) {
        // Non-overlapping ranges on character boundaries, sorted.
        let bounds: Vec<usize> = text.char_indices().map(|(i, _)| i).chain(std::iter::once(text.len())).collect();
        let mut ranges: Vec<(usize, usize, String)> = cuts
            .into_iter()
            .map(|(a, b, t)| {
                let (x, y) = (bounds[a % bounds.len()], bounds[b % bounds.len()]);
                (x.min(y), x.max(y), t)
            })
            .collect();
        ranges.sort_by_key(|r| (r.0, r.1));
        let mut kept: Vec<(usize, usize, String)> = Vec::new();
        for r in ranges {
            if kept.last().is_none_or(|last| r.0 >= last.1 && r.0 > last.0) {
                kept.push(r);
            }
        }
        let mut edits = Edits::new();
        let mut naive = String::new();
        let mut cursor = 0;
        for (start, end, replacement) in &kept {
            edits.replace(*start..*end, replacement.clone());
            naive.push_str(&text[cursor..*start]);
            naive.push_str(replacement);
            cursor = *end;
        }
        naive.push_str(&text[cursor..]);
        prop_assert_eq!(edits.apply(&text).expect("applies"), naive);
    }

    #[test]
    fn zip_writes_read_back(entries in proptest::collection::vec(("[a-z]{1,8}(/[a-z]{1,8})?\\.(xml|bin)", proptest::collection::vec(any::<u8>(), 0..400)), 1..8)) {
        let mut seen = std::collections::BTreeSet::new();
        let entries: Vec<(String, Vec<u8>)> = entries.into_iter().filter(|(n, _)| seen.insert(n.to_ascii_lowercase())).collect();
        let package = Package::from_parts(entries.clone());
        let bytes = package.to_bytes().expect("writes");
        let read = Package::read(&bytes).expect("reads");
        prop_assert_eq!(read.parts(), entries);
        prop_assert_eq!(read.to_bytes().expect("writes again"), bytes);
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(256), ..ProptestConfig::default() })]

    /// A stable-toolchain stand-in for fuzzing: damaged files are refused, never a panic.
    #[test]
    fn the_reader_never_panics_on_damaged_files(
        which in 0usize..38,
        flips in proptest::collection::vec((any::<usize>(), any::<u8>()), 0..12),
        cut in proptest::option::of(any::<usize>()),
    ) {
        let all = lineage_testkit::fixtures::all();
        let mut bytes = all[which % all.len()].1.to_bytes();
        for (at, value) in flips {
            let at = at % bytes.len();
            bytes[at] = value;
        }
        if let Some(cut) = cut {
            bytes.truncate(cut % (bytes.len() + 1));
        }
        if let Ok(docx) = Docx::open(&bytes) {
            let _ = diff(&docx, &docx);
            let _ = lineage_core::clean::clean_copy(&bytes, None);
        }
    }
}
