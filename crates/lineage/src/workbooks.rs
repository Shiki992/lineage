// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Bringing the master's changes into a tailored workbook that is open in
//! Excel (decision 0023). Excel has no tracked changes and Office.js can't
//! replace an open workbook, so this works like the open deck (decision
//! 0019), in two calls:
//!
//! 1. [`prepare`] works out the apply plan from the person's choices and
//!    the workbook as Excel holds it, and records nothing;
//! 2. the pane carries the plan out and reads the workbook back;
//! 3. [`finish`] works out the same plan again, checks that what Excel holds
//!    is what the plan meant ([`lineage_core::xlsx::apply::landed`]), and
//!    records the choices and a checkpoint of the workbook as it now is.
//!
//! Updates from several master checkpoints come in one after another: each
//! plan is worked out on the workbook the previous one leaves, which the
//! Rust writer makes for that purpose. When the writer can't (a chart reads
//! a sheet that gets new rows), only the first update comes in now.

use lineage_core::Workbook;
use lineage_core::docx::props::LineageIds;
use lineage_core::merge::Decision;
use lineage_core::view::Theirs;
use lineage_core::view::WhatChanged;
use lineage_core::xlsx::apply::{ApplyPlan, Expect, Skipped, Step, apply_plan, landed, put_back};
use lineage_core::xlsx::diff::{Hint, diff};
use lineage_core::xlsx::merge::{Plan, Status, plan};
use lineage_core::xlsx::view::SideBySide;
use lineage_core::xlsx::write::apply_steps;

use crate::blob::{BlobSource, restore};
use crate::changes::{checkpoint_pair, variant_status};
use crate::commands::{
    Builder, Context, Outcome, UpdateGroup, Updates, check_file, checkpoint_into,
};
use crate::error::{Error, Result};
use crate::event::{ChangeState, Event, How};
use crate::ids::Id;
use crate::state::FamilyState;

/// The plan for the pane, with what it brings and leaves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkbookPlan {
    pub steps: Vec<Step>,
    /// Run after a failure, in order, to put the workbook back.
    pub undo: Vec<Step>,
    /// What the target cells hold now, for the pane to check first.
    pub expect: Vec<Expect>,
    pub skipped: Vec<Skipped>,
    /// Changes that come in.
    pub brought: usize,
    /// Master updates left for next time.
    pub deferred: usize,
    /// "Brings in 3 changes", "Nothing to bring in", …
    pub text: String,
}

fn open(bytes: &[u8]) -> Result<Workbook> {
    Ok(Workbook::open_for_panel(bytes)?)
}

/// The workbook a node's latest checkpoint holds, and the open one.
fn latest_and_open(
    state: &FamilyState,
    node: &Id,
    bytes: &[u8],
    blobs: &dyn BlobSource,
) -> Result<(Id, Workbook, Workbook)> {
    check_file(state, node, bytes)?;
    if state.format != "xlsx" {
        return Err(Error::NotAllowed("not a workbook family".to_owned()));
    }
    let latest = state
        .latest(node)
        .ok_or_else(|| Error::Ledger("a node without checkpoints".to_owned()))?;
    let before = open(&restore(&latest.snapshot, blobs)?)?;
    Ok((latest.id.clone(), before, open(bytes)?))
}

/// "What changed" in the open workbook since its latest checkpoint, with
/// the cells Excel reported as edited in this session (decision 0022): a
/// change inside one of `hints` is a direct edit.
///
/// # Errors
/// When the file isn't the node's, or a snapshot is missing.
pub fn what_changed(
    state: &FamilyState,
    node: &Id,
    bytes: &[u8],
    hints: &[Hint],
    blobs: &dyn BlobSource,
) -> Result<WhatChanged> {
    let (latest, a, b) = latest_and_open(state, node, bytes, blobs)?;
    let mut view = lineage_core::xlsx::view::what_changed(&diff(&a, &b, hints), &a, &b);
    view.headline = if view.changes.is_empty() && view.formatting.is_empty() {
        format!("No changes since {}", state.label(&latest))
    } else {
        format!("{} since {}", view.headline, state.label(&latest))
    };
    Ok(view)
}

/// The side by side for one cell of the open workbook (`sheet`, `address`
/// such as "B4"), beside the same cell at the latest checkpoint.
///
/// # Errors
/// [`Error::NotAllowed`] when the workbook has no such sheet or cell;
/// otherwise as [`what_changed`].
pub fn side_by_side(
    state: &FamilyState,
    node: &Id,
    bytes: &[u8],
    sheet: &str,
    address: &str,
    blobs: &dyn BlobSource,
) -> Result<SideBySide> {
    let (_, a, b) = latest_and_open(state, node, bytes, blobs)?;
    lineage_core::xlsx::view::side_by_side(&a, &b, sheet, address)
        .ok_or_else(|| Error::NotAllowed("no such cell in this workbook".to_owned()))
}

/// What goes with a clean copy of `bytes` that the person may not expect, for
/// Send a copy to say before saving (decision 0026): hidden sheets, rows and
/// columns, which formulas may read, and macros. Empty for documents and
/// decks, whose hidden content is left out.
///
/// # Errors
/// When the file can't be read.
pub fn goes_with_copy(bytes: &[u8]) -> Result<Vec<String>> {
    let (_, report) = lineage_core::doc::clean_copy(bytes, None)?;
    let mut out = Vec::new();
    for (n, one, many) in [
        (
            report.hidden_sheets,
            "hidden sheet goes",
            "hidden sheets go",
        ),
        (report.hidden_rows, "hidden row goes", "hidden rows go"),
        (
            report.hidden_columns,
            "hidden column goes",
            "hidden columns go",
        ),
    ] {
        // Each line says why: the pane's closing words fit macros too.
        match n {
            0 => {}
            1 => out.push(format!("1 {one} with this copy: formulas may read it")),
            n => out.push(format!("{n} {many} with this copy: formulas may read them")),
        }
    }
    if report.macros {
        out.push("Macros go with this copy".to_owned());
    }
    Ok(out)
}

fn pair(
    state: &FamilyState,
    checkpoint: &crate::state::Checkpoint,
    blobs: &dyn BlobSource,
) -> Result<(Workbook, Workbook)> {
    let (before, after) = checkpoint_pair(state, checkpoint, blobs)?;
    match (before, after) {
        (lineage_core::doc::Doc::Workbook(a), lineage_core::doc::Doc::Workbook(b)) => Ok((*a, *b)),
        _ => Err(Error::NotAllowed("not a workbook family".to_owned())),
    }
}

/// A tailored workbook's "updates from the master" panel: the master's
/// changes offered to it, judged against the workbook as Excel holds it.
///
/// # Errors
/// When the file is not the version's, or snapshots are missing.
pub fn updates(
    state: &FamilyState,
    version: &Id,
    bytes: &[u8],
    blobs: &dyn BlobSource,
) -> Result<Updates> {
    check_file(state, version, bytes)?;
    let status = variant_status(state, version, blobs)?;
    let ours = open(bytes)?;
    let mut groups = Vec::new();
    let mut total = 0;
    for checkpoint in state.master_checkpoints() {
        let offered: Vec<&String> = status
            .offerable
            .iter()
            .filter(|p| p.offered && p.checkpoint == checkpoint.id)
            .map(|p| &p.change.id)
            .collect();
        if offered.is_empty() {
            continue;
        }
        let (before, after) = pair(state, checkpoint, blobs)?;
        let p = plan(&before, &ours, &after);
        let mut view = lineage_core::xlsx::view::from_plan(&p, &Theirs::Master);
        view.changes.retain(|c| offered.contains(&&c.id));
        view.overlaps.retain(|c| offered.contains(&&c.id));
        view.by_hand.retain(|c| offered.contains(&&c.id));
        view.already_there.retain(|c| offered.contains(&&c.id));
        if let Some(wv) = view.workbook.as_mut() {
            for g in &mut wv.groups {
                g.cards
                    .retain(|c| c.ids.iter().any(|id| offered.contains(&id)));
            }
            wv.groups.retain(|g| !g.cards.is_empty());
        }
        let count: usize = view.workbook.as_ref().map_or(0, |wv| {
            wv.groups
                .iter()
                .map(|g| g.cards.iter().filter(|c| !c.choices.is_empty()).count())
                .sum()
        });
        if count == 0 && view.already_there.is_empty() {
            continue;
        }
        total += count;
        groups.push(UpdateGroup {
            checkpoint: checkpoint.id.clone(),
            label: format!("From master {}", state.label(&checkpoint.id)),
            view,
        });
    }
    let headline = match total {
        0 => "You're up to date with the master".to_owned(),
        1 => "1 update from the master".to_owned(),
        n => format!("{n} updates from the master"),
    };
    Ok(Updates { headline, groups })
}

/// The decisions, by master checkpoint: each item id with what was chosen.
type ByCheckpoint = Vec<(Id, Vec<(String, Decision)>)>;

/// Finds the master checkpoint of each decided item.
fn resolve(
    state: &FamilyState,
    version: &Id,
    decisions: &[(String, Decision)],
    blobs: &dyn BlobSource,
) -> Result<ByCheckpoint> {
    let status = variant_status(state, version, blobs)?;
    let mut out: ByCheckpoint = Vec::new();
    for (id, decision) in decisions {
        let pending = status
            .offerable
            .iter()
            .chain(&status.quiet)
            .find(|p| &p.change.id == id)
            .ok_or_else(|| {
                Error::NotAllowed(format!("change {id} is not waiting for a decision"))
            })?;
        match out.iter_mut().find(|(c, _)| *c == pending.checkpoint) {
            Some((_, list)) => list.push((id.clone(), *decision)),
            None => out.push((pending.checkpoint.clone(), vec![(id.clone(), *decision)])),
        }
    }
    // In the master's order.
    let order: Vec<Id> = state
        .master_checkpoints()
        .iter()
        .map(|c| c.id.clone())
        .collect();
    out.sort_by_key(|(c, _)| order.iter().position(|o| o == c).unwrap_or(usize::MAX));
    Ok(out)
}

/// One master checkpoint taken: its id, its decisions, and its plan.
type Stage = (Id, Vec<(String, Decision)>, ApplyPlan);

/// The chain of plans for the decisions, worked out on `bytes`.
struct Chain {
    stages: Vec<Stage>,
    /// What the workbook should hold after every stage, when there is more
    /// than one (written by the Rust writer).
    last: Option<Vec<u8>>,
    deferred: usize,
}

fn chain(
    state: &FamilyState,
    bytes: &[u8],
    decided: &ByCheckpoint,
    blobs: &dyn BlobSource,
) -> Result<Chain> {
    let mut current = bytes.to_vec();
    let mut stages = Vec::new();
    let mut deferred = 0;
    for (i, (checkpoint_id, local)) in decided.iter().enumerate() {
        let checkpoint = state
            .checkpoint(checkpoint_id)
            .ok_or_else(|| Error::Ledger("an unknown master checkpoint".to_owned()))?;
        let (before, after) = pair(state, checkpoint, blobs)?;
        let ours = open(&current)?;
        let p: Plan<'_> = plan(&before, &ours, &after);
        // Already there: nothing to write, and nothing to record.
        let local: Vec<(String, Decision)> = local
            .iter()
            .filter(|(id, _)| {
                p.item(id)
                    .is_some_and(|it| it.status != Status::AlreadyThere)
            })
            .cloned()
            .collect();
        let ap = apply_plan(&p, &local)?;
        // With more than one update, each is worked out on what the one
        // before leaves, so every stage is written here too.
        if decided.len() > 1 {
            match apply_steps(&current, &ap.steps) {
                Ok(next) => current = next,
                Err(_) => {
                    // Only the first can come in on its own; the rest wait.
                    if i == 0 {
                        stages.push((checkpoint_id.clone(), local, ap));
                    }
                    deferred = decided.len() - stages.len();
                    break;
                }
            }
        }
        stages.push((checkpoint_id.clone(), local, ap));
    }
    let last = (stages.len() > 1).then_some(current);
    Ok(Chain {
        stages,
        last,
        deferred,
    })
}

/// The apply plan for the person's choices (item id → decision), worked
/// out on the workbook as Excel holds it. Records nothing.
///
/// # Errors
/// When the file is not the version's, a decision is for a change that
/// isn't waiting, or a snapshot is missing.
pub fn prepare(
    state: &FamilyState,
    version: &Id,
    bytes: &[u8],
    decisions: &[(String, Decision)],
    blobs: &dyn BlobSource,
) -> Result<WorkbookPlan> {
    check_file(state, version, bytes)?;
    let decided = resolve(state, version, decisions, blobs)?;
    let c = chain(state, bytes, &decided, blobs)?;
    let mut out = WorkbookPlan {
        deferred: c.deferred,
        ..WorkbookPlan::default()
    };
    for (i, (_, _, ap)) in c.stages.iter().enumerate() {
        out.steps.extend(ap.steps.iter().cloned());
        if i == 0 {
            out.expect.extend(ap.expect.iter().cloned());
        }
        out.skipped.extend(ap.skipped.iter().cloned());
        out.brought += ap.brought.len();
    }
    let plans: Vec<&ApplyPlan> = c.stages.iter().map(|(_, _, ap)| ap).collect();
    out.undo = undo_after(&plans, out.steps.len());
    out.text = match (out.brought, out.deferred) {
        (0, _) => "Nothing to bring in".to_owned(),
        (1, 0) => "Brings in 1 change".to_owned(),
        (n, 0) => format!("Brings in {n} changes"),
        (n, d) => format!("Brings in {n} changes now; {d} more updates after this one"),
    };
    Ok(out)
}

/// The way back after the pane carried out only the first `done` steps of a
/// plan made of `plans` in turn (Excel stopped partway). Each plan inserts
/// its rows first, bottom-up, then sets cells. A plan stopped among its
/// inserts takes out the rows it inserted, last first; a plan stopped among
/// its cells, or finished, runs its whole way back (setting a cell back to
/// what it held is harmless when it was never changed). Later plans' ways
/// back run first.
fn undo_after(plans: &[&ApplyPlan], done: usize) -> Vec<Step> {
    let mut left = done;
    let mut finished = Vec::new();
    let mut partial = Vec::new();
    for ap in plans {
        if left == 0 {
            break;
        }
        if left >= ap.steps.len() {
            finished.push(*ap);
            left -= ap.steps.len();
            continue;
        }
        let inserts = ap
            .steps
            .iter()
            .take_while(|s| matches!(s, Step::InsertRows { .. }))
            .count();
        if left <= inserts {
            for step in ap.steps[..left].iter().rev() {
                if let Step::InsertRows { sheet, at, count } = step {
                    partial.push(Step::DeleteRows {
                        sheet: sheet.clone(),
                        at: *at,
                        count: *count,
                    });
                }
            }
        } else {
            partial.extend(ap.undo.iter().cloned());
        }
        break;
    }
    for ap in finished.iter().rev() {
        partial.extend(ap.undo.iter().cloned());
    }
    partial
}

/// The way back for the open workbook after Excel carried out only the first
/// `done` steps of the plan [`prepare`] gave for `bytes` and `decisions`.
///
/// # Errors
/// As [`prepare`].
pub fn way_back(
    state: &FamilyState,
    version: &Id,
    bytes: &[u8],
    decisions: &[(String, Decision)],
    done: usize,
    blobs: &dyn BlobSource,
) -> Result<Vec<Step>> {
    check_file(state, version, bytes)?;
    let decided = resolve(state, version, decisions, blobs)?;
    let c = chain(state, bytes, &decided, blobs)?;
    let plans: Vec<&ApplyPlan> = c.stages.iter().map(|(_, _, ap)| ap).collect();
    Ok(undo_after(&plans, done))
}

/// Records what the pane brought in: checks that `held` (what Excel holds
/// after carrying out the plan [`prepare`] gave for `original`) is what the
/// plan meant, then records every decision and a checkpoint of `held`.
///
/// # Errors
/// [`lineage_core::Error::NotLanded`] when `held` isn't what the plan meant
/// (nothing is recorded, and the pane puts the workbook back); otherwise as
/// [`prepare`].
pub fn finish(
    ctx: &mut Context<'_>,
    state: &FamilyState,
    version: &Id,
    original: &[u8],
    held: &[u8],
    decisions: &[(String, Decision)],
    blobs: &dyn BlobSource,
) -> Result<Outcome> {
    check_file(state, version, original)?;
    let decided = resolve(state, version, decisions, blobs)?;
    let c = chain(state, original, &decided, blobs)?;
    let actual = open(held)?;
    let ours = open(original)?;
    let brought: usize = c.stages.iter().map(|(_, _, ap)| ap.brought.len()).sum();
    if brought > 0 {
        match &c.last {
            Some(bytes) => put_back(&open(bytes)?, &actual)?,
            None => landed(&ours, &c.stages[0].2, &actual)?,
        }
    } else {
        // Keep mine and Not now only: nothing may have changed.
        put_back(&ours, &actual)?;
    }

    let mut builder = Builder::new(state, ctx.now, blobs);
    let mut merged = Vec::new();
    let mut declined = Vec::new();
    let mut brought_from: Vec<Id> = Vec::new();
    for (checkpoint, local, ap) in &c.stages {
        for (id, decision) in local {
            // A change brought in that couldn't be written stays waiting.
            if ap.skipped.iter().any(|s| &s.id == id) {
                continue;
            }
            let into = decision.brings_in();
            if into {
                merged.push(id.clone());
                if !brought_from.contains(checkpoint) {
                    brought_from.push(checkpoint.clone());
                }
            } else {
                declined.push(id.clone());
            }
            builder.push(Event::ChangeStateRecorded {
                node_id: version.clone(),
                change_id: id.clone(),
                from_checkpoint_id: checkpoint.clone(),
                state: if into {
                    ChangeState::Merged
                } else {
                    ChangeState::Declined
                },
                how: How::Clicked,
            })?;
        }
    }
    let mut result = None;
    if brought > 0 {
        let latest = state
            .latest(version)
            .ok_or_else(|| Error::Ledger("a version without checkpoints".to_owned()))?;
        let mut parents = vec![latest.id.clone()];
        if let Some(newest) = brought_from.last() {
            parents.push(newest.clone());
        }
        let from = match brought_from.as_slice() {
            [one] => format!("master {}", state.label(one)),
            _ => "the master".to_owned(),
        };
        let note = match merged.len() {
            1 => format!("Brought in 1 change from {from}"),
            n => format!("Brought in {n} changes from {from}"),
        };
        let (checkpoint, file) = checkpoint_into(
            &mut builder,
            ctx.ids,
            ctx.author.clone(),
            version,
            held,
            parents,
            note,
        )?;
        builder.out.file = Some(file);
        builder.out.created = Some(checkpoint.clone());
        builder.out.ids = Some(LineageIds {
            family: state.family_id.to_string(),
            checkpoint: checkpoint.to_string(),
        });
        result = Some(checkpoint);
    }
    builder.push(Event::UpdateApplied {
        node_id: version.clone(),
        result_checkpoint_id: result,
        merged,
        declined,
    })?;
    let _ = restore;
    Ok(builder.finish())
}

/// The three workbooks of a returned file's review: what left, the open
/// workbook, and what came back.
fn returned_three(
    state: &FamilyState,
    returned: &Id,
    node: &Id,
    bytes: &[u8],
    blobs: &dyn BlobSource,
) -> Result<(Workbook, Workbook, Workbook)> {
    check_file(state, node, bytes)?;
    let r = state
        .returned_file(returned)
        .ok_or_else(|| Error::NotAllowed("unknown returned file".to_owned()))?;
    let base = crate::returned::base_bytes(state, &r.from, r.sent.as_ref(), blobs)?;
    let theirs = crate::returned::whole_blob(&r.file, blobs)?;
    Ok((open(&base)?, open(bytes)?, open(&theirs)?))
}

/// The changes in a workbook that came back, judged against the open
/// workbook, with "Keep" and "Skip" on each.
///
/// # Errors
/// When the file is not `node`'s, or blobs are missing.
pub fn returned_view(
    state: &FamilyState,
    returned: &Id,
    node: &Id,
    bytes: &[u8],
    blobs: &dyn BlobSource,
) -> Result<lineage_core::view::WhatChanged> {
    let (base, ours, theirs) = returned_three(state, returned, node, bytes, blobs)?;
    let who = crate::returned::who(state, returned);
    let p = plan(&base, &ours, &theirs);
    Ok(lineage_core::xlsx::view::from_plan(
        &p,
        &Theirs::Returned { who },
    ))
}

fn keep_plan(
    state: &FamilyState,
    returned: &Id,
    node: &Id,
    bytes: &[u8],
    cards: &[(String, Decision)],
    blobs: &dyn BlobSource,
) -> Result<(Workbook, ApplyPlan)> {
    let (base, ours, theirs) = returned_three(state, returned, node, bytes, blobs)?;
    let r = state
        .returned_file(returned)
        .ok_or_else(|| Error::NotAllowed("unknown returned file".to_owned()))?;
    let decided: Vec<&String> = r.decided().collect();
    if cards.iter().any(|(id, _)| decided.contains(&id)) {
        return Err(Error::NotAllowed(
            "that change was decided already".to_owned(),
        ));
    }
    let p = plan(&base, &ours, &theirs);
    let ap = apply_plan(&p, cards).map_err(|e| Error::NotAllowed(e.to_string()))?;
    drop(p);
    Ok((ours, ap))
}

/// The apply plan for keeping changes from a workbook that came back.
/// Records nothing.
///
/// # Errors
/// When a card isn't waiting, or its decision isn't one of its choices.
pub fn prepare_keep(
    state: &FamilyState,
    returned: &Id,
    node: &Id,
    bytes: &[u8],
    cards: &[(String, Decision)],
    blobs: &dyn BlobSource,
) -> Result<WorkbookPlan> {
    let (_, ap) = keep_plan(state, returned, node, bytes, cards, blobs)?;
    let brought = ap.brought.len();
    Ok(WorkbookPlan {
        text: match brought {
            0 => "Nothing to keep".to_owned(),
            n => format!("Keeps {}", crate::returned::changes(n)),
        },
        brought,
        undo: undo_after(&[&ap], ap.steps.len()),
        steps: ap.steps.clone(),
        expect: ap.expect,
        skipped: ap.skipped,
        deferred: 0,
    })
}

/// The way back after Excel carried out only the first `done` steps of the
/// plan [`prepare_keep`] gave.
///
/// # Errors
/// As [`prepare_keep`].
pub fn keep_way_back(
    state: &FamilyState,
    returned: &Id,
    node: &Id,
    bytes: &[u8],
    cards: &[(String, Decision)],
    done: usize,
    blobs: &dyn BlobSource,
) -> Result<Vec<Step>> {
    let (_, ap) = keep_plan(state, returned, node, bytes, cards, blobs)?;
    Ok(undo_after(&[&ap], done))
}

/// Records what the pane kept from a workbook that came back, after
/// checking `held` is what the plan meant.
///
/// # Errors
/// [`lineage_core::Error::NotLanded`] when it isn't (nothing is recorded).
#[allow(clippy::too_many_arguments)]
pub fn finish_keep(
    ctx: &mut Context<'_>,
    state: &FamilyState,
    returned: &Id,
    node: &Id,
    original: &[u8],
    held: &[u8],
    cards: &[(String, Decision)],
    blobs: &dyn BlobSource,
) -> Result<Outcome> {
    let (ours, ap) = keep_plan(state, returned, node, original, cards, blobs)?;
    let actual = open(held)?;
    if ap.brought.is_empty() {
        put_back(&ours, &actual)?;
    } else {
        landed(&ours, &ap, &actual)?;
    }
    let (kept, skipped): (Vec<_>, Vec<_>) = cards
        .iter()
        .filter(|(id, _)| !ap.skipped.iter().any(|s| &s.id == id))
        .partition(|(_, d)| d.brings_in());
    let kept: Vec<String> = kept.into_iter().map(|(c, _)| c.clone()).collect();
    let skipped: Vec<String> = skipped.into_iter().map(|(c, _)| c.clone()).collect();
    let view = returned_view(state, returned, node, original, blobs)?;
    let r = state
        .returned_file(returned)
        .ok_or_else(|| Error::NotAllowed("unknown returned file".to_owned()))?;
    let decided: Vec<&String> = r.decided().collect();
    // Every id a card stands for (a filled-down formula is one card).
    let all: Vec<String> = match view.workbook.as_ref() {
        Some(wv) => wv
            .groups
            .iter()
            .flat_map(|g| {
                g.cards
                    .iter()
                    .filter(|c| !c.choices.is_empty())
                    .flat_map(|c| c.ids.clone())
            })
            .collect(),
        None => view.ids().iter().map(|s| (*s).to_owned()).collect(),
    };
    let finished = all
        .iter()
        .filter(|id| !decided.contains(id))
        .all(|id| cards.iter().any(|(c, _)| c == id));
    let who = crate::returned::who(state, returned);
    let mut builder = Builder::new(state, ctx.now, blobs);
    let mut result = None;
    if !kept.is_empty() {
        let latest = state
            .latest(node)
            .ok_or_else(|| Error::Ledger("a file without checkpoints".to_owned()))?;
        let note = format!("Kept {} from {who}", crate::returned::changes(kept.len()));
        let (checkpoint, file) = checkpoint_into(
            &mut builder,
            ctx.ids,
            ctx.author.clone(),
            node,
            held,
            vec![latest.id.clone()],
            note,
        )?;
        builder.out.file = Some(file);
        builder.out.created = Some(checkpoint.clone());
        builder.out.ids = Some(LineageIds {
            family: state.family_id.to_string(),
            checkpoint: checkpoint.to_string(),
        });
        result = Some(checkpoint);
    }
    builder.push(Event::ReturnReviewed {
        returned_id: returned.clone(),
        into_node_id: node.clone(),
        result_checkpoint_id: result,
        kept,
        skipped,
        finished,
    })?;
    Ok(builder.finish())
}

#[cfg(test)]
mod tests;
