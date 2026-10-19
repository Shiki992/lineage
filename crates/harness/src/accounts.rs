// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Whole accounts for "My documents", pressed through the panel the way a
//! person would make them, for the scenario tests, `cargo xtask accounts` and
//! the family app's Playwright runs. Every name and every word is invented.
//!
//! - [`three_families`]: a resume with tailored versions, copies sent and files
//!   that came back; a cover letter; and a pitch deck with a version for an
//!   investor and copies sent. One version's file has moved, and the library
//!   index is out of date (missing the deck, and the letter's last copy).
//! - [`damaged`]: a family whose ledger was cut off part-way through a line.
//! - [`large`]: 300 checkpoints and 50 copies sent.

use lineage::blob::Blob;
use lineage::event::{self, CleanSummary, Event, MatchedBy, RECORD_VERSION, Record};
use lineage::state::FamilyState;
use lineage::{Id, IdSource, SequentialIds, Timestamp};
use lineage_core::Package;
use lineage_core::merge::Decision;
use lineage_testkit::decks;
use lineage_testkit::pptx::{self as decks_builder, Placed, Shape as DeckShape, p as deck_p};
use lineage_testkit::resume::{self, Line};
use lineage_testkit::returned::{cover_letter_v1_lines, cover_letter_v2_lines, light_edits};

use crate::storage::Tamper;
use crate::{HarnessError, Panel, Result, Storage, StorageError};

/// 2026-10-15T09:30:00Z.
pub const START: Timestamp = Timestamp(1_792_056_600_000);

pub const MASTER: &str = "Resume.docx";
pub const DATA: &str = "Tailored versions/Resume – data roles.docx";
pub const PLATFORM: &str = "Tailored versions/Resume – platform roles.docx";
pub const OLD: &str = "Tailored versions/Resume – old.docx";
/// Where the platform version's file was moved to, by hand.
pub const PLATFORM_MOVED: &str = "Tailored versions/Platform roles (renamed).docx";
pub const LETTER: &str = "Cover letter.docx";
pub const DECK: &str = "Pitch deck.pptx";
pub const DECK_NORTHGATE: &str = "Tailored versions/Pitch deck – Northgate Capital.pptx";

/// Someone edits a file in Word: its body becomes these lines; its ids stay.
pub fn edit<S: Storage>(storage: &mut S, from: &str, to: &str, lines: &[Line]) -> Result<()> {
    let source =
        Package::read(&resume::package("t", lines).to_bytes()).map_err(lineage::Error::from)?;
    let mut package = Package::read(&storage.read_file(from)?).map_err(lineage::Error::from)?;
    package.set_part(
        "word/document.xml",
        source
            .part("word/document.xml")
            .ok_or_else(|| HarnessError::Panel("no document part".to_owned()))?
            .to_vec(),
    );
    storage.write_file(to, &package.to_bytes().map_err(lineage::Error::from)?)?;
    Ok(())
}

/// Appends records no button makes yet (archiving, a deck's history), the
/// way a ledger written elsewhere would arrive.
pub fn append<S: Storage>(
    storage: &mut S,
    family: &Id,
    events: Vec<(Timestamp, Event)>,
) -> Result<()> {
    let (bytes, version) = storage.read_ledger(family)?;
    let state = if bytes.is_empty() {
        None
    } else {
        Some(FamilyState::from_records(&event::decode(&bytes)?)?)
    };
    let mut seq = state.map_or(1, |s| s.next_seq());
    let records: Vec<Record> = events
        .into_iter()
        .map(|(at, event)| {
            let record = Record {
                v: RECORD_VERSION,
                seq,
                at,
                event,
            };
            seq += 1;
            record
        })
        .collect();
    storage.append_ledger(family, version, &event::encode(&records)?)?;
    Ok(())
}

fn family_of<S: Storage>(panel: &Panel<S>) -> Result<Id> {
    Ok(panel.state()?.family_id)
}

/// The resume family: master v1 → v3, three tailored versions (one archived),
/// master changes brought into one and declined in another, a milestone, four
/// copies sent, and two that came back.
fn resume_family<S: Storage>(panel: &mut Panel<S>) -> Result<Id> {
    panel.storage.write_file(
        MASTER,
        &resume::package("Avery Quinn resume", &resume::master_v1_lines()).to_bytes(),
    )?;
    panel.start_tracking(MASTER)?;
    let family = family_of(panel)?;
    panel.make_tailored_version(DATA)?;
    panel.make_tailored_version(PLATFORM)?;
    panel.make_tailored_version(OLD)?;
    edit(
        &mut panel.storage,
        DATA,
        DATA,
        &resume::tailored_data_lines(),
    )?;
    panel.save_checkpoint(DATA, "Tailored for data roles")?;
    edit(
        &mut panel.storage,
        MASTER,
        MASTER,
        &resume::master_v2_lines(),
    )?;
    panel.save_checkpoint(MASTER, "New job at Corvia")?;
    panel.offer_to_versions()?;
    let cards: Vec<String> = panel
        .updates(DATA)?
        .groups
        .iter()
        .flat_map(|g| g.view.changes.iter().map(|c| c.id.clone()))
        .collect();
    panel.bring_in(DATA, &cards.iter().map(String::as_str).collect::<Vec<_>>())?;
    let cards: Vec<String> = panel
        .updates(PLATFORM)?
        .groups
        .iter()
        .flat_map(|g| g.view.changes.iter().map(|c| c.id.clone()))
        .collect();
    panel.decline(
        PLATFORM,
        &cards.iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    let state = panel.state()?;
    let v2 = state
        .latest(&state.master()?.id)
        .map(|c| c.id.clone())
        .ok_or_else(|| HarnessError::Panel("no v2".to_owned()))?;
    panel.mark_milestone(&family, &v2, Some("Sent to ten companies"))?;
    let mentor = panel.send_a_copy(MASTER, "Mentor", Some("Feedback"), true)?;
    let northgate = panel.send_a_copy(DATA, "Northgate Analytics", Some("Data engineer"), true)?;
    panel.send_a_copy(PLATFORM, "Harbour Bank", None, false)?;
    edit(
        &mut panel.storage,
        MASTER,
        MASTER,
        &resume::master_v3_lines(),
    )?;
    panel.save_checkpoint(MASTER, "Corvia title")?;
    panel.send_a_copy(MASTER, "Pellow Ventures", Some("Head of data"), true)?;

    // The mentor's copy comes back edited; two changes kept, the rest skipped.
    let back = "Resume - comments from Sam.docx";
    edit(
        &mut panel.storage,
        &mentor,
        back,
        &light_edits(&resume::master_v2_lines()),
    )?;
    let (found, _) = panel.a_file_came_back(back)?;
    let returned = panel.record_returned(back, &found.candidates[0], MatchedBy::Ids)?;
    let review = panel.returned_review(MASTER, &returned)?;
    let decisions: Vec<(String, Decision)> = review
        .view
        .changes
        .iter()
        .enumerate()
        .map(|(i, c)| {
            (
                c.id.clone(),
                if i < 2 {
                    Decision::BringIn
                } else {
                    Decision::Decline
                },
            )
        })
        .collect();
    let decisions: Vec<(&str, Decision)> =
        decisions.iter().map(|(c, d)| (c.as_str(), *d)).collect();
    panel.keep_and_skip(MASTER, &returned, &decisions)?;
    // Northgate's comes back unchanged.
    let unchanged = "Resume – data roles (from Northgate).docx";
    let bytes = panel.storage.read_file(&northgate)?;
    panel.storage.write_file(unchanged, &bytes)?;
    let (found, _) = panel.a_file_came_back(unchanged)?;
    panel.record_returned(unchanged, &found.candidates[0], MatchedBy::Ids)?;

    let old = state
        .nodes
        .iter()
        .find(|n| n.storage_ref == OLD)
        .map(|n| n.id.clone())
        .ok_or_else(|| HarnessError::Panel("no old version".to_owned()))?;
    let at = panel.now();
    append(
        &mut panel.storage,
        &family,
        vec![(at, Event::NodeArchived { node_id: old })],
    )?;
    Ok(family)
}

/// The cover letter: two checkpoints and one copy sent.
fn letter_family<S: Storage>(panel: &mut Panel<S>) -> Result<Id> {
    panel.storage.write_file(
        LETTER,
        &resume::package("Avery Quinn cover letter", &cover_letter_v1_lines()).to_bytes(),
    )?;
    panel.start_tracking(LETTER)?;
    edit(&mut panel.storage, LETTER, LETTER, &cover_letter_v2_lines())?;
    panel.save_checkpoint(LETTER, "For Northgate")?;
    panel.send_a_copy(LETTER, "Northgate Analytics", None, true)?;
    family_of(panel)
}

/// A pitch deck's history, pressed through the deck panel twenty days
/// earlier than the rest: tracked, a version for Northgate Capital that takes
/// the new traction numbers, a copy sent to each investor, and a team slide.
fn deck_family<S: Storage>(panel: &mut Panel<S>) -> Result<Id> {
    let back_then = panel.now();
    panel.set_now(Timestamp(START.0 - 20 * 86_400_000));
    panel
        .storage
        .write_file(DECK, &decks::pitch_deck().to_bytes())?;
    panel.start_tracking(DECK)?;
    let family = family_of(panel)?;
    panel.make_tailored_version(DECK_NORTHGATE)?;
    edit_deck(
        &mut panel.storage,
        DECK,
        &decks::edits::traction_update(decks::pitch_deck()),
    )?;
    panel.save_checkpoint(DECK, "New traction numbers")?;
    panel.update_versions(&[DECK_NORTHGATE], &[])?;
    panel.send_a_copy(
        DECK_NORTHGATE,
        "Northgate Capital",
        Some("Seed round"),
        true,
    )?;
    let mut team = decks::edits::traction_update(decks::pitch_deck());
    team.slide_mut(decks::ids::TEAM).shapes[0] =
        Placed::from(DeckShape::Title(vec![deck_p("Team and advisors")]));
    edit_deck(&mut panel.storage, DECK, &team)?;
    panel.save_checkpoint(DECK, "Team slide")?;
    panel.send_a_copy(DECK, "Pellow Ventures", Some("Seed round"), true)?;
    panel.set_now(back_then);
    Ok(family)
}

/// Someone edits a deck in PowerPoint: it becomes `built`; its ids stay.
pub fn edit_deck<S: Storage>(
    storage: &mut S,
    file: &str,
    built: &decks_builder::Deck,
) -> Result<()> {
    let old = Package::read(&storage.read_file(file)?).map_err(lineage::Error::from)?;
    let ids = lineage_core::docx::props::read_ids(&old)
        .map_err(lineage::Error::from)?
        .ok_or_else(|| HarnessError::Panel(format!("{file} is not tracked")))?;
    let mut package = Package::read(&built.to_bytes()).map_err(lineage::Error::from)?;
    lineage_core::docx::props::write_ids(&mut package, &ids).map_err(lineage::Error::from)?;
    storage.write_file(file, &package.to_bytes().map_err(lineage::Error::from)?)?;
    Ok(())
}

/// What [`three_families`] made.
pub struct ThreeFamilies {
    pub resume: Id,
    pub letter: Id,
    pub deck: Id,
}

/// The three-family account. The platform version's file is moved by hand
/// (so the page shows "File not found" and offers the moved file), and the
/// index is left as it was before the cover letter's last change.
///
/// # Errors
/// When a button fails; never for these inputs.
pub fn three_families<S: Storage + Tamper>(panel: &mut Panel<S>) -> Result<ThreeFamilies> {
    let resume = resume_family(panel)?;
    let letter = letter_family(panel)?;
    let before = panel.storage.read_index()?;
    let deck = deck_family(panel)?;
    panel.open_family_of(LETTER)?;
    panel.send_a_copy(LETTER, "Harbour Bank", None, false)?;
    let moved = panel.storage.read_file(PLATFORM)?;
    panel.storage.write_file(PLATFORM_MOVED, &moved)?;
    remove(&mut panel.storage, PLATFORM)?;
    // The index as it was before the last write: out of date for the letter,
    // and missing the deck, whose history arrived from elsewhere.
    if let Some((bytes, _)) = before {
        let current = panel.storage.read_index()?.map(|(_, v)| v);
        match panel.storage.write_index(&bytes, current.as_deref()) {
            Ok(_) | Err(StorageError::Conflict) => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(ThreeFamilies {
        resume,
        letter,
        deck,
    })
}

/// Removes a file, as a person deleting or moving it would.
fn remove<S: Storage + Tamper>(storage: &mut S, name: &str) -> Result<()> {
    storage.remove_file(name)?;
    Ok(())
}

/// A family whose ledger stops part-way through its last line.
///
/// # Errors
/// When a button fails; never for these inputs.
pub fn damaged<S: Storage + Tamper>(panel: &mut Panel<S>) -> Result<Id> {
    let family = letter_family(panel)?;
    let (bytes, version) = panel.storage.read_ledger(&family)?;
    let cut = bytes.len() - 40;
    panel
        .storage
        .replace_ledger(&family, version, &bytes[..cut])?;
    Ok(family)
}

/// A large family: a master and four tailored versions with `checkpoints`
/// checkpoints between them, and `sent` copies sent. A few checkpoints are
/// pressed through the panel; the rest repeat the last snapshot, as quick
/// checkpoints with no edits would.
///
/// # Errors
/// When a button fails; never for these inputs.
pub fn large<S: Storage>(panel: &mut Panel<S>, checkpoints: usize, sent: usize) -> Result<Id> {
    panel.storage.write_file(
        MASTER,
        &resume::package("Avery Quinn resume", &resume::master_v1_lines()).to_bytes(),
    )?;
    panel.start_tracking(MASTER)?;
    let family = family_of(panel)?;
    let names = ["data roles", "platform roles", "analytics", "consulting"];
    for name in names {
        panel.make_tailored_version(&format!("Tailored versions/Resume – {name}.docx"))?;
    }
    edit(
        &mut panel.storage,
        MASTER,
        MASTER,
        &resume::master_v2_lines(),
    )?;
    panel.save_checkpoint(MASTER, "New job")?;
    let copy = panel.send_a_copy(MASTER, "Recruiter 1", None, true)?;
    let state = panel.state()?;
    let copy_blob = Blob::new(panel.storage.read_file(&copy)?);
    let nodes: Vec<Id> = state.nodes.iter().map(|n| n.id.clone()).collect();
    let mut last: Vec<Id> = nodes
        .iter()
        .map(|n| {
            state
                .latest(n)
                .map(|c| c.id.clone())
                .ok_or_else(|| HarnessError::Panel("no checkpoint".to_owned()))
        })
        .collect::<Result<_>>()?;
    let mut ids = SequentialIds::new(0xe0);
    let mut events = Vec::new();
    let mut at = START.0 + 3_600_000;
    for n in state.checkpoints.len()..checkpoints {
        let which = n % nodes.len();
        let parent = state
            .latest(&nodes[which])
            .ok_or_else(|| HarnessError::Panel("no parent".to_owned()))?;
        let id = ids.next_id();
        events.push((
            Timestamp(at),
            Event::CheckpointSaved {
                checkpoint_id: id.clone(),
                node_id: nodes[which].clone(),
                parent_ids: vec![last[which].clone()],
                snapshot: parent.snapshot.clone(),
                fingerprint: parent.fingerprint.clone(),
                note: format!("Draft {}", n + 1),
                author: None,
                changes: None,
                engine_version: lineage::ENGINE_VERSION.to_owned(),
            },
        ));
        last[which] = id;
        at += 3_600_000;
    }
    for n in 1..sent {
        events.push((
            Timestamp(at),
            Event::CopySent {
                sent_copy_id: ids.next_id(),
                from_checkpoint_id: last[n % last.len()].clone(),
                recipient: format!("Recruiter {}", n + 1),
                purpose: None,
                format: "docx".to_owned(),
                file: copy_blob.hash.clone(),
                id_embedded: false,
                cleaned: CleanSummary::default(),
            },
        ));
        at += 3_600_000;
    }
    // Every quick checkpoint repeats a snapshot that is stored, so compare works.
    append(&mut panel.storage, &family, events)?;
    Ok(family)
}

pub const MODEL: &str = "Startup model.xlsx";
pub const MODEL_NORTHWIND: &str = "Tailored versions/Startup model – Northwind.xlsx";
pub const MODEL_HARBOR: &str = "Tailored versions/Startup model – Harbor Lane.xlsx";
pub const MODEL_CAME_BACK: &str = "Startup model – Harbor Lane comments.xlsx";
pub const MACROS: &str = "Model with macros.xlsm";
pub const MACROS_VERSION: &str = "Tailored versions/Model with macros – Seed.xlsm";

/// Someone edits a workbook in Excel: it becomes `built`, and keeps its ids.
///
/// # Errors
/// When the file isn't a tracked workbook.
pub fn edit_workbook<S: Storage>(
    storage: &mut S,
    file: &str,
    built: &lineage_testkit::xlsx::Workbook,
) -> Result<()> {
    let bytes = storage.read_file(file)?;
    let next = with_ids_of(&bytes, built)?;
    storage.write_file(file, &next)?;
    Ok(())
}

fn with_ids_of(file: &[u8], built: &lineage_testkit::xlsx::Workbook) -> Result<Vec<u8>> {
    let ids =
        lineage_core::docx::props::read_ids(&Package::read(file).map_err(lineage::Error::from)?)
            .map_err(lineage::Error::from)?
            .ok_or_else(|| HarnessError::Panel("the workbook isn't tracked".to_owned()))?;
    let mut package = Package::read(&built.to_bytes()).map_err(lineage::Error::from)?;
    lineage_core::docx::props::write_ids(&mut package, &ids).map_err(lineage::Error::from)?;
    Ok(package.to_bytes().map_err(lineage::Error::from)?)
}

/// What [`workbooks`] made.
pub struct Workbooks {
    pub model: Id,
    pub macros: Id,
}

/// The workbook account: the founder's financial model with versions for
/// Northwind Angels and Harbor Lane Capital, the master's new price brought
/// into Northwind's, a copy sent to Harbor Lane that came back with a new
/// churn rate (kept in their version); and a model with macros and one
/// version, kept as `.xlsm`.
///
/// # Errors
/// When a button fails; never for these inputs.
pub fn workbooks<S: Storage>(panel: &mut Panel<S>) -> Result<Workbooks> {
    use lineage_testkit::workbooks::{self as wb, Assumptions};

    panel
        .storage
        .write_file(MODEL, &wb::model_master().to_bytes())?;
    panel.start_tracking(MODEL)?;
    let model = panel.state()?.family_id;
    panel.make_tailored_version(MODEL_NORTHWIND)?;
    panel.make_tailored_version(MODEL_HARBOR)?;
    edit_workbook(&mut panel.storage, MODEL_NORTHWIND, &wb::model_northwind())?;
    panel.save_checkpoint(MODEL_NORTHWIND, "Northwind's seat plan")?;
    let copy = panel.send_a_copy(MODEL_HARBOR, "Harbor Lane Capital", Some("Series A"), true)?;
    edit_workbook(&mut panel.storage, MODEL, &wb::model_price_update())?;
    panel.save_checkpoint(MODEL, "New price")?;
    panel.offer_to_versions()?;
    let ids: Vec<String> = panel
        .updates(MODEL_NORTHWIND)?
        .groups
        .iter()
        .flat_map(|g| g.view.changes.iter().map(|c| c.id.clone()))
        .collect();
    let cards: Vec<(&str, Decision)> = ids
        .iter()
        .map(|id| (id.as_str(), Decision::BringIn))
        .collect();
    panel.bring_into_workbook(MODEL_NORTHWIND, &cards, crate::fake_excel)?;

    // Harbor Lane's copy comes back with a different churn rate.
    let sent = panel.storage.read_file(&copy)?;
    let theirs = wb::financial_model(&Assumptions {
        churn: "0.03".to_owned(),
        ..Assumptions::default()
    });
    panel
        .storage
        .write_file(MODEL_CAME_BACK, &with_ids_of(&sent, &theirs)?)?;
    let (found, _) = panel.a_file_came_back(MODEL_CAME_BACK)?;
    let candidate = found
        .candidates
        .first()
        .cloned()
        .ok_or_else(|| HarnessError::Panel("the copy wasn't recognised".to_owned()))?;
    let returned = panel.record_returned(MODEL_CAME_BACK, &candidate, MatchedBy::Ids)?;
    let review = panel.returned_review(MODEL_HARBOR, &returned)?;
    let kept: Vec<String> = review
        .view
        .workbook
        .iter()
        .flat_map(|w| {
            w.groups
                .iter()
                .flat_map(|g| g.cards.iter().flat_map(|c| c.ids.clone()))
        })
        .collect();
    let keep: Vec<(&str, Decision)> = kept
        .iter()
        .map(|id| (id.as_str(), Decision::BringIn))
        .collect();
    panel.keep_into_workbook(MODEL_HARBOR, &returned, &keep, crate::fake_excel)?;

    panel
        .storage
        .write_file(MACROS, &wb::model_xlsm().to_bytes())?;
    panel.start_tracking(MACROS)?;
    let macros = panel.state()?.family_id;
    panel.make_tailored_version(MACROS_VERSION)?;
    Ok(Workbooks { model, macros })
}
