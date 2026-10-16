// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The library index: a small file at the storage root (`index.json`, next to
//! the family folders) so "My documents" can draw every family from one read
//! instead of one ledger each (decision 0015).
//!
//! It is only ever a copy. Each entry records the version of the ledger it was
//! made from (the ledger's length, or its OneDrive eTag), and the host reports
//! every ledger's version as it is now. Where the two disagree, or the index is
//! missing, unreadable or of another version, the ledgers win: those ledgers are
//! read again and the index is rewritten.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::Id;
use crate::library::{EntryFacts, Library};
use crate::read::read_lenient;

/// The index format this crate writes and reads.
pub const INDEX_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Index {
    pub v: u32,
    /// One per family folder, sorted by family id.
    pub families: Vec<Indexed>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Indexed {
    pub family_id: Id,
    /// The version of the ledger these facts were read from.
    pub ledger_version: String,
    /// None for a family folder whose ledger is empty, so it isn't read again
    /// every time but isn't shown either.
    pub facts: Option<EntryFacts>,
}

/// The index, if it can be trusted at all: readable, this version, one entry
/// per family, and each entry about its own family.
#[must_use]
pub fn parse(bytes: &[u8]) -> Option<Index> {
    let index: Index = serde_json::from_slice(bytes).ok()?;
    if index.v != INDEX_VERSION {
        return None;
    }
    let mut seen = std::collections::BTreeSet::new();
    for entry in &index.families {
        let own = entry
            .facts
            .as_ref()
            .is_none_or(|f| f.family_id == entry.family_id);
        if !own || !seen.insert(entry.family_id.clone()) {
            return None;
        }
    }
    Some(index)
}

/// The index as stored: sorted by family id, one line of JSON.
#[must_use]
pub fn encode(index: &Index) -> Vec<u8> {
    let mut sorted = index.clone();
    sorted
        .families
        .sort_by(|a, b| a.family_id.cmp(&b.family_id));
    let mut bytes = serde_json::to_vec(&sorted).unwrap_or_default();
    bytes.push(b'\n');
    bytes
}

fn facts_of(family: &Id, ledger: &[u8]) -> Option<EntryFacts> {
    if ledger.iter().all(u8::is_ascii_whitespace) {
        return None;
    }
    Some(EntryFacts::of(family, &read_lenient(ledger)))
}

/// The library straight from the index, to draw before anything is checked.
#[must_use]
pub fn library_of(index: &[u8]) -> Option<Library> {
    let index = parse(index)?;
    Some(Library::from_facts(
        index.families.into_iter().filter_map(|e| e.facts),
    ))
}

/// The families whose ledgers must be read: every one, if the index can't be
/// trusted; otherwise those missing from it or at another version.
/// `observed` is every family folder and its ledger's version now.
#[must_use]
pub fn stale(index: Option<&[u8]>, observed: &BTreeMap<Id, String>) -> Vec<Id> {
    let Some(index) = index.and_then(parse) else {
        return observed.keys().cloned().collect();
    };
    let known: BTreeMap<&Id, &str> = index
        .families
        .iter()
        .map(|e| (&e.family_id, e.ledger_version.as_str()))
        .collect();
    observed
        .iter()
        .filter(|(family, version)| known.get(family) != Some(&version.as_str()))
        .map(|(family, _)| family.clone())
        .collect()
}

/// The library, and the index to store when it changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refreshed {
    pub library: Library,
    /// The index as it should be stored.
    pub index: Vec<u8>,
    /// True when `index` differs from what was read, so it must be written.
    pub changed: bool,
}

/// Brings the index up to date with the ledgers. `ledgers` holds the bytes of
/// every family [`stale`] named (others are ignored, so passing every ledger
/// is fine).
///
/// # Errors
/// The stale families whose ledgers weren't given.
pub fn refresh(
    index: Option<&[u8]>,
    observed: &BTreeMap<Id, String>,
    ledgers: &[(Id, &[u8])],
) -> Result<Refreshed, Vec<Id>> {
    let stale_now = stale(index, observed);
    let given: BTreeMap<&Id, &[u8]> = ledgers.iter().map(|(f, b)| (f, *b)).collect();
    let missing: Vec<Id> = stale_now
        .iter()
        .filter(|f| !given.contains_key(f))
        .cloned()
        .collect();
    if !missing.is_empty() {
        return Err(missing);
    }
    let old: BTreeMap<Id, Indexed> = index
        .and_then(parse)
        .map(|i| {
            i.families
                .into_iter()
                .map(|e| (e.family_id.clone(), e))
                .collect()
        })
        .unwrap_or_default();
    let families: Vec<Indexed> = observed
        .iter()
        .map(|(family, version)| match old.get(family) {
            Some(entry) if !stale_now.contains(family) => entry.clone(),
            _ => Indexed {
                family_id: family.clone(),
                ledger_version: version.clone(),
                facts: given.get(family).and_then(|bytes| facts_of(family, bytes)),
            },
        })
        .collect();
    let next = Index {
        v: INDEX_VERSION,
        families,
    };
    let bytes = encode(&next);
    Ok(Refreshed {
        library: Library::from_facts(next.families.into_iter().filter_map(|e| e.facts)),
        changed: index != Some(bytes.as_slice()),
        index: bytes,
    })
}

/// The index with one family brought up to date, after its ledger was
/// written. An index that can't be trusted is started again with this family
/// alone; the next [`refresh`] finds the others.
#[must_use]
pub fn update(index: Option<&[u8]>, family: &Id, version: &str, ledger: &[u8]) -> Vec<u8> {
    let mut next = index.and_then(parse).unwrap_or(Index {
        v: INDEX_VERSION,
        families: Vec::new(),
    });
    next.families.retain(|e| &e.family_id != family);
    next.families.push(Indexed {
        family_id: family.clone(),
        ledger_version: version.to_owned(),
        facts: facts_of(family, ledger),
    });
    encode(&next)
}

#[cfg(test)]
mod tests;
