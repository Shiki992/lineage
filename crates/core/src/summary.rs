// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Plain-English summaries of changes, for the panel and the change list.
//!
//! Summaries use the words people use (CLAUDE.md glossary): never "diff",
//! "merge", "commit" or "branch".

use crate::diff::{Change, ChangeKind, FormatNote, SegmentKind, Unit};
use crate::docx::{Block, Docx, StyleNames};
use crate::merge::locate::sequence_at;

/// Longest quote, in characters, before it is cut at a word with "…".
pub const QUOTE_LIMIT: usize = 60;

fn tidy(text: &str) -> String {
    text.split(|c: char| c.is_whitespace())
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// A short quote of `text`: whitespace tidied, cut at a word boundary.
#[must_use]
pub fn quote(text: &str) -> String {
    let text = tidy(&text.replace('\t', " · "));
    if text.chars().count() <= QUOTE_LIMIT {
        return format!("'{text}'");
    }
    let mut out = String::new();
    for word in text.split(' ') {
        if out.chars().count() + word.chars().count() + 1 > QUOTE_LIMIT {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    if out.is_empty() {
        out = text.chars().take(QUOTE_LIMIT).collect();
    }
    format!("'{}…'", out.trim_end_matches([',', ';', ':', '.']))
}

fn row_text(text: &str) -> String {
    text.replace('\t', " · ")
}

fn note_phrase(note: &FormatNote, styles: &StyleNames) -> String {
    let style = |id: &Option<String>| {
        id.as_deref()
            .map_or_else(|| "Normal".to_owned(), |s| styles.display(s))
    };
    match note {
        FormatNote::Toggle { property, on } => {
            let (yes, no) = match *property {
                "b" => ("made bold", "no longer bold"),
                "i" => ("made italic", "no longer italic"),
                "u" => ("underlined", "no longer underlined"),
                _ => ("struck through", "no longer struck through"),
            };
            if *on { yes } else { no }.to_owned()
        }
        FormatNote::ParagraphStyle { from, to } => {
            format!("{} changed to {}", style(from), style(to))
        }
        FormatNote::CharacterStyle => "text style changed".to_owned(),
        FormatNote::List { added: true } => "made a list item".to_owned(),
        FormatNote::List { added: false } => "no longer a list item".to_owned(),
        FormatNote::ListLevel => "list level changed".to_owned(),
        FormatNote::Colour => "colour changed".to_owned(),
        FormatNote::Highlight => "highlight changed".to_owned(),
        FormatNote::Size => "text size changed".to_owned(),
        FormatNote::Font => "font changed".to_owned(),
        FormatNote::Layout => "paragraph layout changed".to_owned(),
        FormatNote::Table => "table formatting changed".to_owned(),
        FormatNote::Section => "page layout changed".to_owned(),
        FormatNote::Other => "formatting changed".to_owned(),
    }
}

fn is_run_note(note: &FormatNote) -> bool {
    matches!(
        note,
        FormatNote::Toggle { .. }
            | FormatNote::CharacterStyle
            | FormatNote::Colour
            | FormatNote::Highlight
            | FormatNote::Size
            | FormatNote::Font
    )
}

fn join_phrases(phrases: &[String]) -> String {
    match phrases {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The changed part of an edit, snapped to whole words, with "…" where text was
/// left out. Returns (before, after).
fn changed_words(before: &str, after: &str) -> (String, String) {
    let a: Vec<&str> = before.split(' ').collect();
    let b: Vec<&str> = after.split(' ').collect();
    let prefix = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let max_suffix = a.len().min(b.len()) - prefix;
    let suffix = a
        .iter()
        .rev()
        .zip(b.iter().rev())
        .take(max_suffix)
        .take_while(|(x, y)| x == y)
        .count();
    let piece = |words: &[&str]| {
        let start = prefix;
        let end = words.len() - suffix;
        let mut out = words[start..end].join(" ");
        if out.is_empty() {
            out = String::new();
        }
        out
    };
    (piece(&a), piece(&b))
}

fn where_prefix(change: &Change) -> String {
    let in_table = change
        .after
        .as_ref()
        .or(change.before.as_ref())
        .is_some_and(|p| !p.cells.is_empty())
        || change.unit == Unit::Row;
    match (&change.heading, in_table) {
        (Some(h), true) => format!("In the table under {}: ", quote(h)),
        (None, true) => "In a table: ".to_owned(),
        _ => String::new(),
    }
}

fn under(change: &Change) -> String {
    change
        .heading
        .as_ref()
        .map(|h| format!(" under {}", quote(h)))
        .unwrap_or_default()
}

/// The text of the next block after the change in the later document, for
/// "Moved 'Skills' above 'Experience'".
fn next_text(after_doc: Option<&Docx>, change: &Change) -> Option<String> {
    let doc = after_doc?;
    let path = change.after.as_ref()?;
    let blocks = sequence_at(doc, &change.story, path)?;
    let index = path
        .cells
        .last()
        .and_then(|s| s.block)
        .unwrap_or(path.block);
    blocks[index + 1..].iter().find_map(|b| match b {
        Block::Paragraph(p) if !p.text().trim().is_empty() => Some(p.text()),
        _ => None,
    })
}

/// A plain-English summary of one change. `after_doc` is the later document,
/// used to name what a moved paragraph now sits above.
#[must_use]
pub fn summarize(change: &Change, styles: &StyleNames, after_doc: Option<&Docx>) -> String {
    let prefix = where_prefix(change);
    let before = change.before_text.as_deref().map(tidy).unwrap_or_default();
    let after = change.after_text.as_deref().map(tidy).unwrap_or_default();
    let text = match (change.kind, change.unit) {
        (ChangeKind::Insert, Unit::Row) => format!("Added a row: {}", quote(&row_text(&after))),
        (ChangeKind::Delete, Unit::Row) => format!("Removed a row: {}", quote(&row_text(&before))),
        (ChangeKind::Insert, Unit::Table) => format!("Added a table{}", under(change)),
        (ChangeKind::Delete, Unit::Table) => format!("Removed a table{}", under(change)),
        (ChangeKind::Insert, Unit::SectionBreak) => {
            format!("Added a section break{}", under(change))
        }
        (ChangeKind::Delete, Unit::SectionBreak) => {
            format!("Removed a section break{}", under(change))
        }
        (_, Unit::SectionBreak) => format!("Page layout changed{}", under(change)),
        (ChangeKind::Insert, _) if after.is_empty() => {
            format!("Added a blank line{}", under(change))
        }
        (ChangeKind::Delete, _) if before.is_empty() => {
            format!("Removed a blank line{}", under(change))
        }
        (ChangeKind::Insert, _) => match &change.heading {
            Some(h) if prefix.is_empty() => format!("Added under {}: {}", quote(h), quote(&after)),
            _ => format!("Added: {}", quote(&after)),
        },
        (ChangeKind::Delete, _) => format!("Removed: {}", quote(&before)),
        (ChangeKind::Move, _) => {
            let what = if before == after {
                "Moved"
            } else {
                "Moved and changed"
            };
            match next_text(after_doc, change) {
                Some(next) => format!("{what} {} above {}", quote(&after), quote(&next)),
                None => format!("{what} {} to the end", quote(&after)),
            }
        }
        (ChangeKind::Format, _) => {
            let phrases: Vec<String> = change
                .format_notes
                .iter()
                .map(|n| note_phrase(n, styles))
                .collect();
            let reformatted: String = change
                .segments
                .iter()
                .filter(|s| s.kind == SegmentKind::Reformatted)
                .map(|s| s.text.as_str())
                .collect();
            if change.format_notes.iter().all(is_run_note) && !reformatted.trim().is_empty() {
                format!("{} {}", quote(&reformatted), join_phrases(&phrases))
            } else if change.unit == Unit::Table {
                format!("Table formatting changed{}", under(change))
            } else {
                format!("{}: {}", quote(&after), join_phrases(&phrases))
            }
        }
        (ChangeKind::Modify, Unit::Table) => format!("Table changed{}", under(change)),
        (ChangeKind::Modify, _) => {
            if before.chars().count() <= QUOTE_LIMIT && after.chars().count() <= QUOTE_LIMIT {
                format!("{} changed to {}", quote(&before), quote(&after))
            } else {
                let (old, new) = changed_words(&before, &after);
                match (old.is_empty(), new.is_empty()) {
                    (true, false) => format!("Added {} in {}", quote(&new), quote(&after)),
                    (false, true) => format!("Removed {} from {}", quote(&old), quote(&before)),
                    _ if old.chars().count() <= QUOTE_LIMIT
                        && new.chars().count() <= QUOTE_LIMIT =>
                    {
                        format!("{} changed to {}", quote(&old), quote(&new))
                    }
                    _ => format!("Rewrote {}", quote(&after)),
                }
            }
        }
    };
    format!("{prefix}{text}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::diff;
    use crate::testutil::open_lines;
    use lineage_testkit::resume;

    fn summaries(a: &[resume::Line], b: &[resume::Line]) -> Vec<String> {
        let (x, y) = (open_lines(a), open_lines(b));
        diff(&x, &y)
            .changes
            .iter()
            .map(|c| summarize(c, &y.styles, Some(&y)))
            .collect()
    }

    #[test]
    fn a_new_job() {
        assert_eq!(
            summaries(&resume::master_v1_lines(), &resume::master_v2_lines()),
            [
                "Added under 'Experience': 'Senior data engineer, Halden Robotics, 2023 to now'",
                "Added under 'Senior data engineer, Halden Robotics, 2023 to now': 'Moved nightly batch jobs to streaming, cutting report delays…'",
            ]
        );
    }

    #[test]
    fn an_edit_and_bold() {
        assert_eq!(
            summaries(&resume::master_v2_lines(), &resume::master_v3_lines()),
            [
                "'Avery Quinn' made bold",
                "'Data engineer, Corvia Freight, 2020 to 2023' changed to 'Lead data engineer, Corvia Freight, 2020 to 2023'",
            ]
        );
    }

    #[test]
    fn a_new_section_is_called_a_section_break() {
        // Found in Phase 2: a paragraph that ends a section was summarised as
        // "Added a blank line", because the break itself has no text.
        let mut with_section = resume::master_v1_lines();
        with_section.push(resume::Line::Raw(lineage_testkit::docx::para(
            "<w:sectPr/>",
            &lineage_testkit::docx::run("References are available on request."),
        )));
        let got = summaries(&resume::master_v1_lines(), &with_section);
        assert!(
            got.iter().any(|s| s.starts_with("Added a section break")),
            "{got:?}"
        );
        assert!(!got.iter().any(|s| s.contains("blank line")), "{got:?}");
        let back = summaries(&with_section, &resume::master_v1_lines());
        assert!(
            back.iter()
                .any(|s| s.starts_with("Removed a section break")),
            "{back:?}"
        );
    }

    #[test]
    fn moves_name_their_new_neighbour() {
        let got = summaries(&resume::master_v1_lines(), &resume::reordered_lines());
        assert!(got.iter().all(|s| s.starts_with("Moved '")), "{got:?}");
        assert!(
            got.iter().any(|s| s.contains(" above 'Summary'")),
            "{got:?}"
        );
    }

    #[test]
    fn long_edits_quote_the_changed_words() {
        let mut lines = resume::master_v1_lines();
        lines[3] = resume::Line::Plain(
            "Data engineer with seven years of experience building reliable pipelines for retail and logistics teams.".into(),
        );
        assert_eq!(
            summaries(&resume::master_v1_lines(), &lines),
            ["'six' changed to 'seven'"]
        );
    }

    #[test]
    fn quotes_are_cut_at_a_word() {
        let long = "word ".repeat(30);
        let q = quote(&long);
        assert!(q.ends_with("…'"));
        assert!(q.chars().count() <= QUOTE_LIMIT + 3);
        assert_eq!(quote("short"), "'short'");
    }

    #[test]
    fn no_technical_words() {
        let all = [
            summaries(&resume::master_v1_lines(), &resume::tailored_data_lines()),
            summaries(&resume::master_v1_lines(), &resume::reordered_lines()),
            summaries(&resume::master_v2_lines(), &resume::master_v3_lines()),
        ]
        .concat();
        for s in all {
            for word in ["diff", "merge", "commit", "branch", "conflict"] {
                assert!(!s.to_lowercase().contains(word), "{s}");
            }
        }
    }
}
