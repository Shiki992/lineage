// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Recognising files that come back: precision and recall on a synthetic set
//! (decision 0013).
//!
//! Three families are built through the commands: Avery's resume (a master,
//! two tailored versions and six sent copies, two of them from one checkpoint),
//! a cover letter (two copies) and meeting notes (never sent). Every sent copy
//! then comes back in every way the fixtures cover: with its ids or without,
//! saved by Word, exported by Google Docs (an imitation) or heavily reformatted,
//! with no edits, light edits or a rewrite. The real LibreOffice re-saves in
//! `fixtures/docx/returned/libreoffice/` come back too, with and without ids.
//! Negatives are files Lineage has never seen: the synthetic fixtures and a
//! stranger's resume on the same template.
//!
//! The report prints with `--nocapture`. The assertions are the rules:
//! no wrong automatic match ever, no automatic match on a negative, and the
//! right copy always found (automatically or among the choices) when the edits
//! are light.

use std::collections::BTreeMap;
use std::path::PathBuf;

use lineage::blob::MemoryBlobs;
use lineage::commands::{self, Context, Outcome};
use lineage::event::{self, Record};
use lineage::identity::content_match;
use lineage::returned::{Family, LEAD, Recognition, Verdict, recognise};
use lineage::state::FamilyState;
use lineage::{Id, SequentialIds, Timestamp};
use lineage_core::clean::clean_copy;
use lineage_core::diff::diff;
use lineage_core::docx::props::{LineageIds, write_ids};
use lineage_core::{Docx, Package};
use lineage_testkit::fixtures;
use lineage_testkit::resume::{self, Line};
use lineage_testkit::returned::{
    self as kit, Edits, LIBREOFFICE, LIBREOFFICE_FAMILY, LIBREOFFICE_SENT, Saved, Source,
};

/// One family, kept in memory the way the harness keeps it on disk.
struct Book {
    ledger: Vec<Record>,
    state: Option<FamilyState>,
    blobs: MemoryBlobs,
}

struct World {
    ids: SequentialIds,
    now: Timestamp,
    books: Vec<Book>,
}

impl World {
    fn new() -> Self {
        Self {
            ids: SequentialIds::new(3),
            now: Timestamp(1_792_056_600_000),
            books: Vec::new(),
        }
    }

    fn ctx(&mut self) -> Context<'_> {
        self.now = Timestamp(self.now.0 + 86_400_000);
        Context {
            now: self.now,
            ids: &mut self.ids,
            author: None,
        }
    }

    fn take(&mut self, book: usize, outcome: Outcome) -> Outcome {
        let b = &mut self.books[book];
        for blob in &outcome.blobs {
            b.blobs.add(blob.clone());
        }
        b.ledger.extend(outcome.records.iter().cloned());
        let bytes = event::encode(&b.ledger).expect("encodes");
        b.state = Some(
            FamilyState::from_records(&event::decode(&bytes).expect("decodes")).expect("folds"),
        );
        outcome
    }

    fn state(&self, book: usize) -> FamilyState {
        self.books[book].state.clone().expect("started")
    }

    /// Starts a family; returns its book and the master's file.
    fn start(&mut self, name: &str, lines: &[Line]) -> (usize, Vec<u8>) {
        let bytes = resume::package("t", lines).to_bytes();
        let empty = MemoryBlobs::default();
        let mut ctx = self.ctx();
        let (_, outcome) =
            commands::start_tracking(&mut ctx, &bytes, name, name, &empty).expect("starts");
        self.books.push(Book {
            ledger: Vec::new(),
            state: None,
            blobs: MemoryBlobs::default(),
        });
        let book = self.books.len() - 1;
        let file = self.take(book, outcome).file.expect("file");
        (book, file)
    }

    fn master(&self, book: usize) -> Id {
        self.state(book).master().expect("master").id.clone()
    }

    fn version(&mut self, book: usize, name: &str) -> (Id, Vec<u8>) {
        let state = self.state(book);
        let blobs = self.books[book].blobs.clone();
        let mut ctx = self.ctx();
        let outcome =
            commands::make_tailored_version(&mut ctx, &state, name, name, &blobs).expect("makes");
        let outcome = self.take(book, outcome);
        (
            outcome.created.expect("node"),
            outcome.new_file.expect("file").bytes,
        )
    }

    fn save(&mut self, book: usize, node: &Id, file: &[u8], lines: &[Line]) -> Vec<u8> {
        let edited = with_body(file, lines);
        let state = self.state(book);
        let blobs = self.books[book].blobs.clone();
        let mut ctx = self.ctx();
        let outcome =
            commands::save_checkpoint(&mut ctx, &state, node, &edited, "", &blobs).expect("saves");
        self.take(book, outcome).file.unwrap_or(edited)
    }

    fn send(&mut self, book: usize, node: &Id, file: &[u8], to: &str, id: bool) -> Sent {
        let state = self.state(book);
        let blobs = self.books[book].blobs.clone();
        let mut ctx = self.ctx();
        let outcome =
            commands::send_copy(&mut ctx, &state, node, file, to, None, id, &blobs).expect("sends");
        let outcome = self.take(book, outcome);
        let sent = outcome.created.expect("sent");
        let state = self.state(book);
        Sent {
            family: state.family_id.clone(),
            checkpoint: state.sent_copy(&sent).expect("recorded").from.clone(),
            sent,
            bytes: outcome.new_file.expect("copy").bytes,
        }
    }

    fn recognise(&self, bytes: &[u8]) -> Recognition {
        let families: Vec<Family<'_>> = self
            .books
            .iter()
            .map(|b| Family {
                state: b.state.as_ref().expect("started"),
                blobs: &b.blobs,
            })
            .collect();
        recognise(bytes, &families).expect("recognises")
    }
}

/// The tailored version for platform roles: the Corvia title and the summary.
fn platform_lines() -> Vec<Line> {
    let mut lines = resume::tailored_same_para_lines();
    lines[3] = Line::Plain(
        "Platform engineer who keeps shared data services fast, observable and cheap to run."
            .to_owned(),
    );
    lines
}

/// The file with its body replaced, everything else (the ids too) as it was.
fn with_body(file: &[u8], lines: &[Line]) -> Vec<u8> {
    let source = Package::read(&resume::package("t", lines).to_bytes()).expect("reads");
    let mut package = Package::read(file).expect("reads");
    package.set_part(
        "word/document.xml",
        source.part("word/document.xml").expect("doc").to_vec(),
    );
    package.to_bytes().expect("bytes")
}

struct Sent {
    family: Id,
    checkpoint: Id,
    sent: Id,
    bytes: Vec<u8>,
}

/// What a returned file truly is.
#[derive(Clone, PartialEq, Eq)]
struct Truth {
    family: Id,
    checkpoint: Id,
    sent: Id,
    /// Another sent copy has the same text, or differs by fewer than
    /// [`LEAD`] words, and no ids tell them apart: the right answer is a choice.
    ambiguous: bool,
}

struct Case {
    kind: String,
    edits: Edits,
    bytes: Vec<u8>,
    truth: Option<Truth>,
}

#[derive(Default, Clone, Copy)]
struct Tally {
    certain_right: usize,
    certain_wrong: usize,
    choice_with: usize,
    choice_without: usize,
    unknown: usize,
}

impl Tally {
    fn total(&self) -> usize {
        self.certain_right
            + self.certain_wrong
            + self.choice_with
            + self.choice_without
            + self.unknown
    }
    fn add(&mut self, other: &Self) {
        self.certain_right += other.certain_right;
        self.certain_wrong += other.certain_wrong;
        self.choice_with += other.choice_with;
        self.choice_without += other.choice_without;
        self.unknown += other.unknown;
    }
}

fn judge(found: &Recognition, truth: Option<&Truth>) -> Tally {
    let mut t = Tally::default();
    let hit = |c: &lineage::returned::Candidate| {
        truth.is_some_and(|tr| {
            c.family == tr.family
                && c.checkpoint == tr.checkpoint
                && c.sent.as_ref() == Some(&tr.sent)
        })
    };
    match found.verdict {
        Verdict::Certain if found.candidates.iter().any(hit) => t.certain_right += 1,
        Verdict::Certain => t.certain_wrong += 1,
        Verdict::Choose if found.candidates.iter().any(hit) => t.choice_with += 1,
        Verdict::Choose => t.choice_without += 1,
        Verdict::Unknown => t.unknown += 1,
    }
    t
}

fn libreoffice_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/docx/returned/libreoffice")
}

fn google_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/docx/returned/google")
}

fn rewrite_ids(bytes: &[u8], family: &Id, sent: &Id) -> Vec<u8> {
    let mut package = Package::read(bytes).expect("reads");
    write_ids(
        &mut package,
        &LineageIds {
            family: family.to_string(),
            checkpoint: sent.to_string(),
        },
    )
    .expect("writes ids");
    package.to_bytes().expect("bytes")
}

fn strip(bytes: &[u8]) -> Vec<u8> {
    clean_copy(bytes, None).expect("cleans").0
}

/// The families, the sent copies, and every returned file with its truth.
fn build() -> (World, Vec<Case>) {
    let mut w = World::new();

    // Avery's resume.
    let (resume_book, master_file) = w.start("Resume.docx", &resume::master_v1_lines());
    let master = w.master(resume_book);
    let (data, data_file) = w.version(resume_book, "Resume – data roles.docx");
    let data_file = w.save(
        resume_book,
        &data,
        &data_file,
        &resume::tailored_data_lines(),
    );
    let (platform, platform_file) = w.version(resume_book, "Resume – platform roles.docx");
    let platform_file = w.save(resume_book, &platform, &platform_file, &platform_lines());
    let s_mentor = w.send(resume_book, &master, &master_file, "Mentor", true);
    let s_northgate = w.send(resume_book, &data, &data_file, "Northgate Analytics", true);
    let s_harbour = w.send(resume_book, &data, &data_file, "Harbour Bank", false);
    let master_file = w.save(
        resume_book,
        &master,
        &master_file,
        &resume::master_v2_lines(),
    );
    let s_recruiter = w.send(resume_book, &master, &master_file, "Recruiter", false);
    let master_file = w.save(
        resume_book,
        &master,
        &master_file,
        &resume::master_v3_lines(),
    );
    let s_pellow = w.send(resume_book, &master, &master_file, "Pellow Ventures", false);
    let s_cobalt = w.send(resume_book, &platform, &platform_file, "Cobalt Labs", false);

    // The cover letter.
    let (letter_book, letter_file) = w.start("Cover letter.docx", &kit::cover_letter_v1_lines());
    let letter = w.master(letter_book);
    let s_letter_ng = w.send(
        letter_book,
        &letter,
        &letter_file,
        "Northgate Analytics",
        true,
    );
    let letter_file = w.save(
        letter_book,
        &letter,
        &letter_file,
        &kit::cover_letter_v2_lines(),
    );
    let s_letter_hb = w.send(letter_book, &letter, &letter_file, "Harbour Bank", false);

    // Meeting notes, never sent.
    w.start("Garden notes.docx", &kit::meeting_notes_lines());

    let copies: Vec<(&Sent, Vec<Line>)> = vec![
        (&s_mentor, resume::master_v1_lines()),
        (&s_northgate, resume::tailored_data_lines()),
        (&s_harbour, resume::tailored_data_lines()),
        (&s_recruiter, resume::master_v2_lines()),
        (&s_pellow, resume::master_v3_lines()),
        (&s_cobalt, platform_lines()),
        (&s_letter_ng, kit::cover_letter_v1_lines()),
        (&s_letter_hb, kit::cover_letter_v2_lines()),
    ];
    // Copies whose text is the same as another copy's, or nearly: Northgate
    // and Harbour Bank got the same checkpoint, and some checkpoints differ by
    // a single word. Without ids these can only be a choice.
    let all: Vec<&Sent> = copies.iter().map(|(s, _)| *s).collect();
    let near: Vec<Id> = all
        .iter()
        .filter(|s| {
            let a = lineage_core::doc::Doc::open(&s.bytes).expect("opens");
            all.iter().any(|t| {
                t.sent != s.sent
                    && content_match(&a, &lineage_core::doc::Doc::open(&t.bytes).expect("opens"))
                        .changed
                        < LEAD
            })
        })
        .map(|s| s.sent.clone())
        .collect();
    let twins = |s: &Sent| near.contains(&s.sent);
    let mut cases = Vec::new();
    for (sent, lines) in &copies {
        for edits in [Edits::None, Edits::Light, Edits::Heavy] {
            let edited = edits.apply(lines);
            let had_ids = Package::read(&sent.bytes)
                .ok()
                .and_then(|p| lineage_core::docx::props::read_ids(&p).ok().flatten())
                .is_some();
            let truth = |ids_inside: bool| {
                Some(Truth {
                    family: sent.family.clone(),
                    checkpoint: sent.checkpoint.clone(),
                    sent: sent.sent.clone(),
                    ambiguous: !ids_inside && twins(sent),
                })
            };
            let word = with_body(&sent.bytes, &edited);
            if had_ids {
                cases.push(Case {
                    kind: "Word, ids kept".into(),
                    edits,
                    bytes: word.clone(),
                    truth: truth(true),
                });
            }
            cases.push(Case {
                kind: "Word, no ids".into(),
                edits,
                bytes: strip(&word),
                truth: truth(false),
            });
            cases.push(Case {
                kind: "Google export".into(),
                edits,
                bytes: kit::package("t", &edited, Saved::GoogleExport).to_bytes(),
                truth: truth(false),
            });
            let mut reformatted = kit::package("t", &edited, Saved::Reformatted);
            if had_ids {
                kit::with_ids(&mut reformatted, sent.family.as_str(), sent.sent.as_str());
            }
            cases.push(Case {
                kind: if had_ids {
                    "Reformatted, ids kept".into()
                } else {
                    "Reformatted, no ids".into()
                },
                edits,
                bytes: reformatted.to_bytes(),
                truth: truth(had_ids),
            });
        }
    }

    // The real LibreOffice re-saves and Google Docs downloads (made from the
    // same inputs; both apps keep the custom properties, so the ids survive).
    for (app, dir) in [
        ("LibreOffice", libreoffice_dir()),
        ("Google Docs", google_dir()),
    ] {
        for input in &LIBREOFFICE {
            let bytes = std::fs::read(dir.join(input.name)).expect("fixture exists");
            let sent = match input.source {
                Source::MasterV1 => &s_mentor,
                Source::TailoredData => &s_northgate,
                Source::CoverLetterV1 => &s_letter_ng,
            };
            let truth = |ids_inside: bool| {
                Some(Truth {
                    family: sent.family.clone(),
                    checkpoint: sent.checkpoint.clone(),
                    sent: sent.sent.clone(),
                    ambiguous: !ids_inside && twins(sent),
                })
            };
            if input.ids {
                cases.push(Case {
                    kind: format!("{app}, ids kept"),
                    edits: input.edits,
                    bytes: rewrite_ids(&bytes, &sent.family, &sent.sent),
                    truth: truth(true),
                });
            }
            cases.push(Case {
                kind: format!("{app}, no ids"),
                edits: input.edits,
                bytes: strip(&bytes),
                truth: truth(false),
            });
        }
    }

    // Negatives: files Lineage has never seen.
    let mut negatives: Vec<(&str, Vec<u8>)> = fixtures::all()
        .into_iter()
        .filter(|(name, _)| !name.starts_with("resume"))
        .map(|(name, p)| (name, p.to_bytes()))
        .collect();
    for saved in [Saved::Word, Saved::GoogleExport, Saved::Reformatted] {
        for edits in [Edits::None, Edits::Light] {
            negatives.push((
                "stranger's resume",
                kit::package("t", &edits.apply(&kit::stranger_lines()), saved).to_bytes(),
            ));
        }
    }
    for (_, bytes) in negatives {
        cases.push(Case {
            kind: "Never seen".into(),
            edits: Edits::None,
            bytes,
            truth: None,
        });
    }
    (w, cases)
}

#[test]
fn recognition_precision_and_recall() {
    let (world, cases) = build();
    let mut by_kind: BTreeMap<(String, String), Tally> = BTreeMap::new();
    let mut positives = Tally::default();
    let mut negatives = Tally::default();
    let mut light_misses = Vec::new();
    let mut ambiguous_certain = 0;
    let mut clear = Tally::default();
    for case in &cases {
        let found = world.recognise(&case.bytes);
        let tally = judge(&found, case.truth.as_ref());
        if std::env::var_os("LINEAGE_RECOGNITION_DEBUG").is_some() {
            let names: Vec<String> = found
                .candidates
                .iter()
                .map(|c| {
                    let to = world
                        .books
                        .iter()
                        .filter_map(|b| b.state.as_ref())
                        .find_map(|s| c.sent.as_ref().and_then(|x| s.sent_copy(x)))
                        .map_or("-".to_owned(), |s| s.recipient.clone());
                    format!("{to}:{}", c.score)
                })
                .collect();
            let truth = case.truth.as_ref().map_or("-".to_owned(), |t| {
                world
                    .books
                    .iter()
                    .filter_map(|b| b.state.as_ref())
                    .find_map(|s| s.sent_copy(&t.sent))
                    .map_or("?".to_owned(), |s| s.recipient.clone())
            });
            println!(
                "{:<22} {:<5} truth {truth:<20} {:?} {names:?}",
                case.kind,
                format!("{:?}", case.edits),
                found.verdict
            );
        }
        let edits = format!("{:?}", case.edits);
        by_kind
            .entry((case.kind.clone(), edits))
            .or_default()
            .add(&tally);
        match &case.truth {
            Some(truth) => {
                positives.add(&tally);
                if truth.ambiguous && found.verdict == Verdict::Certain {
                    ambiguous_certain += 1;
                }
                if !truth.ambiguous {
                    clear.add(&tally);
                }
                if case.edits != Edits::Heavy && tally.certain_right + tally.choice_with == 0 {
                    light_misses.push(format!("{} / {:?}: {:?}", case.kind, case.edits, found));
                }
            }
            None => negatives.add(&tally),
        }
    }

    println!("\nRecognising returned files: {} files", cases.len());
    println!(
        "{:<24} {:<6} {:>5} {:>8} {:>8} {:>8} {:>8} {:>8}",
        "kind", "edits", "files", "auto ok", "auto bad", "choice+", "choice-", "unknown"
    );
    for ((kind, edits), t) in &by_kind {
        println!(
            "{:<24} {:<6} {:>5} {:>8} {:>8} {:>8} {:>8} {:>8}",
            kind,
            edits,
            t.total(),
            t.certain_right,
            t.certain_wrong,
            t.choice_with,
            t.choice_without,
            t.unknown
        );
    }
    let pct = |a: usize, b: usize| {
        if b == 0 {
            100.0
        } else {
            100.0 * a as f64 / b as f64
        }
    };
    let precision = pct(
        positives.certain_right,
        positives.certain_right
            + positives.certain_wrong
            + negatives.certain_right
            + negatives.certain_wrong,
    );
    let recall_auto = pct(clear.certain_right, clear.total());
    let recall_found = pct(
        positives.certain_right + positives.choice_with,
        positives.total(),
    );
    println!(
        "\nPrecision of automatic matches: {precision:.1}% ({} right, {} wrong, {} on files never sent)",
        positives.certain_right,
        positives.certain_wrong,
        negatives.certain_right + negatives.certain_wrong
    );
    println!(
        "Recall, automatic, where no other copy has (nearly) the same text: {recall_auto:.1}% ({} of {})",
        clear.certain_right,
        clear.total()
    );
    println!(
        "Recall, automatic or among the choices: {recall_found:.1}% ({} of {})",
        positives.certain_right + positives.choice_with,
        positives.total()
    );
    println!(
        "Files never sent: {} unknown, {} shown as a choice, {} matched automatically",
        negatives.unknown,
        negatives.choice_with + negatives.choice_without,
        negatives.certain_right + negatives.certain_wrong
    );

    assert_eq!(positives.certain_wrong, 0, "a wrong automatic match");
    assert_eq!(
        negatives.certain_right + negatives.certain_wrong,
        0,
        "a file never sent matched automatically"
    );
    assert_eq!(ambiguous_certain, 0, "twin copies must be a choice");
    assert!(
        light_misses.is_empty(),
        "light edits not found:\n{}",
        light_misses.join("\n")
    );
    assert!(
        recall_auto >= RECALL_AUTO_FLOOR,
        "automatic recall {recall_auto:.1}%"
    );
    assert!(
        recall_found >= RECALL_FOUND_FLOOR,
        "found recall {recall_found:.1}%"
    );
}

/// Floors from the measurement recorded in decision 0013; raise them as the
/// matching improves, never lower them quietly.
const RECALL_AUTO_FLOOR: f64 = 80.0;
const RECALL_FOUND_FLOOR: f64 = 100.0;

#[test]
fn re_saving_elsewhere_changes_no_words() {
    // A copy opened and saved again in another app, with no edits, comes back
    // with no changes to the words: only formatting, if anything.
    let (world, _) = build();
    let state = world.books[0].state.as_ref().expect("resume");
    let mentor = state
        .sent
        .iter()
        .find(|s| s.recipient == "Mentor")
        .expect("mentor copy");
    let sent = world.books[0].blobs.0[&mentor.file].clone();
    let base = Docx::open(&sent).expect("opens");
    let content = |bytes: &[u8]| {
        diff(&base, &Docx::open(bytes).expect("opens"))
            .changes
            .into_iter()
            .filter(|c| !c.formatting_only)
            .map(|c| format!("{:?}: {:?} -> {:?}", c.kind, c.before_text, c.after_text))
            .collect::<Vec<_>>()
    };
    let lo =
        std::fs::read(libreoffice_dir().join("resume-unchanged-with-ids.docx")).expect("fixture");
    assert_eq!(content(&lo), Vec::<String>::new(), "LibreOffice");
    let google =
        std::fs::read(google_dir().join("resume-unchanged-with-ids.docx")).expect("fixture");
    assert_eq!(content(&google), Vec::<String>::new(), "Google Docs");
    for saved in [Saved::GoogleExport, Saved::Reformatted] {
        let bytes = kit::package("t", &resume::master_v1_lines(), saved).to_bytes();
        assert_eq!(content(&bytes), Vec::<String>::new(), "{saved:?}");
    }
    // LibreOffice and Google Docs keep custom properties, so the ids survive.
    for file in [&lo, &google] {
        let ids = lineage_core::docx::props::read_ids(&Package::read(file).expect("reads"))
            .expect("reads ids")
            .expect("has ids");
        assert_eq!(
            (ids.family.as_str(), ids.checkpoint.as_str()),
            (LIBREOFFICE_FAMILY, LIBREOFFICE_SENT)
        );
    }
}
