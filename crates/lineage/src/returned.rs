// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Files that come back (BLUEPRINT §3, "Asking for feedback"; decisions 0012 and
//! 0013).
//!
//! 1. [`recognise`]: which copy is this? By the Lineage ids inside it first, then
//!    by its content. Only a match Lineage is sure of is taken without asking;
//!    anything less is a choice for the person, never a silent guess.
//! 2. [`record`]: the person confirms, and the file is kept exactly as it came
//!    back.
//! 3. [`review`]: only the outside person's changes, found against what left (the
//!    sent copy, or the checkpoint), judged against the open file as it is now,
//!    even if it has moved on since.
//! 4. [`decide`]: kept changes are written into the open file as tracked changes
//!    by Lineage, and a checkpoint is saved (decision 0003).

use std::collections::BTreeSet;

use lineage_core::diff::diff;
use lineage_core::docx::props::{LineageIds, read_ids};
use lineage_core::merge::{self, Decision};
use lineage_core::view::{Theirs, WhatChanged, from_plan_for};
use lineage_core::write::{Revision, apply};
use lineage_core::{Docx, Package};

use crate::blob::{Blob, BlobHash, BlobSource, restore};
use crate::commands::{Builder, Context, Outcome, check_file, checkpoint_into, same_content};
use crate::error::{Error, Result};
use crate::event::{Event, MatchedBy};
use crate::identity::{Fingerprint, content_match, fingerprint, similarity};
use crate::ids::Id;
use crate::state::FamilyState;

/// Least sketch similarity for a checkpoint to be looked at closely.
pub const SHORTLIST_MIN: u8 = 20;
/// How many checkpoints copies were sent from are looked at closely.
pub const SHORTLIST_SENT: usize = 6;
/// How many checkpoints nothing was sent from are looked at closely.
pub const SHORTLIST_UNSENT: usize = 2;
/// Least content match for a sent copy to be taken without asking.
pub const CERTAIN: u8 = 80;
/// How many more changed words every copy sent from another checkpoint must
/// need than a sure match: the evidence that it is this copy and not that one.
pub const LEAD: usize = 5;
/// Least content match to be offered as a choice at all.
pub const POSSIBLE: u8 = 50;
/// Choices are those within this much of the best.
pub const WITHIN: u8 = 20;
/// At most this many choices.
pub const CHOICES: usize = 6;

/// A copy a file could have come from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub family: Id,
    /// The checkpoint the copy left from.
    pub checkpoint: Id,
    /// The sent copy, or `None` for a copy Lineage has no record of sending.
    pub sent: Option<Id>,
    /// How much of the text matches what left, 0 to 100 (100 by ids).
    pub score: u8,
    /// Words changed since it left (0 by ids, where it isn't measured).
    pub changed: usize,
}

/// What Lineage concluded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// One copy, sure enough to take without asking.
    Certain,
    /// More than one copy fits, or the best fit isn't close enough: the person chooses.
    Choose,
    /// Nothing fits.
    Unknown,
}

/// The answer to "which copy is this?".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recognition {
    /// True when the file's Lineage ids decided it.
    pub by_ids: bool,
    pub verdict: Verdict,
    /// Best first. One when certain, none when unknown.
    pub candidates: Vec<Candidate>,
}

/// A family to look in, with access to its blobs.
pub struct Family<'a> {
    pub state: &'a FamilyState,
    pub blobs: &'a dyn BlobSource,
}

/// A returned file, read once.
pub struct Prepared {
    pub docx: Docx,
    pub ids: Option<LineageIds>,
    pub print: Fingerprint,
}

/// Reads a returned file. It is untrusted input: the reader's limits apply.
///
/// # Errors
/// When the file is not a Word document Lineage can read.
pub fn prepare(bytes: &[u8]) -> Result<Prepared> {
    let package = Package::read(bytes)?;
    let ids = read_ids(&package)?;
    let docx = Docx::from_package(package, lineage_core::View::Accepted)?;
    let print = fingerprint(&docx);
    Ok(Prepared { docx, ids, print })
}

/// What the file's ids say, if they name something in these families.
#[must_use]
pub fn by_ids(prepared: &Prepared, states: &[&FamilyState]) -> Option<Recognition> {
    let ids = prepared.ids.as_ref()?;
    let state = states.iter().find(|s| s.family_id.as_str() == ids.family)?;
    let id = Id::parse(&ids.checkpoint).ok()?;
    let certain = |checkpoint: Id, sent: Option<Id>| Recognition {
        by_ids: true,
        verdict: Verdict::Certain,
        candidates: vec![Candidate {
            family: state.family_id.clone(),
            checkpoint,
            sent,
            score: 100,
            changed: 0,
        }],
    };
    if let Some(sent) = state.sent_copy(&id) {
        return Some(certain(sent.from.clone(), Some(sent.id.clone())));
    }
    state.checkpoint(&id)?;
    // A checkpoint's id: the person's own file sent outside Lineage, or a copy
    // made before sent copies carried their own id (decision 0012).
    let older: Vec<Candidate> = state
        .sent
        .iter()
        .filter(|s| s.from == id && s.id_embedded)
        .map(|s| Candidate {
            family: state.family_id.clone(),
            checkpoint: id.clone(),
            sent: Some(s.id.clone()),
            score: 100,
            changed: 0,
        })
        .collect();
    if older.is_empty() {
        return Some(certain(id, None));
    }
    let mut candidates = older;
    candidates.push(Candidate {
        family: state.family_id.clone(),
        checkpoint: id,
        sent: None,
        score: 100,
        changed: 0,
    });
    Some(Recognition {
        by_ids: true,
        verdict: Verdict::Choose,
        candidates,
    })
}

/// The checkpoints worth comparing closely, by family, from the fingerprints
/// stored in the ledgers (no blobs needed).
#[must_use]
pub fn shortlist(prepared: &Prepared, states: &[&FamilyState]) -> Vec<(Id, Vec<Id>)> {
    let mut scored: Vec<(u8, bool, &Id, &Id)> = Vec::new();
    for state in states {
        for checkpoint in &state.checkpoints {
            let sim = similarity(&prepared.print, &checkpoint.fingerprint);
            if sim >= SHORTLIST_MIN {
                let sent = state.sent.iter().any(|s| s.from == checkpoint.id);
                scored.push((sim, sent, &state.family_id, &checkpoint.id));
            }
        }
    }
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.cmp(b.2)).then(a.3.cmp(b.3)));
    let sent = scored.iter().filter(|s| s.1).take(SHORTLIST_SENT);
    let unsent = scored.iter().filter(|s| !s.1).take(SHORTLIST_UNSENT);
    let mut picked: Vec<&(u8, bool, &Id, &Id)> = sent.chain(unsent).collect();
    picked.sort_by(|a, b| b.0.cmp(&a.0).then(a.2.cmp(b.2)).then(a.3.cmp(b.3)));
    let mut out: Vec<(Id, Vec<Id>)> = Vec::new();
    for (_, _, family, checkpoint) in picked {
        match out.iter_mut().find(|(f, _)| f == *family) {
            Some((_, list)) => list.push((*checkpoint).clone()),
            None => out.push(((*family).clone(), vec![(*checkpoint).clone()])),
        }
    }
    out
}

/// The exact bytes a copy left as: the sent copy, or the checkpoint's snapshot
/// when nothing was recorded as sent.
///
/// # Errors
/// When a blob is missing or damaged, or the copy is unknown.
pub fn base_bytes(
    state: &FamilyState,
    checkpoint: &Id,
    sent: Option<&Id>,
    blobs: &dyn BlobSource,
) -> Result<Vec<u8>> {
    let saved = state
        .checkpoint(checkpoint)
        .ok_or_else(|| Error::NotAllowed("unknown checkpoint".to_owned()))?;
    match sent {
        Some(sent) => {
            let copy = state
                .sent_copy(sent)
                .filter(|s| &s.from == checkpoint)
                .ok_or_else(|| Error::NotAllowed("unknown sent copy".to_owned()))?;
            whole_blob(&copy.file, blobs)
        }
        None => restore(&saved.snapshot, blobs),
    }
}

/// A blob that is a whole file (a sent copy, a returned file), checked.
fn whole_blob(hash: &BlobHash, blobs: &dyn BlobSource) -> Result<Vec<u8>> {
    let bytes = blobs
        .blob(hash)
        .ok_or_else(|| Error::MissingBlob(hash.as_str().to_owned()))?;
    if &BlobHash::of(&bytes) != hash {
        return Err(Error::Ledger("a stored file is damaged".to_owned()));
    }
    Ok(bytes)
}

/// Compares the file closely with what left from each of these checkpoints of
/// one family. Every missing blob is asked for in one go.
///
/// # Errors
/// [`Error::MissingBlob`] when blobs must be fetched first; others when stored
/// files are damaged.
pub fn score(
    prepared: &Prepared,
    state: &FamilyState,
    checkpoints: &[Id],
    blobs: &dyn BlobSource,
) -> Result<Vec<Candidate>> {
    let mut out = Vec::new();
    let mut missing = None;
    for checkpoint in checkpoints {
        let copies: Vec<&Id> = state
            .sent
            .iter()
            .filter(|s| &s.from == checkpoint)
            .map(|s| &s.id)
            .collect();
        // Every copy sent from one checkpoint has the same content; one will do.
        let base = match base_bytes(state, checkpoint, copies.first().copied(), blobs) {
            Ok(bytes) => bytes,
            Err(Error::MissingBlob(hash)) => {
                missing.get_or_insert(hash);
                continue;
            }
            Err(e) => return Err(e),
        };
        let found = content_match(&Docx::open(&base)?, &prepared.docx);
        let sent: Vec<Option<Id>> = if copies.is_empty() {
            vec![None]
        } else {
            copies.into_iter().map(|c| Some(c.clone())).collect()
        };
        for sent in sent {
            out.push(Candidate {
                family: state.family_id.clone(),
                checkpoint: checkpoint.clone(),
                sent,
                score: found.percent,
                changed: found.changed,
            });
        }
    }
    match missing {
        Some(hash) => Err(Error::MissingBlob(hash)),
        None => Ok(out),
    }
}

/// Decides from close comparisons (decision 0013):
/// - **certain** when the closest sent copy matches at least [`CERTAIN`], is
///   the only copy sent from its checkpoint, needs at least [`LEAD`] fewer
///   changed words than any copy sent from another checkpoint, and no
///   unrecorded checkpoint needs fewer;
/// - otherwise a **choice** of those within [`WITHIN`] of the best and at least
///   [`POSSIBLE`]. A checkpoint nothing was recorded as sent from is listed only
///   when it fits better than every recorded copy;
/// - **unknown** when nothing reaches [`POSSIBLE`].
#[must_use]
pub fn verdict(mut candidates: Vec<Candidate>) -> Recognition {
    candidates.sort_by(|a, b| {
        a.changed
            .cmp(&b.changed)
            .then(b.score.cmp(&a.score))
            .then(b.sent.is_some().cmp(&a.sent.is_some()))
            .then(a.family.cmp(&b.family))
            .then(a.checkpoint.cmp(&b.checkpoint))
            .then(a.sent.cmp(&b.sent))
    });
    if let Some(best) = candidates.iter().find(|c| c.sent.is_some())
        && best.score >= CERTAIN
    {
        let alone = !candidates
            .iter()
            .any(|c| c.sent.is_some() && c.sent != best.sent && c.checkpoint == best.checkpoint);
        let clear = !candidates.iter().any(|c| {
            c.sent.is_some() && c.checkpoint != best.checkpoint && c.changed < best.changed + LEAD
        });
        let unsent_better = candidates
            .iter()
            .any(|c| c.sent.is_none() && c.changed < best.changed);
        if alone && clear && !unsent_better {
            return Recognition {
                by_ids: false,
                verdict: Verdict::Certain,
                candidates: vec![best.clone()],
            };
        }
    }
    let top = candidates.iter().map(|c| c.score).max().unwrap_or(0);
    if top < POSSIBLE {
        return Recognition {
            by_ids: false,
            verdict: Verdict::Unknown,
            candidates: Vec::new(),
        };
    }
    let closest_sent = candidates
        .iter()
        .filter(|c| c.sent.is_some() && c.score >= POSSIBLE)
        .map(|c| c.changed)
        .min();
    let choices = candidates
        .into_iter()
        .filter(|c| c.score >= POSSIBLE && c.score + WITHIN >= top)
        .filter(|c| c.sent.is_some() || closest_sent.is_none_or(|best| c.changed < best))
        .take(CHOICES)
        .collect();
    Recognition {
        by_ids: false,
        verdict: Verdict::Choose,
        candidates: choices,
    }
}

/// Which copy a file is, looking in every family: by its ids, else by content.
///
/// # Errors
/// When the file can't be read, or blobs are missing.
pub fn recognise(bytes: &[u8], families: &[Family<'_>]) -> Result<Recognition> {
    let prepared = prepare(bytes)?;
    let states: Vec<&FamilyState> = families.iter().map(|f| f.state).collect();
    if let Some(found) = by_ids(&prepared, &states) {
        return Ok(found);
    }
    let mut all = Vec::new();
    for (family, checkpoints) in shortlist(&prepared, &states) {
        if let Some(f) = families.iter().find(|f| f.state.family_id == family) {
            all.extend(score(&prepared, f.state, &checkpoints, f.blobs)?);
        }
    }
    Ok(verdict(all))
}

/// One copy, as the panel lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CandidateRow {
    pub family: Id,
    pub checkpoint: Id,
    pub sent: Option<Id>,
    /// The file the copy was made from, where its changes are reviewed.
    pub node: Id,
    /// "Northgate Analytics", or "A copy you didn't record sending".
    pub title: String,
    /// "Resume – data roles · v2 · sent 2 Oct".
    pub detail: String,
    /// "92% of the text matches", or "Recognised by its Lineage ID".
    pub score: String,
}

/// "Which copy is this?", ready to draw.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecognitionView {
    pub verdict: Verdict,
    pub by_ids: bool,
    pub headline: String,
    pub note: Option<String>,
    pub candidates: Vec<CandidateRow>,
}

fn stem(name: &str) -> &str {
    name.strip_suffix(".docx").unwrap_or(name)
}

/// What the panel shows for a recognition.
#[must_use]
pub fn describe(found: &Recognition, states: &[&FamilyState]) -> RecognitionView {
    let rows: Vec<CandidateRow> = found
        .candidates
        .iter()
        .filter_map(|c| {
            let state = states.iter().find(|s| s.family_id == c.family)?;
            let checkpoint = state.checkpoint(&c.checkpoint)?;
            let node = state.node(&checkpoint.node)?;
            let version = format!(
                "{} · {}",
                stem(&node.display_name),
                state.label(&checkpoint.id)
            );
            let sent = c.sent.as_ref().and_then(|s| state.sent_copy(s));
            Some(CandidateRow {
                family: c.family.clone(),
                checkpoint: c.checkpoint.clone(),
                sent: c.sent.clone(),
                node: node.id.clone(),
                title: sent.map_or_else(
                    || "A copy you didn't record sending".to_owned(),
                    |s| s.recipient.clone(),
                ),
                detail: sent.map_or_else(
                    || format!("{version} · saved {}", checkpoint.at.day_label()),
                    |s| format!("{version} · sent {}", s.at.day_label()),
                ),
                score: if found.by_ids {
                    "Recognised by its Lineage ID".to_owned()
                } else {
                    format!("{}% of the text matches", c.score)
                },
            })
        })
        .collect();
    let (headline, note) = match (found.verdict, rows.first()) {
        (Verdict::Certain, Some(row)) if row.sent.is_some() => {
            (format!("This came back from {}", row.title), None)
        }
        (Verdict::Certain, Some(row)) => (
            format!(
                "This is a copy of {}",
                row.detail.split(" · saved").next().unwrap_or("")
            ),
            Some(
                "Lineage has no record of sending it, so it is compared with that checkpoint."
                    .to_owned(),
            ),
        ),
        (Verdict::Unknown, _) | (_, None) => (
            "Lineage doesn't recognise this file".to_owned(),
            Some(
                "It doesn't match any copy you've sent, or any checkpoint Lineage has saved."
                    .to_owned(),
            ),
        ),
        (Verdict::Choose, Some(_)) => (
            "Which copy is this?".to_owned(),
            Some(
                "It could be more than one copy. Choose the one it came from, so Lineage shows only the changes made to it."
                    .to_owned(),
            ),
        ),
    };
    RecognitionView {
        verdict: if rows.is_empty() {
            Verdict::Unknown
        } else {
            found.verdict
        },
        by_ids: found.by_ids,
        headline,
        note,
        candidates: rows,
    }
}

/// "Review the changes" on a recognised file: keeps the file exactly as it came
/// back, with what left and how many changes it has. Recording the same file
/// for the same copy again does nothing and returns the earlier record.
///
/// # Errors
/// [`Error::NotAllowed`] when the copy is unknown, or `how` is [`MatchedBy::Ids`]
/// but the file's ids don't name it; others when files can't be read.
#[allow(clippy::too_many_arguments)]
pub fn record(
    ctx: &mut Context<'_>,
    state: &FamilyState,
    bytes: &[u8],
    display_name: &str,
    checkpoint: &Id,
    sent: Option<&Id>,
    how: MatchedBy,
    blobs: &dyn BlobSource,
) -> Result<Outcome> {
    let prepared = prepare(bytes)?;
    let base = Docx::open(&base_bytes(state, checkpoint, sent, blobs)?)?;
    if how == MatchedBy::Ids {
        let named = prepared.ids.as_ref().is_some_and(|ids| {
            ids.family == state.family_id.as_str()
                && (ids.checkpoint == checkpoint.as_str()
                    || sent.is_some_and(|s| ids.checkpoint == s.as_str()))
        });
        if !named {
            return Err(Error::NotAllowed(
                "the file's ids don't name that copy".to_owned(),
            ));
        }
    }
    let file = Blob::new(bytes.to_vec());
    if let Some(earlier) = state
        .returned
        .iter()
        .find(|r| r.file == file.hash && &r.from == checkpoint && r.sent.as_ref() == sent)
    {
        return Ok(Outcome {
            nothing_changed: true,
            created: Some(earlier.id.clone()),
            ..Outcome::default()
        });
    }
    let changes = diff(&base, &prepared.docx).changes;
    let formatting = changes.iter().filter(|c| c.formatting_only).count();
    let confidence = match how {
        MatchedBy::Ids => 100,
        MatchedBy::Content | MatchedBy::Person => content_match(&base, &prepared.docx).percent,
    };
    let name = display_name.trim();
    let returned = ctx.ids.next_id();
    let mut builder = Builder::new(state, ctx.now, blobs);
    builder.push(Event::FileReturned {
        returned_id: returned.clone(),
        sent_copy_id: sent.cloned(),
        from_checkpoint_id: checkpoint.clone(),
        display_name: if name.is_empty() {
            "Returned file.docx".to_owned()
        } else {
            name.to_owned()
        },
        file: file.hash.clone(),
        matched_by: how,
        confidence,
        change_count: changes.len() - formatting,
        formatting_count: formatting,
    })?;
    builder.blobs(vec![file]);
    builder.out.created = Some(returned);
    Ok(builder.finish())
}

/// The review of a returned file: their changes, judged against the open file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReturnReview {
    pub returned: Id,
    /// "3 changes from Northgate Analytics".
    pub headline: String,
    /// "You sent this to Northgate Analytics on 2 Oct, from Resume – data roles v2."
    pub from: String,
    /// "Your file has changed since…", "Their changes will go into…".
    pub notes: Vec<String>,
    /// Who made the changes, for the overlap cards: "Northgate Analytics".
    pub theirs: String,
    /// The file the copy was made from, and its name.
    pub from_node: Id,
    pub from_name: String,
    /// Their changes still to decide, and those the file already has.
    pub view: WhatChanged,
}

/// The three documents of a review: what left, the open file, what came back.
fn documents(
    state: &FamilyState,
    returned: &Id,
    node: &Id,
    bytes: &[u8],
    blobs: &dyn BlobSource,
) -> Result<(Docx, Docx, Docx)> {
    check_file(state, node, bytes)?;
    let r = state
        .returned_file(returned)
        .ok_or_else(|| Error::NotAllowed("unknown returned file".to_owned()))?;
    let base = base_bytes(state, &r.from, r.sent.as_ref(), blobs)?;
    let theirs = whole_blob(&r.file, blobs)?;
    Ok((Docx::open(&base)?, Docx::open(bytes)?, Docx::open(&theirs)?))
}

fn who(state: &FamilyState, returned: &Id) -> String {
    state
        .returned_file(returned)
        .and_then(|r| r.sent.as_ref())
        .and_then(|s| state.sent_copy(s))
        .map_or_else(|| "the returned copy".to_owned(), |s| s.recipient.clone())
}

fn changes(n: usize) -> String {
    if n == 1 {
        "1 change".to_owned()
    } else {
        format!("{n} changes")
    }
}

/// Removes decided cards, and words the headline for what is left.
fn open_view(mut view: WhatChanged, decided: &BTreeSet<&String>, who: &str) -> WhatChanged {
    view.changes.retain(|c| !decided.contains(&c.id));
    view.formatting.retain(|c| !decided.contains(&c.id));
    view.overlaps.retain(|c| !decided.contains(&c.id));
    view.by_hand.retain(|c| !decided.contains(&c.id));
    view.already_there.retain(|c| !decided.contains(&c.id));
    let left = view.ids().len();
    view.headline = if left == 0 {
        "Nothing left to review".to_owned()
    } else {
        format!("{} from {who}", changes(left))
    };
    view
}

/// "Review": the changes in a returned file still to decide, judged against
/// the open file `bytes` (a file of `node`).
///
/// # Errors
/// When the file is not `node`'s, the returned file is unknown, or blobs are missing.
pub fn review(
    state: &FamilyState,
    returned: &Id,
    node: &Id,
    bytes: &[u8],
    blobs: &dyn BlobSource,
) -> Result<ReturnReview> {
    let (base, ours, theirs) = documents(state, returned, node, bytes, blobs)?;
    let r = state
        .returned_file(returned)
        .ok_or_else(|| Error::NotAllowed("unknown returned file".to_owned()))?;
    let who = who(state, returned);
    let plan = merge::plan(&base, &ours, &theirs);
    let decided: BTreeSet<&String> = r.decided().collect();
    let view = open_view(
        from_plan_for(&plan, &Theirs::Returned { who: who.clone() }),
        &decided,
        &who,
    );
    let from_node = state
        .checkpoint(&r.from)
        .and_then(|c| state.node(&c.node))
        .ok_or_else(|| Error::Ledger("a returned file from an unknown file".to_owned()))?;
    let version = format!("{} {}", stem(&from_node.display_name), state.label(&r.from));
    let from = match r.sent.as_ref().and_then(|s| state.sent_copy(s)) {
        Some(sent) => format!(
            "You sent this to {} on {}, from {version}.",
            sent.recipient,
            sent.at.day_label()
        ),
        None => format!("A copy of {version}."),
    };
    let mut notes = Vec::new();
    if &from_node.id == node {
        if let Some(latest) = state.latest(node).filter(|l| l.id != r.from) {
            notes.push(format!(
                "Your file has changed since this copy left (it's now {}). Only their changes are shown.",
                state.label(&latest.id)
            ));
        }
    } else {
        let into = state.node(node).map_or("", |n| n.display_name.as_str());
        notes.push(format!(
            "This copy was made from '{}'. The changes you keep will go into '{}'.",
            stem(&from_node.display_name),
            stem(into)
        ));
    }
    let theirs_label = {
        let mut chars = who.chars();
        chars.next().map_or_else(String::new, |first| {
            first.to_uppercase().chain(chars).collect()
        })
    };
    Ok(ReturnReview {
        returned: returned.clone(),
        headline: view.headline.clone(),
        from,
        notes,
        theirs: theirs_label,
        from_node: from_node.id.clone(),
        from_name: stem(&from_node.display_name).to_owned(),
        view,
    })
}

/// "Apply" on a review: writes the kept changes into the open file as tracked
/// changes by Lineage, saves a checkpoint, and records every decision. Changes
/// not decided stay for later.
///
/// # Errors
/// [`Error::NotAllowed`] when a card isn't waiting for a decision or the
/// decision isn't one of its choices; engine errors when a change can't be
/// written safely (nothing is written).
pub fn decide(
    ctx: &mut Context<'_>,
    state: &FamilyState,
    returned: &Id,
    node: &Id,
    bytes: &[u8],
    cards: &[(String, Decision)],
    blobs: &dyn BlobSource,
) -> Result<Outcome> {
    if cards.is_empty() {
        return Err(Error::NotAllowed("nothing was decided".to_owned()));
    }
    let (base, ours, theirs) = documents(state, returned, node, bytes, blobs)?;
    let r = state
        .returned_file(returned)
        .ok_or_else(|| Error::NotAllowed("unknown returned file".to_owned()))?;
    let who = who(state, returned);
    let plan = merge::plan(&base, &ours, &theirs);
    let decided: BTreeSet<&String> = r.decided().collect();
    let view = open_view(
        from_plan_for(&plan, &Theirs::Returned { who: who.clone() }),
        &decided,
        &who,
    );
    let open = view.ids();
    for (card, decision) in cards {
        let item = plan
            .item(card)
            .filter(|_| open.contains(&card.as_str()))
            .ok_or_else(|| {
                Error::NotAllowed(format!("change {card} is not waiting for a decision"))
            })?;
        if !item.choices().contains(decision) {
            return Err(Error::NotAllowed(format!(
                "that isn't one of the choices for change {card}"
            )));
        }
    }
    let written = apply(&plan, cards, &Revision::lineage(&ctx.now.iso()))?;
    let (kept, skipped): (Vec<_>, Vec<_>) = cards.iter().partition(|(_, d)| d.brings_in());
    let kept: Vec<String> = kept.into_iter().map(|(c, _)| c.clone()).collect();
    let skipped: Vec<String> = skipped.into_iter().map(|(c, _)| c.clone()).collect();
    let finished = open
        .iter()
        .all(|id| cards.iter().any(|(c, _)| c.as_str() == *id));
    let mut builder = Builder::new(state, ctx.now, blobs);
    let mut result = None;
    if !kept.is_empty() && !same_content(&written, bytes)? {
        let latest = state
            .latest(node)
            .ok_or_else(|| Error::Ledger("a file without checkpoints".to_owned()))?;
        let note = format!("Kept {} from {who}", changes(kept.len()));
        let (checkpoint, file) = checkpoint_into(
            &mut builder,
            ctx.ids,
            ctx.author.clone(),
            node,
            &written,
            vec![latest.id.clone()],
            note,
        )?;
        builder.out.file = Some(file);
        builder.out.created = Some(checkpoint.clone());
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

/// "Open what came back": the exact bytes of a returned file.
///
/// # Errors
/// When the returned file is unknown or its blob is missing or damaged.
pub fn open(state: &FamilyState, returned: &Id, blobs: &dyn BlobSource) -> Result<Vec<u8>> {
    let r = state
        .returned_file(returned)
        .ok_or_else(|| Error::NotAllowed("unknown returned file".to_owned()))?;
    whole_blob(&r.file, blobs)
}

#[cfg(test)]
mod tests;
