// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Plain-English summaries of deck changes. The slide ("Slide 6 · Traction")
//! is the card's place, so summaries say what changed on it.

use super::diff::{Change, Unit};
use super::model::ShapeKind;
use crate::diff::ChangeKind;
use crate::summary::{changed_words, quote};

fn first_line(text: &str) -> &str {
    text.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or_default()
}

/// "the chart", or a quote of the shape's first line of text.
fn shape_words(kind: Option<ShapeKind>, text: Option<&str>) -> String {
    let line = text.map(first_line).unwrap_or_default();
    match kind {
        Some(ShapeKind::Text) if !line.is_empty() => quote(line),
        Some(ShapeKind::Text) | None => "a text box".to_owned(),
        Some(kind) => kind.noun().to_owned(),
    }
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The changed line of a text edit, as "'a' changed to 'b'".
fn edited(before: &str, after: &str) -> String {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    // The first line that differs, when both have the same number of lines.
    if a.len() == b.len()
        && let Some((x, y)) = a.iter().zip(&b).find(|(x, y)| x != y)
    {
        let (from, to) = changed_words(x, y);
        return match (from.is_empty(), to.is_empty()) {
            (true, false) => format!("Added {} in {}", quote(&to), quote(y)),
            (false, true) => format!("Removed {} from {}", quote(&from), quote(x)),
            _ => format!("{} changed to {}", quote(&from), quote(&to)),
        };
    }
    if b.len() > a.len()
        && let Some(added) = b.iter().find(|line| !a.contains(line))
    {
        return format!("Added {}", quote(added));
    }
    if a.len() > b.len()
        && let Some(removed) = a.iter().find(|line| !b.contains(line))
    {
        return format!("Removed {}", quote(removed));
    }
    format!("Rewrote {}", quote(first_line(before)))
}

/// The summary of one deck change.
#[must_use]
pub fn summarize(change: &Change) -> String {
    let before = change.before_text.as_deref();
    let after = change.after_text.as_deref();
    match (change.unit, change.kind) {
        (Unit::Design, _) => "The slide master, layouts or theme changed".to_owned(),
        (Unit::Animations, _) => "Animations changed".to_owned(),
        (Unit::Slide, ChangeKind::Insert) => {
            format!("Added {}", change.place().to_lowercase_first())
        }
        (Unit::Slide, ChangeKind::Delete) => {
            format!("Removed {}", change.place().to_lowercase_first())
        }
        (Unit::Slide, _) => format!("Moved {}", change.place().to_lowercase_first()),
        (Unit::Settings, _) => {
            let settings = change.settings.clone().unwrap_or_default();
            match (settings.hidden, settings.layout) {
                (Some(true), _) => "Hid this slide".to_owned(),
                (Some(false), _) => "Showed this slide again".to_owned(),
                (None, Some(layout)) if !layout.is_empty() => {
                    format!("Now uses the {} layout", quote(&layout))
                }
                _ => "Changed the background or transition".to_owned(),
            }
        }
        (Unit::Notes, ChangeKind::Format) => "Speaker notes formatting changed".to_owned(),
        (Unit::Notes, _) => match (before.unwrap_or_default(), after.unwrap_or_default()) {
            ("", text) => format!("Added speaker notes: {}", quote(first_line(text))),
            (_, "") => "Removed the speaker notes".to_owned(),
            (a, b) => format!("Speaker notes: {}", edited(a, b).to_lowercase_first()),
        },
        (Unit::Shape, ChangeKind::Insert) => match change.shape_kind {
            Some(ShapeKind::Text) => {
                format!("Added {}", quote(first_line(after.unwrap_or_default())))
            }
            kind => format!(
                "Added {}",
                shape_words(kind, after).replacen("the ", "a ", 1)
            ),
        },
        (Unit::Shape, ChangeKind::Delete) => {
            format!("Removed {}", shape_words(change.shape_kind, before))
        }
        (Unit::Shape, ChangeKind::Format) if change.geometry_only => {
            format!("Moved or resized {}", shape_words(change.shape_kind, after))
        }
        (Unit::Shape, ChangeKind::Format) => {
            format!(
                "{} formatting changed",
                capitalised(&shape_words(change.shape_kind, after))
            )
        }
        (Unit::Shape, _) => match change.shape_kind {
            Some(ShapeKind::Text | ShapeKind::Table | ShapeKind::Chart) if before != after => {
                let what = edited(before.unwrap_or_default(), after.unwrap_or_default());
                match change.shape_kind {
                    Some(ShapeKind::Chart) => {
                        format!("The chart changed: {}", what.to_lowercase_first())
                    }
                    Some(ShapeKind::Table) => {
                        format!("The table changed: {}", what.to_lowercase_first())
                    }
                    _ => what,
                }
            }
            kind => format!("{} changed", capitalised(&shape_words(kind, after))),
        },
    }
}

trait LowerFirst {
    fn to_lowercase_first(&self) -> String;
}

impl LowerFirst for str {
    /// "Slide 6 · Traction" → "slide 6 · Traction"; quotes are left alone.
    fn to_lowercase_first(&self) -> String {
        let mut chars = self.chars();
        match chars.next() {
            Some(first) if first != '\'' => first.to_lowercase().chain(chars).collect(),
            Some(first) => std::iter::once(first).chain(chars).collect(),
            None => String::new(),
        }
    }
}

impl LowerFirst for String {
    fn to_lowercase_first(&self) -> String {
        self.as_str().to_lowercase_first()
    }
}

#[cfg(test)]
mod tests {
    use lineage_testkit::decks::{edits, pitch_deck};

    use super::*;
    use crate::pptx::Deck;
    use crate::pptx::diff::diff;

    fn summaries(a: &lineage_testkit::pptx::Deck, b: &lineage_testkit::pptx::Deck) -> Vec<String> {
        let a = Deck::open(&a.to_bytes()).expect("opens");
        let b = Deck::open(&b.to_bytes()).expect("opens");
        diff(&a, &b).changes.iter().map(summarize).collect()
    }

    #[test]
    fn the_founder_reads_what_changed() {
        assert_eq!(
            summaries(&pitch_deck(), &edits::traction_update(pitch_deck())),
            [
                "The chart changed: '1200' changed to '2000'",
                "'$1.2M' changed to '$2.0M'"
            ]
        );
        assert_eq!(
            summaries(&pitch_deck(), &edits::northgate(pitch_deck())),
            [
                "'Seed round · 2026' changed to 'Prepared for Northgate Capital'",
                "Added slide 8 · Why Northgate"
            ]
        );
        assert_eq!(
            summaries(&pitch_deck(), &edits::reordered(pitch_deck())),
            ["Moved slide 6 · Market"]
        );
        assert_eq!(
            summaries(&pitch_deck(), &edits::team_removed(pitch_deck())),
            ["Removed slide 7 · Team"]
        );
        assert_eq!(
            summaries(&pitch_deck(), &edits::notes_edited(pitch_deck())),
            ["Speaker notes: added 'Mention the two pilots starting in November.'"]
        );
        assert_eq!(
            summaries(&pitch_deck(), &edits::picture_moved(pitch_deck())),
            ["Moved or resized the picture"]
        );
        assert_eq!(
            summaries(&pitch_deck(), &edits::made_bold(pitch_deck())),
            ["'4,800 community lenders in North America' formatting changed"]
        );
        assert_eq!(
            summaries(&pitch_deck(), &edits::links(pitch_deck())),
            ["Added 'Data room: lumenledger.example/room'"]
        );
    }

    #[test]
    fn summaries_never_use_git_words() {
        let decks = [
            edits::traction_update(pitch_deck()),
            edits::northgate(pitch_deck()),
            edits::duplicated(pitch_deck()),
            edits::layout_changed(pitch_deck()),
            edits::hidden_appendix(pitch_deck()),
            edits::table(pitch_deck()),
        ];
        for deck in &decks {
            for summary in summaries(&pitch_deck(), deck) {
                for word in ["diff", "merge", "commit", "branch", "XML", "zip"] {
                    assert!(!summary.contains(word), "{summary}");
                }
            }
        }
    }
}
