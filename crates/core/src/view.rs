// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The "what changed" view model: what the panel draws, already worded.
//!
//! Content changes, formatting-only changes, overlaps (with both sides) and
//! changes to bring in by hand are separate lists. Removed and added text is
//! marked in segments. Every label uses the glossary in CLAUDE.md.

use crate::diff::{ByHand, Change, DocDiff, Segment, SegmentKind};
use crate::docx::Docx;
use crate::merge::{Decision, Item, OverlapKind, Plan, Status};
use crate::summary::{quote, summarize};

/// A button for one decision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Choice {
    pub decision: Decision,
    pub label: String,
}

/// One change, ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChangeCard {
    pub id: String,
    pub summary: String,
    /// Where it is, such as "Under 'Experience'".
    pub place: Option<String>,
    /// The paragraph with removed and added text marked.
    pub text: Vec<Segment>,
    pub choices: Vec<Choice>,
    /// Why it must be brought in by hand, if it must.
    pub by_hand: Option<String>,
}

/// Both sides of an overlap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverlapCard {
    pub id: String,
    pub summary: String,
    pub place: Option<String>,
    /// The text before either side changed it.
    pub before: Option<String>,
    /// This version's side ("Yours").
    pub mine: Vec<Segment>,
    /// The master's side.
    pub masters: Vec<Segment>,
    pub choices: Vec<Choice>,
}

/// Everything the "what changed" panel shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WhatChanged {
    pub headline: String,
    pub changes: Vec<ChangeCard>,
    /// Formatting-only changes, always listed apart from content changes.
    pub formatting: Vec<ChangeCard>,
    pub overlaps: Vec<OverlapCard>,
    pub by_hand: Vec<ChangeCard>,
    /// Changes this version already has (copied in by hand, or made the same way).
    pub already_there: Vec<ChangeCard>,
}

impl WhatChanged {
    /// Every card id, in display order.
    #[must_use]
    pub fn ids(&self) -> Vec<&str> {
        self.changes
            .iter()
            .chain(&self.formatting)
            .map(|c| c.id.as_str())
            .chain(self.overlaps.iter().map(|o| o.id.as_str()))
            .chain(self.by_hand.iter().map(|c| c.id.as_str()))
            .collect()
    }
}

/// The label for a decision on a master's change.
#[must_use]
pub fn label(decision: Decision) -> &'static str {
    Theirs::Master.label(decision)
}

/// Whose changes are on offer, for the words on the cards.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Theirs {
    /// The master's changes, offered to a tailored version.
    Master,
    /// The changes in a file that came back, made by `who` ("Northgate
    /// Analytics", or "the returned copy" when nobody was recorded).
    Returned { who: String },
}

impl Theirs {
    /// The label for a decision.
    #[must_use]
    pub fn label(&self, decision: Decision) -> &'static str {
        let returned = matches!(self, Self::Returned { .. });
        match decision {
            Decision::BringIn if returned => "Keep",
            Decision::BringIn => "Bring in",
            Decision::Decline if returned => "Skip",
            Decision::Decline => "Decline",
            Decision::KeepMine => "Keep mine",
            Decision::UseMasters if returned => "Use theirs",
            Decision::UseMasters => "Use the master's",
            Decision::KeepBoth {
                masters_first: false,
            } => "Keep both, mine first",
            Decision::KeepBoth {
                masters_first: true,
            } if returned => "Keep both, theirs first",
            Decision::KeepBoth {
                masters_first: true,
            } => "Keep both, master's first",
        }
    }

    /// Who made the changes, in the middle of a sentence.
    fn they(&self) -> &str {
        match self {
            Self::Master => "the master",
            Self::Returned { who } => who,
        }
    }

    /// Who made the changes, at the start of a sentence.
    fn who(&self) -> String {
        let they = self.they();
        let mut chars = they.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(chars).collect()
        })
    }
}

/// Why a change must be brought in by hand, in plain words.
#[must_use]
pub fn by_hand_reason(reason: ByHand) -> &'static str {
    match reason {
        ByHand::Note => "Footnotes and endnotes are brought in by hand for now.",
        ByHand::TableStructure => {
            "This changes how the table is laid out. Copy it in by hand for now."
        }
        ByHand::Section => "This changes page layout or sections. Copy it in by hand for now.",
        ByHand::Locked => {
            "This is inside a table of contents or a content control. Copy it in by hand."
        }
        ByHand::Opaque => "This includes content Lineage can't change yet. Copy it in by hand.",
        ByHand::Story => {
            "This header or footer exists in only one of the files. Copy it in by hand."
        }
        ByHand::Revisions => {
            "This part of your version has tracked changes. Accept or reject them in Word first."
        }
        ByHand::NewPart => {
            "This includes a picture or object your version doesn't have. Copy it in by hand."
        }
        ByHand::Markup => "The two files are written differently here. Copy it in by hand.",
        ByHand::Placement => {
            "Lineage can't tell where this goes in your version. Copy it in by hand."
        }
        ByHand::Object => {
            "This is SmartArt, a video or an embedded object. Copy it in by hand for now."
        }
        ByHand::Design => {
            "This changes the slide master, layouts or theme. Copy it in by hand for now."
        }
        ByHand::Animation => "This changes animations. Copy it in by hand for now.",
        ByHand::Layout => {
            "Your version doesn't have this slide's layout. Copy the slide in by hand."
        }
    }
}

fn place(change: &Change) -> Option<String> {
    change
        .heading
        .as_ref()
        .map(|h| format!("Under {}", quote(h)))
}

fn card(change: &Change, after: &Docx, choices: Vec<Choice>) -> ChangeCard {
    ChangeCard {
        id: change.id.clone(),
        summary: summarize(change, &after.styles, Some(after)),
        place: place(change),
        text: change.segments.clone(),
        choices,
        by_hand: change.by_hand.map(|r| by_hand_reason(r).to_owned()),
    }
}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// "What changed" between two checkpoints of one file, such as a master's
/// changes since its last checkpoint. No decisions: it is a view.
#[must_use]
pub fn from_diff(diff: &DocDiff, after: &Docx) -> WhatChanged {
    let mut view = WhatChanged::default();
    for change in &diff.changes {
        let card = card(change, after, Vec::new());
        if change.formatting_only {
            view.formatting.push(card);
        } else {
            view.changes.push(card);
        }
    }
    let total = view.changes.len() + view.formatting.len();
    view.headline = if total == 0 {
        "No changes".to_owned()
    } else {
        count(total, "change", "changes")
    };
    view
}

fn overlap_summary(item: &Item, theirs: &Theirs) -> String {
    let before = item.change.before_text.as_deref().map(quote);
    let before = before.unwrap_or_default();
    let (who, they) = (theirs.who(), theirs.they());
    match item.status {
        Status::Overlap(OverlapKind::BothAdded) => format!("You and {they} both added text here"),
        Status::Overlap(OverlapKind::DeletedVsEdited) => {
            format!("{who} removed {before} and you changed it")
        }
        Status::Overlap(OverlapKind::EditedVsDeleted) => {
            format!("{who} changed {before} and you removed it")
        }
        Status::Overlap(OverlapKind::FormattingVsText) => {
            format!("Both changed {before}: one the words, one the formatting")
        }
        _ if *theirs == Theirs::Master => format!("Both changed {before}"),
        _ => format!("You and {they} both changed {before}"),
    }
}

fn mine_segments(item: &Item) -> Vec<Segment> {
    match &item.mine {
        Some(mine) => mine.segments.clone(),
        None => item
            .change
            .before_text
            .clone()
            .map(|text| {
                vec![Segment {
                    kind: SegmentKind::Same,
                    text,
                }]
            })
            .unwrap_or_default(),
    }
}

/// "What changed" for an offer: the master's changes, judged against this version.
#[must_use]
pub fn from_plan(plan: &Plan<'_>) -> WhatChanged {
    from_plan_for(plan, &Theirs::Master)
}

/// "What changed" for `theirs`' changes, judged against this file: an offer
/// from the master, or the changes in a file that came back.
#[must_use]
pub fn from_plan_for(plan: &Plan<'_>, theirs: &Theirs) -> WhatChanged {
    let mut view = WhatChanged::default();
    for item in &plan.items {
        let choices: Vec<Choice> = item
            .choices()
            .into_iter()
            .map(|decision| Choice {
                decision,
                label: theirs.label(decision).to_owned(),
            })
            .collect();
        match &item.status {
            Status::Clean if item.change.formatting_only => {
                view.formatting
                    .push(card(&item.change, plan.theirs, choices));
            }
            Status::Clean => view.changes.push(card(&item.change, plan.theirs, choices)),
            Status::AlreadyThere { .. } => {
                view.already_there
                    .push(card(&item.change, plan.theirs, choices))
            }
            Status::ByHand(reason) => {
                let mut c = card(&item.change, plan.theirs, choices);
                c.by_hand = Some(by_hand_reason(*reason).to_owned());
                view.by_hand.push(c);
            }
            Status::Overlap(_) => view.overlaps.push(OverlapCard {
                id: item.change.id.clone(),
                summary: overlap_summary(item, theirs),
                place: place(&item.change),
                before: item.change.before_text.clone(),
                mine: mine_segments(item),
                masters: item.change.segments.clone(),
                choices,
            }),
        }
    }
    let offered =
        view.changes.len() + view.formatting.len() + view.overlaps.len() + view.by_hand.len();
    view.headline = match theirs {
        Theirs::Master if offered == 0 => "You're up to date with the master".to_owned(),
        Theirs::Master => format!("{} from the master", count(offered, "update", "updates")),
        Theirs::Returned { .. } if offered == 0 => "Nothing left to review".to_owned(),
        Theirs::Returned { who } => format!("{} from {who}", count(offered, "change", "changes")),
    };
    view
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::diff;
    use crate::merge;
    use crate::testutil::open_lines;
    use lineage_testkit::resume;

    #[test]
    fn formatting_is_listed_apart_from_content() {
        let (a, b) = (
            open_lines(&resume::master_v2_lines()),
            open_lines(&resume::master_v3_lines()),
        );
        let view = from_diff(&diff(&a, &b), &b);
        assert_eq!(view.headline, "2 changes");
        assert_eq!(view.changes.len(), 1);
        assert_eq!(view.formatting.len(), 1);
        assert_eq!(view.formatting[0].summary, "'Avery Quinn' made bold");
        let removed_added: Vec<SegmentKind> = view.changes[0].text.iter().map(|s| s.kind).collect();
        assert!(
            removed_added.contains(&SegmentKind::Removed)
                && removed_added.contains(&SegmentKind::Added)
        );
        assert_eq!(view.changes[0].place.as_deref(), Some("Under 'Experience'"));
    }

    #[test]
    fn an_offer_shows_both_sides_of_an_overlap() {
        let base = open_lines(&resume::master_v1_lines());
        let ours = open_lines(&resume::tailored_same_para_lines());
        let theirs = open_lines(&resume::master_v3_lines());
        let plan = merge::plan(&base, &ours, &theirs);
        let view = from_plan(&plan);
        assert_eq!(view.overlaps.len(), 1);
        let overlap = &view.overlaps[0];
        assert!(
            overlap
                .summary
                .starts_with("Both changed 'Data engineer, Corvia"),
            "{}",
            overlap.summary
        );
        assert!(
            overlap
                .mine
                .iter()
                .any(|s| s.kind == SegmentKind::Added && s.text.contains("(logistics)"))
        );
        assert!(
            overlap
                .masters
                .iter()
                .any(|s| s.kind == SegmentKind::Added && s.text.contains("Lead"))
        );
        let labels: Vec<&str> = overlap.choices.iter().map(|c| c.label.as_str()).collect();
        assert_eq!(labels, ["Keep mine", "Use the master's"]);
        assert_eq!(view.headline, "4 updates from the master");
        let clean: Vec<&str> = view.changes[0]
            .choices
            .iter()
            .map(|c| c.label.as_str())
            .collect();
        assert_eq!(clean, ["Bring in", "Decline"]);
    }

    #[test]
    fn a_change_brought_in_by_hand_can_still_be_declined() {
        // Found in Phase 2: by-hand cards had no buttons, so a person who
        // didn't want the change could never stop it being offered.
        let base = open_lines(&resume::master_v1_lines());
        let mut lines = resume::master_v1_lines();
        lines.push(resume::Line::Raw(lineage_testkit::docx::para(
            "<w:sectPr/>",
            &lineage_testkit::docx::run("References are available on request."),
        )));
        let theirs = open_lines(&lines);
        let view = from_plan(&merge::plan(&base, &base, &theirs));
        assert!(!view.by_hand.is_empty());
        for card in &view.by_hand {
            let labels: Vec<&str> = card.choices.iter().map(|c| c.label.as_str()).collect();
            assert_eq!(labels, ["Decline"], "{}", card.summary);
            assert!(card.by_hand.is_some());
        }
    }

    #[test]
    fn a_returned_file_is_worded_as_theirs() {
        let base = open_lines(&resume::master_v1_lines());
        let ours = open_lines(&resume::tailored_same_para_lines());
        let theirs = open_lines(&resume::master_v3_lines());
        let plan = merge::plan(&base, &ours, &theirs);
        let who = Theirs::Returned {
            who: "Northgate Analytics".to_owned(),
        };
        let view = from_plan_for(&plan, &who);
        assert_eq!(view.headline, "4 changes from Northgate Analytics");
        assert!(
            view.overlaps[0]
                .summary
                .starts_with("You and Northgate Analytics both changed 'Data engineer"),
            "{}",
            view.overlaps[0].summary
        );
        let labels: Vec<&str> = view.overlaps[0]
            .choices
            .iter()
            .map(|c| c.label.as_str())
            .collect();
        assert_eq!(labels, ["Keep mine", "Use theirs"]);
        let clean: Vec<&str> = view.changes[0]
            .choices
            .iter()
            .map(|c| c.label.as_str())
            .collect();
        assert_eq!(clean, ["Keep", "Skip"]);
        // The master's wording is unchanged.
        assert_eq!(from_plan(&plan).headline, "4 updates from the master");

        let deleted = open_lines(&resume::tailored_deleted_job_lines());
        let plan = merge::plan(&base, &deleted, &theirs);
        let unnamed = Theirs::Returned {
            who: "the returned copy".to_owned(),
        };
        let view = from_plan_for(&plan, &unnamed);
        assert!(
            view.overlaps
                .iter()
                .any(|o| o.summary.starts_with("The returned copy changed")),
            "{:?}",
            view.overlaps.iter().map(|o| &o.summary).collect::<Vec<_>>()
        );
        assert_eq!(
            unnamed.label(Decision::KeepBoth {
                masters_first: true
            }),
            "Keep both, theirs first"
        );
    }

    #[test]
    fn already_there_changes_are_not_offered() {
        let base = open_lines(&resume::master_v1_lines());
        let ours = open_lines(&resume::tailored_copied_job_lines());
        let theirs = open_lines(&resume::master_v2_lines());
        let view = from_plan(&merge::plan(&base, &ours, &theirs));
        assert_eq!(view.already_there.len(), 2);
        assert_eq!(view.headline, "You're up to date with the master");
    }
}
