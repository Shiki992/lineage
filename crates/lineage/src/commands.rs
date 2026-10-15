// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! One function per thing a person can do in the panel. Each is pure: it takes
//! the family state, the file's bytes and access to stored blobs, and returns the
//! events to append, the blobs to store and the bytes to write. The host does the
//! storing and writing, then folds the new events into its state.

use lineage_core::clean::clean_copy;
use lineage_core::diff::{diff, spec};
use lineage_core::docx::props::{LineageIds, read_ids, write_ids};
use lineage_core::merge::{self, Decision, Status};
use lineage_core::summary::summarize;
use lineage_core::view::{WhatChanged, from_diff, from_plan};
use lineage_core::write::{Revision, apply};
use lineage_core::{Docx, ENGINE_VERSION, Package};

use crate::blob::{Blob, BlobHash, BlobSource, Layered, restore, snapshot};
use crate::changes::{checkpoint_pair, detect, variant_status};
use crate::error::{Error, Result};
use crate::event::{ChangeState, CleanSummary, Event, How, RECORD_VERSION, Record, Role};
use crate::identity::fingerprint;
use crate::ids::{Id, IdSource, Timestamp};
use crate::state::FamilyState;

/// What every command needs from the host.
pub struct Context<'a> {
    pub now: Timestamp,
    pub ids: &'a mut dyn IdSource,
    /// The person's name for checkpoint records, if known.
    pub author: Option<String>,
}

/// A file the host should save as a new file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewFile {
    /// A suggested name, such as "Resume – data roles.docx".
    pub name: String,
    pub bytes: Vec<u8>,
}

/// What a command produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Records to append to the ledger, numbered after the state's last record.
    pub records: Vec<Record>,
    /// Blobs to store (already deduplicated within the outcome).
    pub blobs: Vec<Blob>,
    /// New bytes for the file the command acted on.
    pub file: Option<Vec<u8>>,
    /// The Lineage ids the file now carries. The task pane can set these through
    /// Office.js instead of replacing the whole file.
    pub ids: Option<LineageIds>,
    /// A new file to save: a tailored version, or a sent copy.
    pub new_file: Option<NewFile>,
    /// The new node, checkpoint or sent copy the command created.
    pub created: Option<Id>,
    /// True when there was nothing to do (for example, no changes to save).
    pub nothing_changed: bool,
}

/// Accumulates events against a working copy of the state, so every record is
/// checked by the same rules as a ledger read back from storage.
struct Builder<'s> {
    state: FamilyState,
    now: Timestamp,
    out: Outcome,
    stored: &'s dyn BlobSource,
}

impl<'s> Builder<'s> {
    fn new(state: &FamilyState, now: Timestamp, stored: &'s dyn BlobSource) -> Self {
        Self {
            state: state.clone(),
            now,
            out: Outcome::default(),
            stored,
        }
    }

    fn push(&mut self, event: Event) -> Result<()> {
        let record = Record {
            v: RECORD_VERSION,
            seq: self.state.next_seq(),
            at: self.now,
            event,
        };
        self.state.apply(&record)?;
        self.out.records.push(record);
        Ok(())
    }

    fn blobs(&mut self, blobs: Vec<Blob>) {
        for blob in blobs {
            if !self.out.blobs.iter().any(|b| b.hash == blob.hash) {
                self.out.blobs.push(blob);
            }
        }
    }

    fn source(&self) -> Layered<'_> {
        Layered {
            fresh: &self.out.blobs,
            stored: self.stored,
        }
    }

    fn finish(self) -> Outcome {
        self.out
    }
}

fn with_ids(bytes: &[u8], ids: &LineageIds) -> Result<Vec<u8>> {
    let mut package = Package::read(bytes)?;
    write_ids(&mut package, ids)?;
    Ok(package.to_bytes()?)
}

/// True when two files have the same content apart from Lineage's ids: every
/// part is the same, or the engine finds no change in any story, with every
/// revision accepted and with every revision rejected. Word re-saves a file in
/// its own way (revision ids, dates, settings) even when nothing was edited, so
/// bytes alone would make a checkpoint every time a file went through Word.
/// Edits that live only outside the stories, such as a style's definition,
/// are picked up with the next change to the text.
fn same_content(a: &[u8], b: &[u8]) -> Result<bool> {
    let (pa, pb) = (Package::read(a)?, Package::read(b)?);
    let strip = |p: &Package| -> Vec<(String, Vec<u8>)> {
        let custom = lineage_core::docx::props::custom_part(p);
        p.parts()
            .into_iter()
            .filter(|(name, _)| Some(name) != custom.as_ref())
            .collect()
    };
    if strip(&pa) == strip(&pb) {
        return Ok(true);
    }
    for view in [lineage_core::View::Accepted, lineage_core::View::Rejected] {
        let (da, db) = (Docx::open_view(a, view)?, Docx::open_view(b, view)?);
        if !diff(&da, &db).changes.is_empty() {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Checks the file is a checkpoint of `node` in this family.
fn check_file(state: &FamilyState, node: &Id, bytes: &[u8]) -> Result<LineageIds> {
    let package = Package::read(bytes)?;
    let ids = read_ids(&package)?
        .ok_or_else(|| Error::WrongFile("the file has no Lineage ids".to_owned()))?;
    if ids.family != state.family_id.as_str() {
        return Err(Error::WrongFile(
            "the file belongs to another family".to_owned(),
        ));
    }
    let belongs = state
        .node(node)
        .is_some_and(|n| n.checkpoints.iter().any(|c| c.as_str() == ids.checkpoint));
    if !belongs {
        return Err(Error::WrongFile(
            "the file is a different version".to_owned(),
        ));
    }
    Ok(ids)
}

/// Saves a checkpoint of `bytes` for `node` into the builder. Returns the new
/// checkpoint and the file with its ids.
fn checkpoint_into(
    builder: &mut Builder<'_>,
    ids_source: &mut dyn IdSource,
    author: Option<String>,
    node: &Id,
    bytes: &[u8],
    parents: Vec<Id>,
    note: String,
) -> Result<(Id, Vec<u8>)> {
    let checkpoint = ids_source.next_id();
    let ids = LineageIds {
        family: builder.state.family_id.to_string(),
        checkpoint: checkpoint.to_string(),
    };
    let bytes = with_ids(bytes, &ids)?;
    let docx = Docx::open(&bytes)?;
    let (manifest, blobs) = snapshot(&bytes)?;
    builder.blobs(blobs);
    let is_master = builder.state.node(node).map(|n| n.role) == Some(Role::Master);
    let mut changes = None;
    if is_master
        && let Some(parent) = parents
            .first()
            .and_then(|p| builder.state.checkpoint(p))
            .cloned()
    {
        let before = Docx::open(&restore(&parent.snapshot, &builder.source())?)?;
        let d = diff(&before, &docx);
        let list = spec::change_list(&d.changes, |c| {
            Some(summarize(c, &docx.styles, Some(&docx)))
        });
        let blob = Blob::new(list.to_json().into_bytes());
        changes = Some(blob.hash.clone());
        builder.blobs(vec![blob]);
    }
    builder.push(Event::CheckpointSaved {
        checkpoint_id: checkpoint.clone(),
        node_id: node.clone(),
        parent_ids: parents,
        snapshot: manifest,
        fingerprint: fingerprint(&docx),
        note,
        author,
        changes,
        engine_version: ENGINE_VERSION.to_owned(),
    })?;
    if builder.state.node(node).map(|n| n.role) == Some(Role::Variant) {
        let state = builder.state.clone();
        let events = detect(&state, node, &docx, &checkpoint, &builder.source())?;
        for event in events {
            builder.push(event)?;
        }
    }
    builder.out.ids = Some(ids);
    Ok((checkpoint, bytes))
}

fn stem(name: &str) -> &str {
    name.strip_suffix(".docx").unwrap_or(name)
}

/// "Start tracking": the file becomes the master of a new family, with its
/// first checkpoint. Returns the file with the Lineage ids in it.
///
/// # Errors
/// [`Error::NotAllowed`] when the file is already tracked; engine errors when
/// it is not a Word document Lineage can read.
pub fn start_tracking(
    ctx: &mut Context<'_>,
    bytes: &[u8],
    display_name: &str,
    storage_ref: &str,
    blobs: &dyn BlobSource,
) -> Result<(FamilyState, Outcome)> {
    let package = Package::read(bytes)?;
    if read_ids(&package)?.is_some() {
        return Err(Error::NotAllowed("this file is already tracked".to_owned()));
    }
    Docx::open(bytes)?;
    let family = ctx.ids.next_id();
    let master = ctx.ids.next_id();
    let started = Record {
        v: RECORD_VERSION,
        seq: 1,
        at: ctx.now,
        event: Event::FamilyStarted {
            family_id: family,
            name: stem(display_name).to_owned(),
            format: "docx".to_owned(),
        },
    };
    let state = FamilyState::from_records(std::slice::from_ref(&started))?;
    let mut builder = Builder::new(&state, ctx.now, blobs);
    builder.out.records.push(started);
    builder.push(Event::NodeAdded {
        node_id: master.clone(),
        role: Role::Master,
        display_name: display_name.to_owned(),
        storage_ref: storage_ref.to_owned(),
        made_from_checkpoint_id: None,
        follows_master: false,
    })?;
    let (checkpoint, file) = checkpoint_into(
        &mut builder,
        ctx.ids,
        ctx.author.clone(),
        &master,
        bytes,
        Vec::new(),
        "Started tracking".to_owned(),
    )?;
    builder.out.file = Some(file);
    builder.out.created = Some(checkpoint);
    let state = builder.state.clone();
    Ok((state, builder.finish()))
}

/// "Save checkpoint". For a tailored version this also runs detection: changes
/// typed in by hand are tagged merged, and Lineage changes rejected in Word are
/// tagged declined.
///
/// # Errors
/// [`Error::WrongFile`] when the file is not this node's; engine errors otherwise.
pub fn save_checkpoint(
    ctx: &mut Context<'_>,
    state: &FamilyState,
    node: &Id,
    bytes: &[u8],
    note: &str,
    blobs: &dyn BlobSource,
) -> Result<Outcome> {
    check_file(state, node, bytes)?;
    let latest = state
        .latest(node)
        .ok_or_else(|| Error::Ledger("a node without checkpoints".to_owned()))?;
    let latest_bytes = restore(&latest.snapshot, blobs)?;
    if same_content(bytes, &latest_bytes)? {
        return Ok(Outcome {
            nothing_changed: true,
            ..Outcome::default()
        });
    }
    let mut builder = Builder::new(state, ctx.now, blobs);
    let note = if note.trim().is_empty() {
        "Saved a checkpoint".to_owned()
    } else {
        note.to_owned()
    };
    let (checkpoint, file) = checkpoint_into(
        &mut builder,
        ctx.ids,
        ctx.author.clone(),
        node,
        bytes,
        vec![latest.id.clone()],
        note,
    )?;
    builder.out.file = Some(file);
    builder.out.created = Some(checkpoint);
    Ok(builder.finish())
}

/// "Make a tailored version" of the master's latest checkpoint.
///
/// # Errors
/// When the master's snapshot cannot be restored.
pub fn make_tailored_version(
    ctx: &mut Context<'_>,
    state: &FamilyState,
    display_name: &str,
    storage_ref: &str,
    blobs: &dyn BlobSource,
) -> Result<Outcome> {
    let master = state.master()?;
    let from = state
        .latest(&master.id)
        .ok_or_else(|| Error::Ledger("the master has no checkpoint".to_owned()))?;
    let bytes = restore(&from.snapshot, blobs)?;
    let node = ctx.ids.next_id();
    let mut builder = Builder::new(state, ctx.now, blobs);
    builder.push(Event::NodeAdded {
        node_id: node.clone(),
        role: Role::Variant,
        display_name: display_name.to_owned(),
        storage_ref: storage_ref.to_owned(),
        made_from_checkpoint_id: Some(from.id.clone()),
        follows_master: true,
    })?;
    let note = format!("Made from master {}", state.label(&from.id));
    let (_, file) = checkpoint_into(
        &mut builder,
        ctx.ids,
        ctx.author.clone(),
        &node,
        &bytes,
        vec![from.id.clone()],
        note,
    )?;
    builder.out.new_file = Some(NewFile {
        name: display_name.to_owned(),
        bytes: file,
    });
    builder.out.created = Some(node);
    Ok(builder.finish())
}

/// "What changed" in a file since its latest checkpoint.
///
/// # Errors
/// When the file or the checkpoint cannot be read.
pub fn what_changed(
    state: &FamilyState,
    node: &Id,
    bytes: &[u8],
    blobs: &dyn BlobSource,
) -> Result<WhatChanged> {
    check_file(state, node, bytes)?;
    let latest = state
        .latest(node)
        .ok_or_else(|| Error::Ledger("a node without checkpoints".to_owned()))?;
    let before = Docx::open(&restore(&latest.snapshot, blobs)?)?;
    let after = Docx::open(bytes)?;
    let mut view = from_diff(&diff(&before, &after), &after);
    view.headline = if view.changes.is_empty() && view.formatting.is_empty() {
        format!("No changes since {}", state.label(&latest.id))
    } else {
        format!("{} since {}", view.headline, state.label(&latest.id))
    };
    Ok(view)
}

/// "Offer it to the versions": offers every master change each version has not
/// been offered yet.
///
/// # Errors
/// When a version is unknown or a change list blob is missing.
pub fn offer_to_versions(
    ctx: &mut Context<'_>,
    state: &FamilyState,
    versions: &[Id],
    blobs: &dyn BlobSource,
) -> Result<Outcome> {
    let mut builder = Builder::new(state, ctx.now, blobs);
    for version in versions {
        if state.node(version).map(|n| n.role) != Some(Role::Variant) {
            return Err(Error::NotAllowed(
                "offers go to tailored versions".to_owned(),
            ));
        }
        let status = variant_status(state, version, blobs)?;
        let mut by_checkpoint: Vec<(Id, Vec<String>)> = Vec::new();
        for pending in status.offerable.into_iter().filter(|p| !p.offered) {
            match by_checkpoint
                .iter_mut()
                .find(|(c, _)| *c == pending.checkpoint)
            {
                Some((_, ids)) => ids.push(pending.change.id),
                None => by_checkpoint.push((pending.checkpoint, vec![pending.change.id])),
            }
        }
        for (checkpoint, change_ids) in by_checkpoint {
            builder.push(Event::UpdateOffered {
                offer_id: ctx.ids.next_id(),
                from_checkpoint_id: checkpoint,
                to_node_id: version.clone(),
                change_ids,
            })?;
        }
    }
    if builder.out.records.is_empty() {
        builder.out.nothing_changed = true;
    }
    Ok(builder.finish())
}

/// Offered master changes for one checkpoint, judged against the version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateGroup {
    pub checkpoint: Id,
    /// "From master v2".
    pub label: String,
    pub view: WhatChanged,
}

/// The "updates from the master" panel of a tailored version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Updates {
    /// "2 updates from the master".
    pub headline: String,
    pub groups: Vec<UpdateGroup>,
}

/// The master changes offered to a version, judged against its current file.
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
    let ours = Docx::open(bytes)?;
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
        let (before, after) = checkpoint_pair(state, checkpoint, blobs)?;
        let plan = merge::plan(&before, &ours, &after);
        let mut view = from_plan(&plan);
        view.changes.retain(|c| offered.contains(&&c.id));
        view.formatting.retain(|c| offered.contains(&&c.id));
        view.overlaps.retain(|c| offered.contains(&&c.id));
        view.by_hand.retain(|c| offered.contains(&&c.id));
        view.already_there.retain(|c| offered.contains(&&c.id));
        let count =
            view.changes.len() + view.formatting.len() + view.overlaps.len() + view.by_hand.len();
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

/// "Bring in" and "Decline": writes the brought-in changes into the version
/// as tracked changes by Lineage, saves a checkpoint (decision 0003) and records
/// every decision.
///
/// # Errors
/// [`Error::NotAllowed`] when a decision refers to a change that is not offered;
/// engine errors when a change cannot be written safely (nothing is written).
pub fn decide(
    ctx: &mut Context<'_>,
    state: &FamilyState,
    version: &Id,
    bytes: &[u8],
    decisions: &[(Id, String, Decision)],
    blobs: &dyn BlobSource,
) -> Result<Outcome> {
    check_file(state, version, bytes)?;
    let status = variant_status(state, version, blobs)?;
    for (checkpoint, change, _) in decisions {
        let open = status
            .offerable
            .iter()
            .chain(&status.quiet)
            .any(|p| &p.checkpoint == checkpoint && &p.change.id == change);
        if !open {
            return Err(Error::NotAllowed(format!(
                "change {change} is not waiting for a decision"
            )));
        }
    }
    let revision = Revision::lineage(&ctx.now.iso());
    let mut current = bytes.to_vec();
    let mut brought_from: Vec<Id> = Vec::new();
    let mut merged = Vec::new();
    let mut declined = Vec::new();
    let mut builder = Builder::new(state, ctx.now, blobs);
    for checkpoint in state.master_checkpoints() {
        let local: Vec<(String, Decision)> = decisions
            .iter()
            .filter(|(c, _, _)| *c == checkpoint.id)
            .map(|(_, id, d)| (id.clone(), *d))
            .collect();
        if local.is_empty() {
            continue;
        }
        let (before, after) = checkpoint_pair(state, checkpoint, blobs)?;
        let ours = Docx::open(&current)?;
        let plan = merge::plan(&before, &ours, &after);
        for (id, decision) in &local {
            let item = plan.item(id).ok_or_else(|| {
                Error::NotAllowed(format!("change {id} is not in this checkpoint"))
            })?;
            if matches!(item.status, Status::AlreadyThere { .. }) {
                continue;
            }
            let state_now = if decision.brings_in() {
                ChangeState::Merged
            } else {
                ChangeState::Declined
            };
            if decision.brings_in() {
                merged.push(id.clone());
                if !brought_from.contains(&checkpoint.id) {
                    brought_from.push(checkpoint.id.clone());
                }
            } else {
                declined.push(id.clone());
            }
            builder.push(Event::ChangeStateRecorded {
                node_id: version.clone(),
                change_id: id.clone(),
                from_checkpoint_id: checkpoint.id.clone(),
                state: state_now,
                how: How::Clicked,
            })?;
        }
        current = apply(&plan, &local, &revision)?;
    }
    let mut result = None;
    if !merged.is_empty() && !same_content(&current, bytes)? {
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
            &current,
            parents,
            note,
        )?;
        builder.out.file = Some(file);
        builder.out.created = Some(checkpoint.clone());
        result = Some(checkpoint);
    }
    builder.push(Event::UpdateApplied {
        node_id: version.clone(),
        result_checkpoint_id: result,
        merged,
        declined,
    })?;
    Ok(builder.finish())
}

/// The buttons pressed on update cards: each card id with its decision. Finds
/// the master checkpoint each card belongs to, then runs [`decide`].
///
/// # Errors
/// [`Error::NotAllowed`] when a card is not on the version's updates panel;
/// otherwise as [`decide`].
pub fn decide_cards(
    ctx: &mut Context<'_>,
    state: &FamilyState,
    version: &Id,
    bytes: &[u8],
    cards: &[(String, Decision)],
    blobs: &dyn BlobSource,
) -> Result<Outcome> {
    let panel = updates(state, version, bytes, blobs)?;
    let mut resolved = Vec::with_capacity(cards.len());
    for (card, decision) in cards {
        let group = panel
            .groups
            .iter()
            .find(|g| g.view.ids().contains(&card.as_str()))
            .ok_or_else(|| Error::NotAllowed(format!("no update card {card}")))?;
        resolved.push((group.checkpoint.clone(), card.clone(), *decision));
    }
    decide(ctx, state, version, bytes, &resolved, blobs)
}

/// "Save copy and record it": saves a checkpoint if the file has unsaved
/// changes, makes a clean copy, and records it as sent. The copy carries the
/// two Lineage ids unless `embed_id` is false.
///
/// # Errors
/// When the file is not the node's, or the clean copy cannot be made safely.
#[allow(clippy::too_many_arguments)]
pub fn send_copy(
    ctx: &mut Context<'_>,
    state: &FamilyState,
    node: &Id,
    bytes: &[u8],
    recipient: &str,
    purpose: Option<&str>,
    embed_id: bool,
    blobs: &dyn BlobSource,
) -> Result<Outcome> {
    check_file(state, node, bytes)?;
    let node_info = state
        .node(node)
        .ok_or_else(|| Error::NotAllowed("unknown file".to_owned()))?;
    let latest = state
        .latest(node)
        .ok_or_else(|| Error::Ledger("a node without checkpoints".to_owned()))?;
    let latest_bytes = restore(&latest.snapshot, blobs)?;
    let mut builder = Builder::new(state, ctx.now, blobs);
    let (from, from_bytes) = if same_content(bytes, &latest_bytes)? {
        (latest.id.clone(), latest_bytes)
    } else {
        let note = format!("Saved before sending to {recipient}");
        let (checkpoint, file) = checkpoint_into(
            &mut builder,
            ctx.ids,
            ctx.author.clone(),
            node,
            bytes,
            vec![latest.id.clone()],
            note,
        )?;
        builder.out.file = Some(file.clone());
        (checkpoint, file)
    };
    let ids = embed_id.then(|| LineageIds {
        family: state.family_id.to_string(),
        checkpoint: from.to_string(),
    });
    let (copy, report) = clean_copy(&from_bytes, ids.as_ref())?;
    let blob = Blob::new(copy.clone());
    let file = blob.hash.clone();
    builder.blobs(vec![blob]);
    let sent = ctx.ids.next_id();
    builder.push(Event::CopySent {
        sent_copy_id: sent.clone(),
        from_checkpoint_id: from,
        recipient: recipient.to_owned(),
        purpose: purpose.map(str::to_owned),
        format: "docx".to_owned(),
        file,
        id_embedded: embed_id,
        cleaned: CleanSummary {
            revisions: report.revisions,
            comments: report.comments,
            hidden_runs: report.hidden_runs,
        },
    })?;
    builder.out.new_file = Some(NewFile {
        name: format!("{} ({recipient}).docx", stem(&node_info.display_name)),
        bytes: copy,
    });
    builder.out.created = Some(sent);
    Ok(builder.finish())
}

/// "Open what they received": the exact bytes of a sent copy.
///
/// # Errors
/// When the sent copy is unknown or its blob is missing or damaged.
pub fn open_sent_copy(state: &FamilyState, sent: &Id, blobs: &dyn BlobSource) -> Result<Vec<u8>> {
    let copy = state
        .sent
        .iter()
        .find(|s| &s.id == sent)
        .ok_or_else(|| Error::NotAllowed("unknown sent copy".to_owned()))?;
    let bytes = blobs
        .blob(&copy.file)
        .ok_or_else(|| Error::MissingBlob(copy.file.as_str().to_owned()))?;
    if BlobHash::of(&bytes) != copy.file {
        return Err(Error::Ledger("the stored copy is damaged".to_owned()));
    }
    Ok(bytes)
}
