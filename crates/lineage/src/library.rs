// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! "My documents": one entry per family the person tracks, folded from each
//! family's ledger (decision 0015). A damaged ledger still gives an entry, from
//! the part that could be read, flagged so the screen says so.

use serde::{Deserialize, Serialize};

use crate::ids::{Id, Timestamp};
use crate::read::{Readable, read_lenient};
use crate::state::FamilyState;

/// The facts a library row is drawn from. This is also what the library index
/// stores (`index.json`), so it holds data, not wording.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryFacts {
    pub family_id: Id,
    pub name: String,
    /// "docx", "pptx", "xlsx", …
    pub format: String,
    /// The master's file name, if the family has one yet.
    pub master_file: Option<String>,
    /// Tailored versions, archived ones left out.
    pub tailored: usize,
    /// Checkpoints of every file.
    pub checkpoints: usize,
    pub sent: usize,
    /// Files that came back.
    pub returned: usize,
    /// Of those, the ones with changes still to decide.
    pub to_review: usize,
    pub last_at: Timestamp,
    /// The damage note, when only part of the history could be read.
    pub damage: Option<String>,
}

impl EntryFacts {
    /// The facts of a family as far as it could be read.
    #[must_use]
    pub fn of(family_id: &Id, read: &Readable) -> Self {
        let damage = read.note();
        let Some(state) = &read.state else {
            return Self {
                family_id: family_id.clone(),
                name: "A damaged history".to_owned(),
                format: String::new(),
                master_file: None,
                tailored: 0,
                checkpoints: 0,
                sent: 0,
                returned: 0,
                to_review: 0,
                last_at: Timestamp(0),
                damage,
            };
        };
        Self {
            family_id: family_id.clone(),
            name: state.name.clone(),
            format: state.format.clone(),
            master_file: state.master().ok().map(|m| m.display_name.clone()),
            tailored: state.variants().count(),
            checkpoints: state.checkpoints.len(),
            sent: state.sent.len(),
            returned: state.returned.len(),
            to_review: state
                .returned
                .iter()
                .filter(|r| !r.finished() && r.change_count + r.formatting_count > 0)
                .count(),
            last_at: state.last_at,
            damage,
        }
    }

    /// The facts of a whole, undamaged family.
    #[must_use]
    pub fn of_state(state: &FamilyState) -> Self {
        Self::of(
            &state.family_id,
            &Readable {
                state: Some(state.clone()),
                records: 0,
                damage: None,
            },
        )
    }
}

/// One row of "My documents", worded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryEntry {
    pub facts: EntryFacts,
    /// "Word document", "PowerPoint deck", "Excel workbook".
    pub kind: String,
    /// "2 tailored versions · 3 sent · 1 came back".
    pub summary: String,
    /// "Last change 15 Oct 2026".
    pub last_activity: String,
    /// "1 file came back with changes to review".
    pub attention: Option<String>,
    /// False for a format this version of Lineage can't open yet, or a damaged
    /// history: compare, restore and open are not offered.
    pub can_open: bool,
}

/// Formats the engine reads today.
#[must_use]
pub fn readable_format(format: &str) -> bool {
    format == "docx"
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// "Word document" and the rest.
#[must_use]
pub fn kind_of(format: &str) -> &'static str {
    match format {
        "docx" | "gdoc" => "Word document",
        "pptx" | "gslides" => "PowerPoint deck",
        "xlsx" | "gsheet" => "Excel workbook",
        _ => "Document",
    }
}

impl LibraryEntry {
    #[must_use]
    pub fn from_facts(facts: EntryFacts) -> Self {
        let mut parts = vec![
            if facts.tailored == 0 {
                "No tailored versions".to_owned()
            } else {
                plural(facts.tailored, "tailored version", "tailored versions")
            },
            plural(facts.checkpoints, "checkpoint", "checkpoints"),
            format!("{} sent", facts.sent),
        ];
        if facts.returned > 0 {
            parts.push(format!("{} came back", facts.returned));
        }
        let attention = match facts.to_review {
            0 => None,
            1 => Some("1 file came back with changes to review".to_owned()),
            n => Some(format!("{n} files came back with changes to review")),
        };
        Self {
            kind: kind_of(&facts.format).to_owned(),
            summary: parts.join(" · "),
            last_activity: if facts.last_at.0 == 0 {
                String::new()
            } else {
                format!("Last change {}", facts.last_at.date_label())
            },
            attention,
            can_open: facts.damage.is_none() && readable_format(&facts.format),
            facts,
        }
    }
}

/// Every family the person tracks.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Library {
    /// Newest activity first.
    pub entries: Vec<LibraryEntry>,
    /// "You don't track any documents yet." when empty.
    pub empty: Option<String>,
}

impl Library {
    /// The library from facts, newest activity first.
    #[must_use]
    pub fn from_facts(facts: impl IntoIterator<Item = EntryFacts>) -> Self {
        let mut entries: Vec<LibraryEntry> =
            facts.into_iter().map(LibraryEntry::from_facts).collect();
        entries.sort_by(|a, b| {
            b.facts
                .last_at
                .cmp(&a.facts.last_at)
                .then_with(|| a.facts.name.cmp(&b.facts.name))
                .then_with(|| a.facts.family_id.cmp(&b.facts.family_id))
        });
        let empty = entries.is_empty().then(|| {
            "You don't track any documents yet. Open a file in Word and choose Start tracking in the Lineage panel."
                .to_owned()
        });
        Self { entries, empty }
    }
}

/// The library from every family's ledger: `(family id, ledger bytes)`. A
/// folder whose name isn't a family id, or whose ledger is empty, is left out.
#[must_use]
pub fn build_library(ledgers: &[(&str, &[u8])]) -> Library {
    Library::from_facts(ledgers.iter().filter_map(|(family, bytes)| {
        let family = Id::parse(family).ok()?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return None;
        }
        Some(EntryFacts::of(&family, &read_lenient(bytes)))
    }))
}
