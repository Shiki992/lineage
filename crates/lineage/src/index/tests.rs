// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use proptest::prelude::*;

use super::*;
use crate::library::build_library;
use crate::testfamily::everything;

/// Three families' ledgers: the everything family, a fresh one, and an empty
/// folder. Made once: building them runs the whole engine.
fn ledgers() -> Vec<(Id, Vec<u8>)> {
    static MADE: std::sync::OnceLock<Vec<(Id, Vec<u8>)>> = std::sync::OnceLock::new();
    MADE.get_or_init(make).clone()
}

fn make() -> Vec<(Id, Vec<u8>)> {
    let family = everything();
    let mut fresh = crate::tests::Host::new();
    fresh.ids = crate::ids::SequentialIds::new(9);
    fresh.start(
        &lineage_testkit::resume::package("t", &lineage_testkit::resume::master_v1_lines())
            .to_bytes(),
    );
    vec![
        (
            family.host.state().family_id.clone(),
            family.host.ledger_bytes(),
        ),
        (fresh.state().family_id.clone(), fresh.ledger_bytes()),
        (Id::parse(&"e".repeat(32)).expect("id"), Vec::new()),
    ]
}

fn observed(ledgers: &[(Id, Vec<u8>)]) -> BTreeMap<Id, String> {
    ledgers
        .iter()
        .map(|(f, b)| (f.clone(), b.len().to_string()))
        .collect()
}

fn borrowed(ledgers: &[(Id, Vec<u8>)]) -> Vec<(Id, &[u8])> {
    ledgers
        .iter()
        .map(|(f, b)| (f.clone(), b.as_slice()))
        .collect()
}

fn full(ledgers: &[(Id, Vec<u8>)]) -> Refreshed {
    refresh(None, &observed(ledgers), &borrowed(ledgers)).expect("refreshes")
}

#[test]
fn no_index_means_every_ledger_is_read_and_an_index_written() {
    let ledgers = ledgers();
    let seen = observed(&ledgers);
    assert_eq!(stale(None, &seen).len(), 3);
    assert_eq!(
        refresh(None, &seen, &[]).expect_err("needs ledgers").len(),
        3
    );
    let first = full(&ledgers);
    assert!(first.changed);
    assert_eq!(
        first.library.entries.len(),
        2,
        "the empty folder isn't shown"
    );
    let pairs: Vec<(&str, &[u8])> = ledgers
        .iter()
        .map(|(f, b)| (f.as_str(), b.as_slice()))
        .collect();
    assert_eq!(first.library, build_library(&pairs));

    // Now the index is right: nothing to read, nothing to write.
    assert!(stale(Some(&first.index), &seen).is_empty());
    let second = refresh(Some(&first.index), &seen, &[]).expect("fresh");
    assert!(!second.changed);
    assert_eq!(second.library, first.library);
    assert_eq!(library_of(&first.index), Some(first.library.clone()));
}

#[test]
fn a_ledger_that_moved_on_is_read_again_and_a_removed_family_drops_out() {
    let mut ledgers = ledgers();
    let first = full(&ledgers);
    let mut fam = everything();
    let latest = fam
        .host
        .state()
        .latest(&fam.master)
        .expect("latest")
        .id
        .clone();
    fam.host.milestone(&latest, Some("Final"));
    ledgers[0].1 = fam.host.ledger_bytes();
    ledgers.remove(1);
    let seen = observed(&ledgers);
    assert_eq!(stale(Some(&first.index), &seen), vec![ledgers[0].0.clone()]);
    let next = refresh(Some(&first.index), &seen, &borrowed(&ledgers)).expect("refreshes");
    assert!(next.changed);
    assert_eq!(next.library.entries.len(), 1);
    assert_eq!(next.library, full(&ledgers).library);
}

#[test]
fn update_after_a_write_matches_a_rebuild() {
    let ledgers = ledgers();
    let mut index = None;
    for (family, bytes) in &ledgers {
        index = Some(update(
            index.as_deref(),
            family,
            &bytes.len().to_string(),
            bytes,
        ));
    }
    let index = index.expect("index");
    assert_eq!(index, full(&ledgers).index);
    // From an index that can't be trusted, an update starts again with one family.
    let lone = update(Some(b"{not json"), &ledgers[1].0, "1", &ledgers[1].1);
    assert_eq!(parse(&lone).expect("parses").families.len(), 1);
}

#[test]
fn indexes_that_cannot_be_trusted_are_refused() {
    let ledgers = ledgers();
    let good = full(&ledgers).index;
    let text = String::from_utf8(good.clone()).expect("utf-8");
    let other_version = text.replacen("\"v\":1", "\"v\":2", 1);
    let extra_field = text.replacen("\"v\":1", "\"v\":1,\"x\":0", 1);
    let mut index = parse(&good).expect("parses");
    let mut doubled = index.clone();
    doubled.families.push(index.families[0].clone());
    index.families[0].family_id = index.families[1].family_id.clone();
    for bad in [
        &b""[..],
        b"[]",
        other_version.as_bytes(),
        extra_field.as_bytes(),
        &serde_json::to_vec(&doubled).expect("json"),
        &serde_json::to_vec(&index).expect("json"),
    ] {
        assert!(parse(bad).is_none(), "{}", String::from_utf8_lossy(bad));
        assert_eq!(stale(Some(bad), &observed(&ledgers)).len(), 3);
    }
}

/// Ways an index can go wrong between writes.
#[derive(Clone, Debug)]
enum Harm {
    Drop(usize),
    Version(usize),
    Facts(usize),
    Bogus,
    Truncate(usize),
    Byte(usize, u8),
}

fn harm() -> impl Strategy<Value = Harm> {
    prop_oneof![
        (0..3usize).prop_map(Harm::Drop),
        (0..3usize).prop_map(Harm::Version),
        (0..3usize).prop_map(Harm::Facts),
        Just(Harm::Bogus),
        (0..400usize).prop_map(Harm::Truncate),
        (0..400usize, any::<u8>()).prop_map(|(i, b)| Harm::Byte(i, b)),
    ]
}

fn apply(index: &[u8], harms: &[Harm]) -> Vec<u8> {
    let mut bytes = index.to_vec();
    for h in harms {
        if let Some(mut parsed) = parse(&bytes) {
            let n = parsed.families.len().max(1);
            match h {
                Harm::Drop(i) if !parsed.families.is_empty() => {
                    parsed.families.remove(i % n);
                }
                Harm::Version(i) if !parsed.families.is_empty() => {
                    parsed.families[i % n].ledger_version = "stale".to_owned();
                }
                Harm::Facts(i) if !parsed.families.is_empty() => {
                    if let Some(f) = parsed.families[i % n].facts.as_mut() {
                        f.name = "Wrong name".to_owned();
                        f.sent += 7;
                    }
                }
                Harm::Bogus => parsed.families.push(Indexed {
                    family_id: Id::parse(&"f".repeat(32)).expect("id"),
                    ledger_version: "1".to_owned(),
                    facts: None,
                }),
                _ => {}
            }
            bytes = encode(&parsed);
        }
        match h {
            Harm::Truncate(at) => bytes.truncate(*at % (bytes.len() + 1)),
            Harm::Byte(at, b) if !bytes.is_empty() => {
                let i = *at % bytes.len();
                bytes[i] = *b;
            }
            _ => {}
        }
    }
    bytes
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    /// Whatever happened to the index, refreshing it with the ledgers it asks
    /// for gives exactly what a full rebuild gives, except that facts edited
    /// by hand under a ledger version that still matches are believed (the
    /// version is the only thing compared, by design).
    #[test]
    fn a_damaged_index_refreshes_to_a_rebuild(harms in proptest::collection::vec(harm(), 1..5)) {
        let ledgers = ledgers();
        let rebuilt = full(&ledgers);
        let damaged = apply(&rebuilt.index, &harms);
        let seen = observed(&ledgers);
        let wanted = stale(Some(&damaged), &seen);
        let given: Vec<(Id, &[u8])> = borrowed(&ledgers)
            .into_iter()
            .filter(|(f, _)| wanted.contains(f))
            .collect();
        let refreshed = refresh(Some(&damaged), &seen, &given).expect("refreshes");
        let hand_edited = harms.iter().any(|h| matches!(h, Harm::Facts(_)));
        if !hand_edited {
            prop_assert_eq!(&refreshed.library, &rebuilt.library);
            prop_assert_eq!(&refreshed.index, &rebuilt.index);
        }
        prop_assert_eq!(refreshed.changed, damaged != refreshed.index);
        // Either way the family set is the ledgers' set.
        let families: Vec<Id> = parse(&refreshed.index)
            .expect("parses")
            .families
            .into_iter()
            .map(|e| e.family_id)
            .collect();
        prop_assert_eq!(families, seen.keys().cloned().collect::<Vec<_>>());
    }
}
