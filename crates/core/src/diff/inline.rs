// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What changed inside one paragraph: a token diff over words, spaces,
//! punctuation and atoms, with formatting compared on the tokens that match.

use super::keys::{container_key, run_props_key};
use super::lcs::lcs;
use crate::docx::{Inline, Paragraph};
use crate::hash::Fnv;

/// One token of a paragraph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// The text a person sees (an atom's display text, possibly empty).
    pub text: String,
    /// Identity for matching: the text, or the atom's key.
    pub key: u64,
    /// Formatting of the token: run properties and container.
    pub props: u64,
    /// The inline it came from.
    pub inline: usize,
    /// Character offset in the inline's text where the token starts (0 for atoms).
    pub offset: usize,
    /// Character offset in the paragraph's text.
    pub start: usize,
    pub is_atom: bool,
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
    } else if c.is_whitespace() && c != '\t' && c != '\n' {
        Class::Space
    } else {
        Class::Other
    }
}

/// Splits a paragraph into tokens. Words never span two runs with different
/// formatting, so a token always has one formatting.
#[must_use]
pub fn tokens(p: &Paragraph, source: Option<&str>) -> Vec<Token> {
    let mut out = Vec::new();
    let mut start = 0;
    for (index, inline) in p.inlines.iter().enumerate() {
        match inline {
            Inline::Text { text, run } => {
                let run = &p.runs[*run];
                let props = run_props_key(
                    &run.props,
                    container_key(p, run.container, source).as_deref(),
                );
                let chars: Vec<char> = text.chars().collect();
                let mut k = 0;
                while k < chars.len() {
                    let c = class(chars[k]);
                    let mut end = k + 1;
                    if c != Class::Other {
                        while end < chars.len() && class(chars[end]) == c {
                            end += 1;
                        }
                    }
                    let piece: String = chars[k..end].iter().collect();
                    // Text in one run that continues a word from the previous
                    // run with the same formatting joins that token.
                    if let Some(last) = out.last_mut()
                        && k == 0
                        && joins(last, &piece, props)
                    {
                        let last: &mut Token = last;
                        last.text.push_str(&piece);
                        last.key = Fnv::new().str("t").str(&last.text).finish();
                    } else {
                        out.push(Token {
                            key: Fnv::new().str("t").str(&piece).finish(),
                            text: piece,
                            props,
                            inline: index,
                            offset: k,
                            start: start + k,
                            is_atom: false,
                        });
                    }
                    k = end;
                }
                start += chars.len();
            }
            Inline::Atom(atom) => {
                let props = atom
                    .run
                    .map_or(0, |r| run_props_key(&p.runs[r].props, None));
                out.push(Token {
                    text: atom.display.clone(),
                    key: Fnv::new().str("a").u64(atom.key).finish(),
                    props,
                    inline: index,
                    offset: 0,
                    start,
                    is_atom: true,
                });
                start += atom.display.chars().count();
            }
            Inline::Marker(_) => {}
        }
    }
    out
}

fn joins(last: &Token, piece: &str, props: u64) -> bool {
    if last.is_atom || last.props != props {
        return false;
    }
    let (Some(a), Some(b)) = (last.text.chars().last(), piece.chars().next()) else {
        return false;
    };
    class(a) == class(b) && class(a) != Class::Other
}

/// One step of a paragraph diff.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Token `a` and token `b` are the same text; `format_changed` if their
    /// formatting differs.
    Same {
        a: usize,
        b: usize,
        format_changed: bool,
    },
    Removed {
        a: usize,
    },
    Added {
        b: usize,
    },
}

/// The diff of two paragraphs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlineDiff {
    pub a: Vec<Token>,
    pub b: Vec<Token>,
    pub steps: Vec<Step>,
}

impl InlineDiff {
    #[must_use]
    pub fn text_changed(&self) -> bool {
        self.steps.iter().any(|s| !matches!(s, Step::Same { .. }))
    }

    #[must_use]
    pub fn format_changed(&self) -> bool {
        self.steps.iter().any(|s| {
            matches!(
                s,
                Step::Same {
                    format_changed: true,
                    ..
                }
            )
        })
    }

    /// Half-open character range in `b`'s text covering every change, if any.
    #[must_use]
    pub fn changed_range(&self) -> Option<(usize, usize)> {
        let mut range: Option<(usize, usize)> = None;
        let mut extend = |start: usize, end: usize| {
            range = Some(match range {
                Some((s, e)) => (s.min(start), e.max(end)),
                None => (start, end),
            });
        };
        for (index, step) in self.steps.iter().enumerate() {
            match *step {
                Step::Same {
                    b,
                    format_changed: true,
                    ..
                }
                | Step::Added { b } => {
                    let t = &self.b[b];
                    extend(t.start, t.start + t.text.chars().count());
                }
                Step::Removed { .. } => {
                    // Where the removed text was: just before the next token of b.
                    let at = self.steps[index..]
                        .iter()
                        .find_map(|s| match s {
                            Step::Same { b, .. } | Step::Added { b } => Some(self.b[*b].start),
                            Step::Removed { .. } => None,
                        })
                        .unwrap_or_else(|| {
                            self.b
                                .last()
                                .map_or(0, |t| t.start + t.text.chars().count())
                        });
                    extend(at, at);
                }
                Step::Same { .. } => {}
            }
        }
        range
    }
}

/// Diffs two paragraphs.
#[must_use]
pub fn diff_paragraphs(
    a: &Paragraph,
    b: &Paragraph,
    a_src: Option<&str>,
    b_src: Option<&str>,
) -> InlineDiff {
    let ta = tokens(a, a_src);
    let tb = tokens(b, b_src);
    let ka: Vec<u64> = ta.iter().map(|t| t.key).collect();
    let kb: Vec<u64> = tb.iter().map(|t| t.key).collect();
    let pairs = lcs(&ka, &kb);
    let mut steps = Vec::with_capacity(ta.len().max(tb.len()));
    let (mut i, mut j) = (0, 0);
    for (pi, pj) in pairs {
        while i < pi {
            steps.push(Step::Removed { a: i });
            i += 1;
        }
        while j < pj {
            steps.push(Step::Added { b: j });
            j += 1;
        }
        steps.push(Step::Same {
            a: pi,
            b: pj,
            format_changed: ta[pi].props != tb[pj].props,
        });
        i = pi + 1;
        j = pj + 1;
    }
    while i < ta.len() {
        steps.push(Step::Removed { a: i });
        i += 1;
    }
    while j < tb.len() {
        steps.push(Step::Added { b: j });
        j += 1;
    }
    InlineDiff {
        a: ta,
        b: tb,
        steps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docx::{Block, Docx};
    use lineage_testkit::docx::{Package, para, run, run_with};

    fn paragraph(content: &str) -> Paragraph {
        let docx = Docx::open(&Package::new("t", para("", content)).to_bytes()).expect("opens");
        match &docx.body().blocks[0] {
            Block::Paragraph(p) => p.clone(),
            other => panic!("{other:?}"),
        }
    }

    fn texts(tokens: &[Token]) -> Vec<&str> {
        tokens.iter().map(|t| t.text.as_str()).collect()
    }

    #[test]
    fn tokens_ignore_run_splits_but_not_formatting() {
        let split = paragraph(&[run("Data engi"), run("neer, 2020")].concat());
        assert_eq!(
            texts(&tokens(&split, None)),
            ["Data", " ", "engineer", ",", " ", "2020"]
        );
        let bold = paragraph(&[run("Data "), run_with("<w:b/>", "engi"), run("neer")].concat());
        assert_eq!(texts(&tokens(&bold, None)), ["Data", " ", "engi", "neer"]);
    }

    #[test]
    fn word_level_diff() {
        let a = paragraph(&run("Data engineer, Corvia Freight"));
        let b = paragraph(&run("Lead data engineer, Corvia Freight"));
        let d = diff_paragraphs(&a, &b, None, None);
        assert!(d.text_changed());
        assert!(!d.format_changed());
        let removed: Vec<&str> = d
            .steps
            .iter()
            .filter_map(|s| match s {
                Step::Removed { a } => Some(d.a[*a].text.as_str()),
                _ => None,
            })
            .collect();
        let added: Vec<&str> = d
            .steps
            .iter()
            .filter_map(|s| match s {
                Step::Added { b } => Some(d.b[*b].text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(removed, ["Data"]);
        assert_eq!(added, ["Lead", " ", "data"]);
        assert_eq!(d.changed_range(), Some((0, 9)));
    }

    #[test]
    fn formatting_only() {
        let a = paragraph(&run("Avery Quinn"));
        let b = paragraph(&run_with("<w:b/>", "Avery Quinn"));
        let d = diff_paragraphs(&a, &b, None, None);
        assert!(!d.text_changed());
        assert!(d.format_changed());
        assert_eq!(d.changed_range(), Some((0, 11)));
    }
}
