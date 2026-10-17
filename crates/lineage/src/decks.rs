// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The pitch deck panel (BLUEPRINT §6, screen 4) and its "Update N versions"
//! button (decision 0018).
//!
//! PowerPoint can't open a file from an add-in, and has no tracked changes, so
//! a deck's tailored versions are updated from the master deck's panel: the
//! engine writes each chosen version's file (the host replaces it in storage,
//! checking it hasn't changed since it was read), saves a checkpoint per
//! version, and reports what happened to each. Anything both sides changed is
//! left for the person to choose, version by version; nothing is merged
//! silently.

use lineage_core::merge::Decision;
use lineage_core::view::{ChangeCard, WhatChanged};

use crate::blob::{Blob, BlobSource, Layered, restore};
use crate::changes::variant_status;
use crate::commands::{
    Context, Outcome, check_file, decide_cards, offer_to_versions, same_content, save_checkpoint,
    updates, what_changed,
};
use crate::error::{Error, Result};
use crate::ids::Id;
use crate::state::FamilyState;
use crate::views::{MasterPanel, master_panel};

/// What happened to one version.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunResult {
    /// Changes were brought in (some may still wait for a choice).
    Updated,
    /// It already had everything.
    UpToDate,
    /// Nothing was brought in: every change waits for the person's choice.
    NeedsChoice,
    /// It couldn't be updated; `text` says why.
    NotUpdated,
}

/// One version's line in the report after "Update N versions".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionRun {
    pub node: Id,
    pub name: String,
    pub result: RunResult,
    /// "Brought in 2 changes · 1 to choose", "Already up to date", …
    pub text: String,
    pub brought_in: usize,
    /// The person had edited the file outside Lineage, so those edits were
    /// saved as a checkpoint first.
    pub saved_edits: bool,
    /// What is still to decide: overlaps (with both sides) and changes to copy
    /// in by hand.
    pub left: WhatChanged,
    /// The version's new bytes, for the host to write over its file. `None`
    /// when the file is unchanged.
    pub file: Option<Vec<u8>>,
}

/// The result of "Update N versions": the records and blobs for the ledger
/// (one append for the whole run), and a line per version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateRun {
    pub outcome: Outcome,
    pub versions: Vec<VersionRun>,
    /// "Updated 2 of 3 versions".
    pub headline: String,
}

fn fold(state: &mut FamilyState, out: &mut Outcome, more: Outcome) -> Result<()> {
    for record in &more.records {
        state.apply(record)?;
    }
    out.records.extend(more.records);
    for blob in more.blobs {
        if !out.blobs.iter().any(|b: &Blob| b.hash == blob.hash) {
            out.blobs.push(blob);
        }
    }
    Ok(())
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// "Update N versions": offers the master's changes to each version in
/// `inputs` (its node and the bytes of its file as stored), and brings in
/// every change that only the master made. `choices` holds the person's
/// decisions on cards a previous run left (keep mine, use the master's).
///
/// # Errors
/// Only when the history itself can't be read; a version that can't be
/// updated is reported in its line, and the others still are.
pub fn update_versions(
    ctx: &mut Context<'_>,
    state: &FamilyState,
    inputs: &[(Id, Vec<u8>)],
    choices: &[(Id, String, Decision)],
    blobs: &dyn BlobSource,
) -> Result<UpdateRun> {
    let mut state = state.clone();
    let mut out = Outcome::default();
    let nodes: Vec<Id> = inputs.iter().map(|(n, _)| n.clone()).collect();
    let offered = {
        let layered = Layered {
            fresh: &out.blobs,
            stored: blobs,
        };
        offer_to_versions(ctx, &state, &nodes, &layered)?
    };
    fold(&mut state, &mut out, offered)?;

    let mut versions = Vec::new();
    for (node, bytes) in inputs {
        let name = state
            .node(node)
            .map_or_else(String::new, |n| n.display_name.clone());
        let mut run = VersionRun {
            node: node.clone(),
            name,
            result: RunResult::UpToDate,
            text: String::new(),
            brought_in: 0,
            saved_edits: false,
            left: WhatChanged::default(),
            file: None,
        };
        let mut current = bytes.clone();
        let result = update_one(
            ctx,
            &mut state,
            &mut out,
            node,
            &mut current,
            choices,
            blobs,
            &mut run,
        );
        if current != *bytes {
            run.file = Some(current);
        }
        if let Err(e) = result {
            // History that can't be read stops the whole run: the host fetches
            // missing blobs and runs it again.
            if matches!(e, Error::MissingBlob(_) | Error::Ledger(_)) {
                return Err(e);
            }
            run.result = RunResult::NotUpdated;
            run.text = format!("Not updated: {}", reason(&e));
        }
        versions.push(run);
    }
    let updated = versions
        .iter()
        .filter(|v| v.result == RunResult::Updated)
        .count();
    let headline = match (updated, versions.len()) {
        (0, _) if versions.iter().all(|v| v.result == RunResult::UpToDate) => {
            "Every version already has these changes".to_owned()
        }
        (0, _) => "No version was updated".to_owned(),
        (u, n) if u == n => match n {
            1 => "Updated 1 version".to_owned(),
            n => format!("Updated all {n} versions"),
        },
        (u, n) => format!("Updated {u} of {n} versions"),
    };
    Ok(UpdateRun {
        outcome: out,
        versions,
        headline,
    })
}

fn reason(e: &Error) -> String {
    let message = e.user_message();
    let mut chars = message.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(chars).collect()
    })
}

#[allow(clippy::too_many_arguments)]
fn update_one(
    ctx: &mut Context<'_>,
    state: &mut FamilyState,
    out: &mut Outcome,
    node: &Id,
    current: &mut Vec<u8>,
    choices: &[(Id, String, Decision)],
    blobs: &dyn BlobSource,
    run: &mut VersionRun,
) -> Result<()> {
    check_file(state, node, current)?;
    // Edits made in the file outside Lineage are saved first, so the update
    // never loses them and the history shows them.
    let latest = state
        .latest(node)
        .ok_or_else(|| Error::Ledger("a version without checkpoints".to_owned()))?
        .clone();
    let latest_bytes = {
        let layered = Layered {
            fresh: &out.blobs,
            stored: blobs,
        };
        restore(&latest.snapshot, &layered)?
    };
    if !same_content(current, &latest_bytes)? {
        let saved = {
            let layered = Layered {
                fresh: &out.blobs,
                stored: blobs,
            };
            save_checkpoint(
                ctx,
                state,
                node,
                current,
                "Your edits before the update",
                &layered,
            )?
        };
        if let Some(file) = saved.file.clone() {
            *current = file;
        }
        fold(state, out, saved)?;
        run.saved_edits = true;
    }

    let panel = {
        let layered = Layered {
            fresh: &out.blobs,
            stored: blobs,
        };
        updates(state, node, current, &layered)?
    };
    let mut cards: Vec<(String, Decision)> = Vec::new();
    let mut left = WhatChanged::default();
    for group in &panel.groups {
        for card in group.view.changes.iter().chain(&group.view.formatting) {
            let declined = choices
                .iter()
                .any(|(n, c, d)| n == node && *c == card.id && *d == Decision::Decline);
            cards.push((
                card.id.clone(),
                if declined {
                    Decision::Decline
                } else {
                    Decision::BringIn
                },
            ));
        }
        for overlap in &group.view.overlaps {
            match choices
                .iter()
                .find(|(n, c, _)| n == node && *c == overlap.id)
            {
                Some((_, _, decision)) => cards.push((overlap.id.clone(), *decision)),
                None => left.overlaps.push(overlap.clone()),
            }
        }
        left.by_hand.extend(group.view.by_hand.iter().cloned());
    }
    let to_choose = left.overlaps.len();
    let by_hand = left.by_hand.len();
    let brought: usize = cards.iter().filter(|(_, d)| d.brings_in()).count();
    run.left = left;
    if cards.is_empty() {
        run.result = if to_choose > 0 {
            RunResult::NeedsChoice
        } else if by_hand > 0 {
            RunResult::NotUpdated
        } else {
            RunResult::UpToDate
        };
        run.text = match run.result {
            RunResult::NeedsChoice => format!(
                "You changed the same thing here: {} to choose",
                plural(to_choose, "change", "changes")
            ),
            RunResult::NotUpdated => format!(
                "{} to copy in by hand",
                plural(by_hand, "change", "changes")
            ),
            _ => "Already up to date".to_owned(),
        };
        return Ok(());
    }
    let decided = {
        let layered = Layered {
            fresh: &out.blobs,
            stored: blobs,
        };
        decide_cards(ctx, state, node, current, &cards, &layered)?
    };
    if let Some(file) = decided.file.clone() {
        *current = file;
    }
    fold(state, out, decided)?;
    run.brought_in = brought;
    run.result = if brought > 0 {
        RunResult::Updated
    } else {
        RunResult::UpToDate
    };
    let mut text = if brought > 0 {
        format!("Brought in {}", plural(brought, "change", "changes"))
    } else {
        "Nothing brought in".to_owned()
    };
    if to_choose > 0 {
        text.push_str(&format!(" · {} to choose", to_choose));
    }
    if by_hand > 0 {
        text.push_str(&format!(" · {} to copy in by hand", by_hand));
    }
    run.text = text;
    Ok(())
}

/// "You just changed Slide 6 · Traction", with the changes marked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JustChanged {
    pub headline: String,
    pub cards: Vec<ChangeCard>,
    /// True when the changes are in the open deck and not saved yet: the
    /// update saves a checkpoint first.
    pub unsaved: bool,
}

/// A tailored version the update can go to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub id: Id,
    pub name: String,
    /// Ticked when it is missing master changes.
    pub checked: bool,
    /// "Last sent 1 Oct, before this change", "Up to date".
    pub context: String,
}

/// The most recent copy sent from the family.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LastSent {
    pub sent: Id,
    /// "Sent to Pellow Ventures on 1 Oct".
    pub text: String,
    /// "Open what they saw".
    pub button: String,
}

/// The pitch deck panel of a master deck.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeckPanel {
    pub master: MasterPanel,
    pub just_changed: Option<JustChanged>,
    pub targets: Vec<Target>,
    /// "Update 3 versions", when there is something to update.
    pub button: Option<String>,
    pub last_sent: Option<LastSent>,
}

fn changed_headline(cards: &[ChangeCard], unsaved: bool, label: &str) -> String {
    let mut places: Vec<&str> = Vec::new();
    for card in cards {
        if let Some(place) = card.place.as_deref()
            && !places.contains(&place)
        {
            places.push(place);
        }
    }
    let what = match places.as_slice() {
        [one] => (*one).to_owned(),
        [] => "the deck".to_owned(),
        many => format!("{} slides", many.len()),
    };
    if unsaved {
        format!("You just changed {what}")
    } else {
        format!("{what} changed in {label}")
    }
}

/// The panel of a master deck. `open` is the open deck's bytes, to show what
/// changed since its last checkpoint.
///
/// # Errors
/// When the family isn't a deck, the file isn't its master, or blobs are missing.
pub fn deck_panel(
    state: &FamilyState,
    open: Option<&[u8]>,
    blobs: &dyn BlobSource,
) -> Result<DeckPanel> {
    if state.format != "pptx" {
        return Err(Error::NotAllowed("not a deck".to_owned()));
    }
    let mut master = master_panel(state, blobs)?;
    let master_id = state.master()?.id.clone();
    master.chip = "Master deck".to_owned();
    master.follows = master.follows.replace("this file", "this deck");
    // The callout and its offer button are replaced by the update below.
    master.callout = None;

    let mut just_changed = None;
    if let Some(bytes) = open {
        let view = what_changed(state, &master_id, bytes, blobs)?;
        let cards: Vec<ChangeCard> = view.changes.into_iter().chain(view.formatting).collect();
        if !cards.is_empty() {
            just_changed = Some(JustChanged {
                headline: changed_headline(&cards, true, ""),
                cards,
                unsaved: true,
            });
        }
    }

    let mut targets = Vec::new();
    let mut waiting_any = false;
    for node in state.variants().filter(|n| n.follows_master) {
        let status = variant_status(state, &node.id, blobs)?;
        let missing = !status.offerable.is_empty();
        waiting_any |= missing;
        let checked = missing || just_changed.is_some();
        let context = if !checked {
            "Up to date".to_owned()
        } else if let Some(sent) = state.sent_from(&node.id).last() {
            format!("Last sent {}, before this change", sent.at.day_label())
        } else {
            format!("Has {}", state.label(&status.base))
        };
        targets.push(Target {
            id: node.id.clone(),
            name: node.display_name.clone(),
            checked,
            context,
        });
    }
    if just_changed.is_none() && waiting_any {
        // Saved, but not yet in every version: what the latest master
        // checkpoint changed.
        if let Some(latest) = state.latest(&master_id)
            && let Some(parent) = latest.parents.first()
        {
            let view = crate::commands::compare(state, parent, &latest.id, blobs)?;
            let cards: Vec<ChangeCard> = view.changes.into_iter().chain(view.formatting).collect();
            if !cards.is_empty() {
                just_changed = Some(JustChanged {
                    headline: changed_headline(&cards, false, &state.label(&latest.id)),
                    cards,
                    unsaved: false,
                });
            }
        }
    }
    let checked = targets.iter().filter(|t| t.checked).count();
    let button = (checked > 0).then(|| match checked {
        1 => "Update 1 version".to_owned(),
        n => format!("Update {n} versions"),
    });
    let last_sent = state.sent.iter().max_by_key(|s| s.at).map(|s| LastSent {
        sent: s.id.clone(),
        text: format!("Sent to {} on {}", s.recipient, s.at.day_label()),
        button: "Open what they saw".to_owned(),
    });
    Ok(DeckPanel {
        master,
        just_changed,
        targets,
        button,
        last_sent,
    })
}

#[cfg(test)]
mod tests;
