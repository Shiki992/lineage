// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Panel view models for the personal screens (BLUEPRINT §6): the master panel,
//! a tailored version's panel, and "What I've sent". Every string is what the
//! panel shows, in the words of the glossary.

use crate::blob::BlobSource;
use crate::changes::variant_status;
use crate::error::Result;
use crate::event::Role;
use crate::ids::Id;
use crate::state::{FamilyState, Returned};

/// The latest checkpoint, as a card.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckpointCard {
    pub id: Id,
    /// "v3".
    pub label: String,
    pub note: String,
    /// "15 Oct".
    pub when: String,
}

/// "Your master has 1 change your tailored versions don't have".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Callout {
    pub text: String,
    /// "Offer it to both versions".
    pub button: String,
    /// The versions the button offers to.
    pub versions: Vec<Id>,
}

/// One tailored version in the master's list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionRow {
    pub id: Id,
    pub name: String,
    /// "Has v3 · last sent 2 Oct".
    pub status: String,
    pub button: String,
}

/// A file that came back with changes still to review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReturnedRow {
    pub id: Id,
    /// "Northgate Analytics sent back your copy from 2 Oct".
    pub text: String,
    /// "3 changes", "Formatting changes only", with "· partly reviewed".
    pub detail: String,
    /// "Review".
    pub button: String,
}

fn count_changes(r: &Returned) -> String {
    match (r.change_count, r.formatting_count) {
        (0, 0) => "No changes".to_owned(),
        (0, _) => "Formatting changes only".to_owned(),
        (1, _) => "1 change".to_owned(),
        (n, _) => format!("{n} changes"),
    }
}

/// Files that came back from copies of `node`, with something left to review.
fn returned_rows(state: &FamilyState, node: &Id) -> Vec<ReturnedRow> {
    state
        .returned
        .iter()
        .filter(|r| {
            !r.finished()
                && r.change_count + r.formatting_count > 0
                && state.checkpoint(&r.from).is_some_and(|c| &c.node == node)
        })
        .map(|r| {
            let text = match r.sent.as_ref().and_then(|s| state.sent_copy(s)) {
                Some(sent) => format!(
                    "{} sent back your copy from {}",
                    sent.recipient,
                    sent.at.day_label()
                ),
                None => format!("A copy of {} came back", state.label(&r.from)),
            };
            let mut detail = count_changes(r);
            if !r.reviews.is_empty() {
                detail.push_str(" · partly reviewed");
            }
            ReturnedRow {
                id: r.id.clone(),
                text,
                detail,
                button: "Review".to_owned(),
            }
        })
        .collect()
}

/// The master panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MasterPanel {
    pub file_name: String,
    /// "Master".
    pub chip: String,
    /// "2 tailored versions follow this file".
    pub follows: String,
    pub latest: CheckpointCard,
    pub callout: Option<Callout>,
    /// Files that came back from copies of the master, still to review.
    pub returned: Vec<ReturnedRow>,
    pub versions: Vec<VersionRow>,
    /// "Make a tailored version", "Save checkpoint", "Send a copy".
    pub actions: Vec<String>,
    /// "What I've sent".
    pub footer: String,
}

/// A tailored version's panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionPanel {
    pub file_name: String,
    /// "Tailored version".
    pub chip: String,
    /// "Made from master v1 · has v3".
    pub status: String,
    pub latest: CheckpointCard,
    /// "2 updates from the master", if any were offered.
    pub updates: Option<String>,
    /// Files that came back from copies of this version, still to review.
    pub returned: Vec<ReturnedRow>,
    pub actions: Vec<String>,
    pub footer: String,
}

/// The text with its first letter in lower case, for the middle of a sentence.
fn lower_first(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_lowercase().chain(chars).collect()
    })
}

fn card(state: &FamilyState, node: &Id) -> Option<CheckpointCard> {
    let latest = state.latest(node)?;
    Some(CheckpointCard {
        id: latest.id.clone(),
        label: state.label(&latest.id),
        note: latest.note.clone(),
        when: latest.at.day_label(),
    })
}

fn version_status(
    state: &FamilyState,
    node: &Id,
    blobs: &dyn BlobSource,
) -> Result<(String, usize, Vec<String>)> {
    let status = variant_status(state, node, blobs)?;
    let mut text = format!("Has {}", state.label(&status.base));
    if let Some(sent) = state.sent_from(node).last() {
        text.push_str(&format!(" · last sent {}", sent.at.day_label()));
    }
    let not_offered: Vec<String> = status
        .offerable
        .iter()
        .filter(|p| !p.offered)
        .map(|p| format!("{}:{}", p.checkpoint, p.change.id))
        .collect();
    // A master deck has no "offer" step: "Update N versions" offers and brings
    // in at once (decision 0018), so a deck version waits for every pending
    // change, offered or not (decision 0019).
    let deck = state.format == "pptx";
    let offered = status
        .offerable
        .iter()
        .filter(|p| p.offered || deck)
        .count();
    Ok((text, offered, not_offered))
}

/// The master panel.
///
/// # Errors
/// When a change list blob is missing.
pub fn master_panel(state: &FamilyState, blobs: &dyn BlobSource) -> Result<MasterPanel> {
    let master = state.master()?;
    let mut versions = Vec::new();
    let mut waiting: Vec<String> = Vec::new();
    let mut waiting_for: Vec<Id> = Vec::new();
    for node in state.variants().filter(|n| n.follows_master) {
        let (status, _, not_offered) = version_status(state, &node.id, blobs)?;
        if !not_offered.is_empty() {
            waiting_for.push(node.id.clone());
        }
        for key in not_offered {
            if !waiting.contains(&key) {
                waiting.push(key);
            }
        }
        versions.push(VersionRow {
            id: node.id.clone(),
            name: node.display_name.clone(),
            status,
            button: "Open".to_owned(),
        });
    }
    let follows = match versions.len() {
        0 => "No tailored versions yet".to_owned(),
        1 => "1 tailored version follows this file".to_owned(),
        n => format!("{n} tailored versions follow this file"),
    };
    let callout = (!waiting.is_empty()).then(|| {
        let changes = if waiting.len() == 1 {
            "1 change".to_owned()
        } else {
            format!("{} changes", waiting.len())
        };
        let it = if waiting.len() == 1 { "it" } else { "them" };
        let to = match waiting_for.len() {
            1 if versions.len() == 1 => "your tailored version".to_owned(),
            1 => format!(
                "'{}'",
                state
                    .node(&waiting_for[0])
                    .map_or("", |n| n.display_name.as_str())
            ),
            2 => "both versions".to_owned(),
            n => format!("all {n} versions"),
        };
        let who = if versions.len() == 1 {
            "your tailored version doesn't"
        } else {
            "your tailored versions don't"
        };
        Callout {
            text: format!("Your master has {changes} {who} have"),
            button: format!("Offer {it} to {to}"),
            versions: waiting_for.clone(),
        }
    });
    Ok(MasterPanel {
        file_name: master.display_name.clone(),
        chip: "Master".to_owned(),
        follows,
        latest: card(state, &master.id)
            .ok_or_else(|| crate::Error::Ledger("the master has no checkpoint".to_owned()))?,
        callout,
        returned: returned_rows(state, &master.id),
        versions,
        actions: vec![
            "Make a tailored version".to_owned(),
            "Save checkpoint".to_owned(),
            "Send a copy".to_owned(),
        ],
        footer: "What I've sent".to_owned(),
    })
}

/// A tailored version's panel.
///
/// # Errors
/// When the node is not a tailored version, or a blob is missing.
pub fn version_panel(
    state: &FamilyState,
    node: &Id,
    blobs: &dyn BlobSource,
) -> Result<VersionPanel> {
    let info = state
        .node(node)
        .filter(|n| n.role == Role::Variant)
        .ok_or_else(|| crate::Error::NotAllowed("not a tailored version".to_owned()))?;
    let (has, offered, _) = version_status(state, node, blobs)?;
    let made_from = info
        .made_from
        .as_ref()
        .map(|c| state.label(c))
        .unwrap_or_default();
    Ok(VersionPanel {
        file_name: info.display_name.clone(),
        chip: "Tailored version".to_owned(),
        // "Has v1 · last sent 17 Oct" → "has v1 · last sent 17 Oct".
        status: format!("Made from master {made_from} · {}", lower_first(&has)),
        latest: card(state, node)
            .ok_or_else(|| crate::Error::Ledger("no checkpoint".to_owned()))?,
        updates: match offered {
            0 => None,
            1 => Some("1 update from the master".to_owned()),
            n => Some(format!("{n} updates from the master")),
        },
        returned: returned_rows(state, node),
        actions: vec!["Save checkpoint".to_owned(), "Send a copy".to_owned()],
        footer: "What I've sent".to_owned(),
    })
}

/// One sent copy in "What I've sent".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentRow {
    pub id: Id,
    pub recipient: String,
    /// "Resume – data roles · v2".
    pub version: String,
    pub purpose: Option<String>,
    /// "Word".
    pub format: String,
    /// "2 Oct".
    pub date: String,
    /// "Came back with 5 changes", once a returned file is matched to it.
    pub chip: Option<String>,
    /// The latest file that came back from this copy.
    pub returned: Option<Id>,
    /// "Review" while a returned file has changes to decide, else "Open".
    pub button: String,
    /// Its sharing link, if it was ever shared as one (decision 0032).
    pub link: Option<SentLink>,
    /// While an edit link works: the shared file, to look for their edits.
    pub shared: Option<SharedFile>,
}

/// A sent copy's sharing link, as "What I've sent" shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentLink {
    /// "Shared as a link · Can edit", or "Link stopped 21 Oct".
    pub line: String,
    /// "Anyone with the link can open it".
    pub who: String,
    pub url: String,
    pub access: crate::event::LinkAccess,
    pub live: bool,
    /// The storage's id for the link, and the shared file, to stop it.
    pub id: String,
    pub file_ref: String,
}

/// The file behind a live edit link, and every version of it Lineage has
/// seen (the copy that left, and each that came back from it), as SHA-256
/// hashes: a shared file whose hash isn't among them was edited since.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SharedFile {
    pub file_ref: String,
    pub known: Vec<String>,
}

fn sent_link(copy: &crate::state::SentCopy) -> Option<SentLink> {
    use crate::event::LinkAccess;
    let link = copy.link.as_ref()?;
    let can = match link.access {
        LinkAccess::Edit => "Can edit",
        LinkAccess::View => "View only",
    };
    let line = match link.stopped {
        None => format!("Shared as a link · {can}"),
        Some(at) => format!("Link stopped {}", at.day_label()),
    };
    let who = match link.scope.as_str() {
        "anonymous" => "Anyone with the link can open it",
        "organization" => "People in your organisation with the link can open it",
        "users" => "Only the people it was shared with can open it",
        _ => "Who can open it follows your OneDrive's settings",
    };
    Some(SentLink {
        line,
        who: who.to_owned(),
        url: link.url.clone(),
        access: link.access,
        live: link.live(),
        id: link.id.clone(),
        file_ref: link.file_ref.clone(),
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentGroup {
    /// "October 2026".
    pub month: String,
    pub rows: Vec<SentRow>,
}

/// "What I've sent".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SentList {
    pub title: String,
    pub groups: Vec<SentGroup>,
    /// "Each copy is kept exactly as sent."
    pub note: String,
}

/// "What I've sent", newest first, grouped by month. `search` filters by
/// recipient, version or purpose (case-insensitive).
#[must_use]
pub fn sent_list(state: &FamilyState, search: Option<&str>) -> SentList {
    let needle = search
        .map(str::to_lowercase)
        .filter(|s| !s.trim().is_empty());
    let mut copies: Vec<&crate::state::SentCopy> = state.sent.iter().collect();
    copies.sort_by_key(|s| std::cmp::Reverse(s.at));
    let mut groups: Vec<SentGroup> = Vec::new();
    for copy in copies {
        let node = state
            .checkpoint(&copy.from)
            .and_then(|c| state.node(&c.node));
        let version = format!(
            "{} · {}",
            crate::commands::stem(node.map_or("", |n| n.display_name.as_str())),
            state.label(&copy.from)
        );
        if let Some(needle) = &needle {
            let haystack = format!(
                "{} {} {}",
                copy.recipient,
                version,
                copy.purpose.as_deref().unwrap_or("")
            )
            .to_lowercase();
            if !haystack.contains(needle) {
                continue;
            }
        }
        let returned = state
            .returned
            .iter()
            .rfind(|r| r.sent.as_ref() == Some(&copy.id));
        let chip = returned.map(|r| {
            if r.finished() {
                "Reviewed".to_owned()
            } else if !r.reviews.is_empty() {
                "Partly reviewed".to_owned()
            } else {
                let chip = match (r.change_count, r.formatting_count) {
                    (0, 0) => "Came back unchanged".to_owned(),
                    (0, _) => "Came back with formatting changes only".to_owned(),
                    (1, _) => "Came back with 1 change".to_owned(),
                    (n, _) => format!("Came back with {n} changes"),
                };
                // Macro code from outside is always named (decision 0027).
                if r.new_macro_code {
                    format!("{chip}, including new macro code")
                } else {
                    chip
                }
            }
        });
        let to_review =
            returned.is_some_and(|r| !r.finished() && r.change_count + r.formatting_count > 0);
        let row = SentRow {
            id: copy.id.clone(),
            recipient: copy.recipient.clone(),
            version,
            purpose: copy.purpose.clone(),
            format: crate::library::format_label(&copy.format).to_owned(),
            date: copy.at.day_label(),
            chip,
            returned: returned.map(|r| r.id.clone()),
            button: if to_review { "Review" } else { "Open" }.to_owned(),
            link: sent_link(copy),
            shared: copy
                .link
                .as_ref()
                .filter(|l| l.live() && l.access == crate::event::LinkAccess::Edit)
                .map(|l| SharedFile {
                    file_ref: l.file_ref.clone(),
                    known: std::iter::once(copy.file.as_str().to_owned())
                        .chain(
                            state
                                .returned
                                .iter()
                                .filter(|r| r.sent.as_ref() == Some(&copy.id))
                                .map(|r| r.file.as_str().to_owned()),
                        )
                        .collect(),
                }),
        };
        let month = copy.at.month_label();
        match groups.last_mut() {
            Some(group) if group.month == month => group.rows.push(row),
            _ => groups.push(SentGroup {
                month,
                rows: vec![row],
            }),
        }
    }
    SentList {
        title: "What I've sent".to_owned(),
        groups,
        note: "Each copy is kept exactly as sent.".to_owned(),
    }
}

/// One checkpoint on a file's timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TimelineEntry {
    pub id: Id,
    /// "v3".
    pub label: String,
    pub note: String,
    /// "15 Oct 2026".
    pub date: String,
    pub milestone: Option<String>,
    pub author: Option<String>,
    /// "Sent to Northgate Analytics", "Sent 3 times".
    pub sent: Option<String>,
}

/// One file of the family, with its checkpoints newest first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileHistory {
    pub node: Id,
    pub name: String,
    /// "Master", "Tailored version".
    pub chip: String,
    /// "Made from master v1", "Restored from Resume v2".
    pub status: String,
    pub archived: bool,
    /// Where the file is kept, as the ledger records it.
    pub storage_ref: String,
    pub entries: Vec<TimelineEntry>,
}

/// A document's page in "My documents".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentPage {
    pub family_id: Id,
    pub name: String,
    pub format: String,
    /// "Word document".
    pub kind: String,
    /// False when the history is damaged or the format can't be opened yet:
    /// compare, restore, milestones and relinking are not offered.
    pub can_change: bool,
    /// Why not, in the panel's words.
    pub note: Option<String>,
    /// The master first, then tailored versions in the order they were made.
    pub files: Vec<FileHistory>,
    pub sent: SentList,
    /// "Contains macros".
    pub badges: Vec<String>,
}

/// A document's page. `damage` is the note from a lenient read, if any.
#[must_use]
pub fn document_page(state: &FamilyState, damage: Option<String>) -> DocumentPage {
    let readable = crate::library::readable_format(&state.format);
    let note = damage.clone().or_else(|| {
        (!readable).then(|| "This version of Lineage can't open these files yet.".to_owned())
    });
    let mut nodes: Vec<_> = state.nodes.iter().collect();
    nodes.sort_by_key(|n| n.role != Role::Master);
    let files = nodes
        .into_iter()
        .map(|node| {
            let entries = node
                .checkpoints
                .iter()
                .rev()
                .filter_map(|c| state.checkpoint(c))
                .map(|c| {
                    let sent: Vec<_> = state.sent.iter().filter(|s| s.from == c.id).collect();
                    TimelineEntry {
                        id: c.id.clone(),
                        label: state.label(&c.id),
                        note: c.note.clone(),
                        date: c.at.date_label(),
                        milestone: c.milestone.clone(),
                        author: c.author.clone(),
                        sent: match sent.as_slice() {
                            [] => None,
                            [one] => Some(format!("Sent to {}", one.recipient)),
                            many => Some(format!("Sent {} times", many.len())),
                        },
                    }
                })
                .collect();
            let (chip, status) = match node.role {
                Role::Master => (
                    "Master",
                    "The file your tailored versions follow".to_owned(),
                ),
                _ => (
                    "Tailored version",
                    node.made_from.as_ref().map_or_else(String::new, |m| {
                        format!("Made from master {}", state.label(m))
                    }),
                ),
            };
            let first_note = node
                .checkpoints
                .first()
                .and_then(|c| state.checkpoint(c))
                .map(|c| c.note.as_str())
                .filter(|n| n.starts_with("Restored from "));
            FileHistory {
                node: node.id.clone(),
                name: node.display_name.clone(),
                chip: chip.to_owned(),
                status: first_note.map_or(status, str::to_owned),
                archived: node.archived,
                storage_ref: node.storage_ref.clone(),
                entries,
            }
        })
        .collect();
    DocumentPage {
        family_id: state.family_id.clone(),
        name: state.name.clone(),
        format: state.format.clone(),
        kind: crate::library::kind_of(&state.format).to_owned(),
        can_change: damage.is_none() && readable,
        note,
        files,
        sent: sent_list(state, None),
        badges: if crate::library::master_has_macros(state) {
            vec![crate::library::CONTAINS_MACROS.to_owned()]
        } else {
            Vec::new()
        },
    }
}
