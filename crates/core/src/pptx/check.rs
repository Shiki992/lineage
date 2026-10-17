// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Checks on a written deck, before anything is saved: the package holds
//! together the way PowerPoint needs, and the result says what was decided.

use std::collections::BTreeSet;

use super::Deck;
use super::diff::{Unit, slide_content};
use super::merge::Plan;
use crate::diff::ChangeKind;
use crate::error::{Error, Result};
use crate::merge::{Decision, Status};
use crate::package::{ContentTypes, Package, Relationships, resolve_target};
use crate::xml::{self, ns};

fn broken(detail: String) -> Error {
    Error::CannotEdit(detail)
}

/// The package holds together: every relationship target exists, every part
/// has a content type, slide ids and every slide's shape ids are unique, and
/// every relationship id a slide uses is one it has.
///
/// # Errors
/// [`Error::CannotEdit`] naming the first problem (structure only, never text).
pub fn structure(bytes: &[u8]) -> Result<Deck> {
    let deck = Deck::open(bytes).map_err(|e| broken(format!("the result doesn't open: {e}")))?;
    let package = &deck.package;
    let types = ContentTypes::parse(
        package
            .part(ContentTypes::PART)
            .ok_or_else(|| broken("no content types".to_owned()))?,
    )?;
    for name in package.part_names() {
        if name == ContentTypes::PART {
            continue;
        }
        if types.of(name).is_none() {
            return Err(broken(format!("{name} has no content type")));
        }
        if !name.ends_with(".rels") {
            continue;
        }
        let source = crate::clean::source_of(name);
        if !source.is_empty() && !package.contains(&source) {
            return Err(broken(format!("{name} belongs to no part")));
        }
        let rels = Relationships::parse(name, package.part(name).unwrap_or_default())?;
        let mut ids = BTreeSet::new();
        for rel in &rels.items {
            if !ids.insert(rel.id.as_str()) {
                return Err(broken(format!("{name} uses {} twice", rel.id)));
            }
            if !rel.external && !package.contains(&resolve_target(&source, &rel.target)) {
                return Err(broken(format!("{name} points at a missing part")));
            }
        }
    }
    let mut slide_ids = BTreeSet::new();
    for slide in &deck.slides {
        if !(256..2_147_483_648).contains(&slide.sld_id) || !slide_ids.insert(slide.sld_id) {
            return Err(broken(format!("slide id {} is not valid", slide.sld_id)));
        }
        let text = deck.part_text(&slide.part)?;
        let doc = xml::parse(&slide.part, text)?;
        let rels = super::read::read_rels(package, &slide.part)?;
        let mut shape_ids = BTreeSet::new();
        for node in doc.descendants().filter(roxmltree::Node::is_element) {
            for attribute in node.attributes().filter(|a| a.namespace() == Some(ns::R)) {
                if rels.by_id(attribute.value()).is_none() {
                    return Err(broken(format!(
                        "{} uses a missing relationship",
                        slide.part
                    )));
                }
            }
            if xml::is(node, ns::P, "cNvPr")
                && let Some(id) = node.attribute("id").and_then(|v| v.parse::<u32>().ok())
                && !shape_ids.insert(id)
            {
                return Err(broken(format!("{} has shape id {id} twice", slide.part)));
            }
        }
    }
    for section in &deck.sections {
        if section.slides.iter().any(|id| !slide_ids.contains(id)) {
            return Err(broken("a section lists a missing slide".to_owned()));
        }
    }
    if !deck.sections.is_empty() {
        let listed: BTreeSet<u32> = deck
            .sections
            .iter()
            .flat_map(|s| s.slides.iter().copied())
            .collect();
        if listed != slide_ids {
            return Err(broken("a slide is in no section".to_owned()));
        }
    }
    Ok(deck)
}

/// The written deck says what was decided: everything brought in is there,
/// everything removed is gone, and every slide nothing was brought into is
/// exactly as it was.
///
/// # Errors
/// [`Error::CannotEdit`] naming what is wrong (structure only, never text).
pub fn written(plan: &Plan<'_>, decisions: &[(String, Decision)], bytes: &[u8]) -> Result<()> {
    let result = structure(bytes)?;
    let brought: Vec<_> = plan
        .items
        .iter()
        .filter(|item| {
            decisions.iter().any(|(id, d)| {
                *id == item.change.id
                    && d.brings_in()
                    && !matches!(item.status, Status::AlreadyThere { .. } | Status::ByHand(_))
            })
        })
        .collect();
    let ours = plan.ours;
    let theirs = plan.theirs;

    // Slides of ours that something was brought into, or removed, or replaced.
    let mut touched: BTreeSet<usize> = BTreeSet::new();
    for item in &brought {
        if let Some(base) = item.change.before
            && let Some(o) = plan.ours_diff.after_of(base.slide)
        {
            touched.insert(o);
        }
        if let Some(gap) = item.change.gap
            && let super::diff::Gap::Shape { slide, .. } = gap
            && let Some(o) = plan.ours_diff.after_of(slide)
        {
            touched.insert(o);
        }
        if let Some(mine) = &item.mine
            && let Some(after) = mine.after
        {
            touched.insert(after.slide);
        }
    }
    let result_fulls: Vec<u64> = result.slides.iter().map(|s| s.full).collect();
    for (index, slide) in ours.slides.iter().enumerate() {
        if !touched.contains(&index) && !result_fulls.contains(&slide.full) {
            return Err(broken(format!(
                "slide {} changed but nothing was brought into it",
                index + 1
            )));
        }
    }

    let contents: Vec<u64> = result.slides.iter().map(slide_content).collect();
    for item in &brought {
        let change = &item.change;
        match (change.unit, change.kind) {
            (Unit::Slide, ChangeKind::Insert) => {
                let slide = &theirs.slides[change.after.map_or(0, |a| a.slide)];
                if !contents.contains(&slide_content(slide)) {
                    return Err(broken("an added slide is missing".to_owned()));
                }
            }
            (Unit::Slide, ChangeKind::Delete) => {
                if let Some(base) = change.before
                    && let Some(o) = plan.ours_diff.after_of(base.slide)
                    && result
                        .slides
                        .iter()
                        .any(|s| s.sld_id == ours.slides[o].sld_id)
                {
                    return Err(broken("a removed slide is still there".to_owned()));
                }
            }
            (Unit::Shape, ChangeKind::Insert | ChangeKind::Modify | ChangeKind::Format) => {
                let after = change.after.ok_or_else(|| broken("no shape".to_owned()))?;
                let wanted = &theirs.slides[after.slide].shapes[after.shape.unwrap_or(0)];
                let found = result
                    .slides
                    .iter()
                    .any(|s| s.shapes.iter().any(|shape| shape.full == wanted.full));
                if !found {
                    return Err(broken("a brought-in shape is missing".to_owned()));
                }
            }
            _ => {}
        }
    }
    // The rest of the deck (the design, every untouched part) is as it was.
    if result.design != ours.design {
        return Err(broken("the design changed".to_owned()));
    }
    Ok(())
}

/// What PowerPoint holds after Lineage changed its slides is the deck Lineage
/// meant it to hold (decision 0019): the same slides with the same ids in the
/// same order, the same sections, and no change between them by the engine's
/// keys, which leave out what PowerPoint renews on its own (decision 0017).
///
/// # Errors
/// [`Error::NotLanded`] naming the first difference (never text).
pub fn landed(expected: &Deck, held: &Deck) -> Result<()> {
    let not = |detail: String| Error::NotLanded(detail);
    let names = |deck: &Deck| -> Vec<(u32, Option<u32>)> {
        deck.slides
            .iter()
            .map(|s| (s.sld_id, s.creation_id))
            .collect()
    };
    let (want, have) = (names(expected), names(held));
    if want != have {
        return Err(not(format!("slides {have:?}, expected {want:?}")));
    }
    if expected.sections != held.sections {
        let list = |deck: &Deck| -> Vec<(usize, Vec<u32>)> {
            deck.sections
                .iter()
                .map(|s| (s.name.chars().count(), s.slides.clone()))
                .collect()
        };
        return Err(not(format!(
            "sections {:?}, expected {:?}",
            list(held),
            list(expected)
        )));
    }
    let diff = super::diff::diff(expected, held);
    if let Some(change) = diff.changes.first() {
        return Err(not(format!(
            "{} change(s), the first a {:?} of the {} on slide {}",
            diff.changes.len(),
            change.kind,
            change.unit.as_str(),
            change.slide_number
        )));
    }
    Ok(())
}

/// Untouched parts are byte-identical to ours'.
#[must_use]
pub fn untouched_parts_kept(ours: &Package, result: &Package, touched: &BTreeSet<String>) -> bool {
    ours.part_names()
        .filter(|n| !touched.contains(&n.to_ascii_lowercase()))
        .filter(|n| result.contains(n))
        .all(|n| ours.part(n) == result.part(n))
}
