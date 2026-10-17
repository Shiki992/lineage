// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Clean copies of decks for sending (CLAUDE.md: "copies that leave are clean").
//!
//! A clean deck has no hidden slides, comments, revision information, tags or
//! add-in data, no personal metadata, thumbnail or custom XML, and no custom
//! properties but the two Lineage ids (or none). Speaker notes and the data
//! behind charts stay: the maintainer chose to keep them (decision 0018).

use std::collections::BTreeSet;

use super::Deck;
use super::write::remove_slides;
use crate::clean::{
    CleanReport, empty_properties, only_ids, prune_content_types, prune_relationships, source_of,
};
use crate::error::Result;
use crate::package::props::LineageIds;
use crate::xml::{self, Edits, ns};

fn removable(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.starts_with("ppt/comments/")
        || matches!(
            lower.as_str(),
            "ppt/commentauthors.xml" | "ppt/authors.xml" | "ppt/revisioninfo.xml"
        )
        || lower.starts_with("ppt/tags/")
        || lower.starts_with("docprops/thumbnail")
        || lower.starts_with("customxml/")
        || lower.contains("webextensions/")
}

/// Makes a clean copy of a deck for sending. With `ids`, the copy carries the
/// two Lineage ids as its only custom properties; without, none.
///
/// # Errors
/// [`crate::Error`] when the deck can't be read or cleaned safely.
pub fn clean_copy(bytes: &[u8], ids: Option<&LineageIds>) -> Result<(Vec<u8>, CleanReport)> {
    let mut report = CleanReport::default();
    let deck = Deck::open(bytes)?;
    let hidden: Vec<usize> = deck
        .slides
        .iter()
        .enumerate()
        .filter(|(_, s)| s.hidden)
        .map(|(i, _)| i)
        .collect();
    let deck = if hidden.is_empty() {
        deck
    } else {
        report.hidden_slides = hidden.len();
        Deck::open(&remove_slides(&deck, &hidden)?)?
    };
    let mut package = deck.package.clone();

    // Comments are counted, then their parts go.
    for name in package
        .part_names()
        .filter(|n| n.to_ascii_lowercase().starts_with("ppt/comments/"))
    {
        let Some(bytes) = package.part(name) else {
            continue;
        };
        if let Ok(text) = xml::text(name, bytes)
            && let Ok(doc) = xml::parse(name, text)
        {
            report.comments += doc
                .descendants()
                .filter(|n| n.is_element() && n.tag_name().name() == "cm")
                .count();
        }
    }

    // Tags are reached through p:custDataLst in slides and the presentation.
    let parts: Vec<String> = package
        .part_names()
        .filter(|n| {
            let lower = n.to_ascii_lowercase();
            lower.starts_with("ppt/") && lower.ends_with(".xml") && !removable(n)
        })
        .map(str::to_owned)
        .collect();
    for part in parts {
        let Some(bytes) = package.part(&part) else {
            continue;
        };
        let Ok(text) = xml::text(&part, bytes) else {
            continue;
        };
        if !text.contains("custDataLst") {
            continue;
        }
        let doc = xml::parse(&part, text)?;
        let mut edits = Edits::new();
        for node in doc
            .descendants()
            .filter(|n| xml::is(*n, ns::P, "custDataLst"))
        {
            edits.delete(node.range());
        }
        if !edits.is_empty() {
            let new = edits.apply(text)?;
            package.set_part(&part, new.into_bytes());
        }
    }

    let removed: BTreeSet<String> = package
        .part_names()
        .filter(|n| removable(n))
        .map(str::to_ascii_lowercase)
        .collect();
    let mut removed_names: Vec<String> = package
        .part_names()
        .filter(|n| removable(n))
        .map(str::to_owned)
        .collect();
    for name in &removed_names {
        package.remove_part(name);
    }
    let orphan_rels: Vec<String> = package
        .part_names()
        .filter(|n| n.ends_with(".rels") && removed.contains(&source_of(n).to_ascii_lowercase()))
        .map(str::to_owned)
        .collect();
    for name in &orphan_rels {
        package.remove_part(name);
    }
    removed_names.extend(orphan_rels);

    let mut removed_set = removed;
    only_ids(&mut package, ids, &mut removed_set, &mut removed_names)?;
    prune_relationships(&mut package, &removed_set)?;
    prune_content_types(&mut package, &removed_set)?;
    empty_properties(&mut package);

    report.parts_removed = removed_names;
    let out = package.to_bytes()?;
    // The copy must still be a deck that holds together.
    super::check::structure(&out)?;
    Ok((out, report))
}

#[cfg(test)]
mod tests {
    use lineage_testkit::decks::{edits, ids, pitch_deck};

    use super::*;
    use crate::package::props::{custom_properties, read_ids};

    fn lineage_ids() -> LineageIds {
        LineageIds {
            family: format!("f{:031}", 1),
            checkpoint: format!("s{:031}", 2),
        }
    }

    #[test]
    fn hidden_slides_comments_and_metadata_leave_notes_and_charts_stay() {
        let mut deck = edits::comments(edits::hidden_appendix(pitch_deck()));
        deck.custom = vec![("Owner".to_owned(), "Rowan Ellis".to_owned())];
        let (out, report) = clean_copy(&deck.to_bytes(), Some(&lineage_ids())).expect("cleans");
        let clean = Deck::open(&out).expect("opens");
        assert_eq!(report.hidden_slides, 1);
        assert_eq!(report.comments, 3);
        assert_eq!(clean.slides.len(), 8);
        assert!(clean.slides.iter().all(|s| !s.hidden));
        assert!(
            !clean
                .package
                .part_names()
                .any(|n| n.starts_with("ppt/comments/"))
        );
        assert!(!clean.package.contains("ppt/commentAuthors.xml"));
        // Notes and the chart's workbook stay.
        assert!(clean.slide(ids::TRACTION).expect("t").notes.is_some());
        assert!(
            clean
                .package
                .part_names()
                .any(|n| n.starts_with("ppt/embeddings/"))
        );
        // Only the two ids are left as custom properties, and nobody is named.
        assert_eq!(
            read_ids(&clean.package).expect("reads"),
            Some(lineage_ids())
        );
        assert_eq!(custom_properties(&clean.package).expect("reads").len(), 2);
        let core = String::from_utf8(
            clean
                .package
                .part("docProps/core.xml")
                .expect("core")
                .to_vec(),
        )
        .expect("utf-8");
        assert!(!core.contains("Lineage fixture generator"));
        let app = String::from_utf8(
            clean
                .package
                .part("docProps/app.xml")
                .expect("app")
                .to_vec(),
        )
        .expect("utf-8");
        assert!(!app.contains("Lumen Ledger"));
    }

    #[test]
    fn without_ids_there_are_no_custom_properties() {
        let deck = edits::sections(pitch_deck());
        let (out, report) = clean_copy(&deck.to_bytes(), None).expect("cleans");
        let clean = Deck::open(&out).expect("opens");
        assert_eq!(report.hidden_slides, 0);
        assert_eq!(read_ids(&clean.package).expect("reads"), None);
        assert_eq!(clean.sections.len(), 2);
        // The slides read the same as before.
        let before = Deck::open(&deck.to_bytes()).expect("opens");
        assert_eq!(clean.lines(), before.lines());
    }

    #[test]
    fn a_hidden_slide_in_a_section_leaves_its_section_too() {
        let mut deck = edits::sections(edits::hidden_appendix(pitch_deck()));
        deck.sections[1].1.push(264);
        let (out, _) = clean_copy(&deck.to_bytes(), None).expect("cleans");
        let clean = Deck::open(&out).expect("opens");
        assert!(!clean.sections[1].slides.contains(&264));
        crate::testutil::smoke_deck("deck-clean-sections", &out);
    }
}
