// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The engine as the task pane drives it: one method per thing the panel does,
//! in plain Rust so it is tested natively. `lib.rs` wraps each in a
//! `#[wasm_bindgen]` method and nothing more.
//!
//! Every method returns a [`Reply`]: a JSON envelope, `{"ok":true,"value":…}` or
//! `{"ok":false,"error":{"kind":…,"userMessage":…,"detail":…}}`, with any bytes
//! (the file, a new file, blobs to store) beside it. The ledger comes in as the
//! stored JSON Lines on every call, so the engine holds no family state of its
//! own between calls; only fetched blobs are kept (see [`crate::cache`]).

use lineage::blob::{Blob, BlobHash};
use lineage::commands::{self, Context, Outcome};
use lineage::event;
use lineage::returned;
use lineage::state::FamilyState;
use lineage::views;
use lineage::{Id, SeededIds, Timestamp};
use lineage_core::Package;
use lineage_core::docx::props;
use serde::Serialize;

use crate::cache::{BlobCache, DEFAULT_LIMIT, Recording};
use crate::dto::{
    CardDecision, CopyOfDto, FamilyDto, IdsDto, LedgerDto, MasterPanelDto, OpenFileDto, OutcomeDto,
    RecognitionDto, RecordArgs, ReturnReviewDto, SendArgs, SentListDto, UpdatesDto,
    VersionPanelDto, WhatChangedDto, WrittenDto, ledger_lines,
};

/// What a call returns: the JSON envelope and any bytes.
#[derive(Debug, Default)]
pub struct Reply {
    pub json: String,
    /// New bytes for the file the call acted on, or the bytes it asked for.
    pub file: Option<Vec<u8>>,
    /// A new file: a tailored version, or a sent copy.
    pub new_file: Option<Vec<u8>>,
    /// Blobs to store before appending the ledger lines.
    pub blobs: Vec<Blob>,
}

impl Reply {
    fn ok<T: Serialize>(value: &T) -> Self {
        #[derive(Serialize)]
        struct Ok<'a, T> {
            ok: bool,
            value: &'a T,
        }
        match serde_json::to_string(&Ok { ok: true, value }) {
            Ok(json) => Self {
                json,
                ..Self::default()
            },
            Err(e) => Self::failure(&Failure::BadInput(format!("could not encode: {e}"))),
        }
    }

    fn failure(failure: &Failure) -> Self {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Error<'a> {
            kind: &'a str,
            user_message: &'a str,
            detail: String,
            #[serde(skip_serializing_if = "Vec::is_empty")]
            hashes: Vec<&'a str>,
            /// The family the missing blobs are in, when the call looked in more
            /// than one.
            #[serde(skip_serializing_if = "Option::is_none")]
            family: Option<&'a str>,
        }
        #[derive(Serialize)]
        struct Envelope<'a> {
            ok: bool,
            error: Error<'a>,
        }
        let error = match failure {
            Failure::Lineage(e) => Error {
                kind: match e {
                    lineage::Error::Engine(_) => "engine",
                    lineage::Error::Ledger(_) | lineage::Error::MissingBlob(_) => "ledger",
                    lineage::Error::WrongFile(_) => "wrongFile",
                    lineage::Error::NotAllowed(_) => "notAllowed",
                },
                user_message: e.user_message(),
                detail: e.to_string(),
                hashes: Vec::new(),
                family: None,
            },
            Failure::Missing(hashes, family) => Error {
                kind: "missingBlobs",
                user_message: "Lineage is still fetching this file's history.",
                detail: format!("{} blobs to fetch", hashes.len()),
                hashes: hashes.iter().map(BlobHash::as_str).collect(),
                family: family.as_ref().map(Id::as_str),
            },
            Failure::BadInput(detail) => Error {
                kind: "badInput",
                user_message: "Lineage couldn't understand that request. Nothing was changed.",
                detail: detail.clone(),
                hashes: Vec::new(),
                family: None,
            },
        };
        let json = serde_json::to_string(&Envelope { ok: false, error }).unwrap_or_else(|_| {
            r#"{"ok":false,"error":{"kind":"badInput","userMessage":"Something went wrong. Nothing was changed.","detail":"could not encode the error"}}"#.to_owned()
        });
        Self {
            json,
            ..Self::default()
        }
    }
}

/// Why a call failed.
#[derive(Debug)]
enum Failure {
    Lineage(lineage::Error),
    /// Blobs the cache lacks; the host fetches them (from this family, when
    /// named) and calls again.
    Missing(Vec<BlobHash>, Option<Id>),
    BadInput(String),
}

impl From<lineage::Error> for Failure {
    fn from(e: lineage::Error) -> Self {
        Self::Lineage(e)
    }
}

type Result<T> = std::result::Result<T, Failure>;

/// Milliseconds since 1970 from JavaScript: a finite whole number in range.
fn timestamp(now_ms: f64) -> Result<Timestamp> {
    const MAX_SAFE: f64 = 9_007_199_254_740_991.0;
    if now_ms.is_finite() && now_ms.fract() == 0.0 && (0.0..=MAX_SAFE).contains(&now_ms) {
        // In range and whole, so the conversion is exact.
        #[allow(clippy::cast_possible_truncation)]
        Ok(Timestamp(now_ms as i64))
    } else {
        Err(Failure::BadInput(format!("not a time: {now_ms}")))
    }
}

fn id(text: &str) -> Result<Id> {
    Id::parse(text).map_err(Failure::BadInput)
}

fn json<T: serde::de::DeserializeOwned>(text: &str) -> Result<T> {
    serde_json::from_str(text).map_err(|e| Failure::BadInput(e.to_string()))
}

/// A name a person typed: not blank, not absurdly long.
fn name(text: &str) -> Result<&str> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 200 || trimmed.contains(char::is_control) {
        Err(Failure::BadInput(
            "a name must be 1 to 200 printable characters".to_owned(),
        ))
    } else {
        Ok(trimmed)
    }
}

fn fold(ledger: &[u8]) -> Result<FamilyState> {
    Ok(FamilyState::from_records(&event::decode(ledger)?)?)
}

/// One task pane's engine: fetched blobs, the id source and the person's name.
pub struct Session {
    cache: BlobCache,
    ids: SeededIds,
    author: Option<String>,
}

impl Session {
    #[must_use]
    pub fn new(seed: [u8; 32]) -> Self {
        Self::with_limit(seed, DEFAULT_LIMIT)
    }

    #[must_use]
    pub fn with_limit(seed: [u8; 32], limit: usize) -> Self {
        Self {
            cache: BlobCache::new(limit),
            ids: SeededIds::new(seed),
            author: None,
        }
    }

    /// The name recorded on new checkpoints, or none.
    pub fn set_author(&mut self, author: Option<String>) {
        self.author = author
            .map(|a| a.trim().to_owned())
            .filter(|a| !a.is_empty());
    }

    /// Adds a blob fetched from storage.
    pub fn add_blob(&mut self, hash: &str, bytes: Vec<u8>) -> Reply {
        let added = BlobHash::parse(hash)
            .map_err(|e| Failure::BadInput(e.to_string()))
            .and_then(|hash| self.cache.add(&hash, bytes).map_err(Failure::BadInput));
        match added {
            Ok(()) => Reply::ok(&serde_json::Value::Null),
            Err(e) => Reply::failure(&e),
        }
    }

    #[must_use]
    pub fn has_blob(&self, hash: &str) -> bool {
        BlobHash::parse(hash).is_ok_and(|h| self.cache.contains(&h))
    }

    #[must_use]
    pub fn cached_bytes(&self) -> usize {
        self.cache.bytes()
    }

    /// Runs a read-only view, reporting missing blobs.
    fn view<T: Serialize>(&self, f: impl FnOnce(&Recording<'_>) -> lineage::Result<T>) -> Reply {
        let source = Recording::new(&self.cache);
        let result = f(&source);
        Self::finish(result.map_err(Failure::from), &source)
            .map_or_else(|e| Reply::failure(&e), |v| Reply::ok(&v))
    }

    /// Turns a command's result into a reply, preferring "fetch these" over the
    /// error a missing blob caused.
    fn finish<T>(result: Result<T>, source: &Recording<'_>) -> Result<T> {
        match result {
            Ok(v) => Ok(v),
            Err(e) => {
                let missed = source.missed();
                if missed.is_empty() {
                    Err(e)
                } else {
                    Err(Failure::Missing(missed, None))
                }
            }
        }
    }

    /// Runs a command that changes the family, and packs its outcome.
    fn command(
        &mut self,
        now_ms: f64,
        f: impl FnOnce(&mut Context<'_>, &Recording<'_>) -> lineage::Result<Outcome>,
    ) -> Reply {
        self.command_in(now_ms, |ctx, source| Ok((f(ctx, source)?, None)))
    }

    /// As [`Session::command`], for a command that may start a family.
    fn command_in(
        &mut self,
        now_ms: f64,
        f: impl FnOnce(&mut Context<'_>, &Recording<'_>) -> lineage::Result<(Outcome, Option<Id>)>,
    ) -> Reply {
        let now = match timestamp(now_ms) {
            Ok(t) => t,
            Err(e) => return Reply::failure(&e),
        };
        let outcome = {
            let source = Recording::new(&self.cache);
            let mut ctx = Context {
                now,
                ids: &mut self.ids,
                author: self.author.clone(),
            };
            let result = f(&mut ctx, &source).map_err(Failure::from);
            Self::finish(result, &source)
        };
        let (outcome, family_id) = match outcome {
            Ok(o) => o,
            Err(e) => return Reply::failure(&e),
        };
        let lines = match ledger_lines(&outcome) {
            Ok(l) => l,
            Err(e) => return Reply::failure(&Failure::Lineage(e)),
        };
        for blob in &outcome.blobs {
            self.cache.insert(blob.hash.clone(), blob.bytes.clone());
        }
        let dto = OutcomeDto::new(&outcome, lines, family_id);
        let mut reply = Reply::ok(&dto);
        reply.file = outcome.file;
        reply.new_file = outcome.new_file.map(|f| f.bytes);
        reply.blobs = outcome.blobs;
        reply
    }

    /// The Lineage ids in a file, or null when it isn't tracked.
    #[must_use]
    pub fn read_ids(&self, file: &[u8]) -> Reply {
        let ids = Package::read(file)
            .map_err(lineage::Error::from)
            .and_then(|p| Ok(props::read_ids(&p)?));
        match ids {
            Ok(ids) => Reply::ok(&ids.as_ref().and_then(IdsDto::from_file)),
            Err(e) => Reply::failure(&Failure::Lineage(e)),
        }
    }

    /// The file with these ids written in; everything else untouched.
    #[must_use]
    pub fn write_ids(&self, file: &[u8], ids: &str) -> Reply {
        let written = json::<IdsDto>(ids).and_then(|ids| {
            let mut package = Package::read(file).map_err(lineage::Error::from)?;
            props::write_ids(&mut package, &ids.to_file()).map_err(lineage::Error::from)?;
            Ok(package.to_bytes().map_err(lineage::Error::from)?)
        });
        match written {
            Ok(bytes) => {
                let mut reply = Reply::ok(&serde_json::Value::Null);
                reply.file = Some(bytes);
                reply
            }
            Err(e) => Reply::failure(&e),
        }
    }

    /// Who is in the family.
    #[must_use]
    pub fn family(&self, ledger: &[u8]) -> Reply {
        match fold(ledger) {
            Ok(state) => Reply::ok(&FamilyDto::new(&state)),
            Err(e) => Reply::failure(&e),
        }
    }

    /// Which file of the family carries `checkpoint`.
    #[must_use]
    pub fn open_file(&self, ledger: &[u8], checkpoint: &str) -> Reply {
        let found = fold(ledger).and_then(|state| {
            let named = id(checkpoint)?;
            // A sent copy carries its own id (decision 0012): the file is a copy
            // that came back, made from that checkpoint.
            let copy = state.sent_copy(&named);
            let checkpoint = copy.map_or(named, |s| s.from.clone());
            let saved = state.checkpoint(&checkpoint).ok_or_else(|| {
                Failure::Lineage(lineage::Error::WrongFile(
                    "the checkpoint is not in this family".to_owned(),
                ))
            })?;
            let node = state.node(&saved.node).ok_or_else(|| {
                Failure::Lineage(lineage::Error::Ledger(
                    "a checkpoint without a file".to_owned(),
                ))
            })?;
            Ok(OpenFileDto {
                family_id: state.family_id.clone(),
                node_id: node.id.clone(),
                role: node.role.into(),
                display_name: node.display_name.clone(),
                storage_ref: node.storage_ref.clone(),
                checkpoint_label: state.label(&checkpoint),
                is_latest: node.checkpoints.last() == Some(&checkpoint) && copy.is_none(),
                copy_of: copy.map(|s| CopyOfDto {
                    sent_copy_id: s.id.clone(),
                    recipient: s.recipient.clone(),
                    date: s.at.day_label(),
                }),
            })
        });
        found.map_or_else(|e| Reply::failure(&e), |v| Reply::ok(&v))
    }

    /// The manifests and change lists worth fetching before the first panel.
    #[must_use]
    pub fn referenced_blobs(&self, ledger: &[u8]) -> Reply {
        match fold(ledger) {
            Ok(state) => {
                let hashes: Vec<String> = lineage::blob::referenced(&state)
                    .into_iter()
                    .filter(|h| !self.cache.contains(h))
                    .map(String::from)
                    .collect();
                Reply::ok(&hashes)
            }
            Err(e) => Reply::failure(&e),
        }
    }

    /// "Start tracking".
    pub fn start_tracking(
        &mut self,
        file: &[u8],
        display_name: &str,
        storage_ref: &str,
        now_ms: f64,
    ) -> Reply {
        let display_name = match name(display_name) {
            Ok(n) => n.to_owned(),
            Err(e) => return Reply::failure(&e),
        };
        self.command_in(now_ms, |ctx, source| {
            let (state, outcome) =
                commands::start_tracking(ctx, file, &display_name, storage_ref, source)?;
            Ok((outcome, Some(state.family_id)))
        })
    }

    /// "Save checkpoint".
    pub fn save_checkpoint(
        &mut self,
        ledger: &[u8],
        node: &str,
        file: &[u8],
        note: &str,
        now_ms: f64,
    ) -> Reply {
        let prepared = fold(ledger).and_then(|s| Ok((s, id(node)?)));
        let (state, node) = match prepared {
            Ok(p) => p,
            Err(e) => return Reply::failure(&e),
        };
        self.command(now_ms, |ctx, source| {
            commands::save_checkpoint(ctx, &state, &node, file, note, source)
        })
    }

    /// "Make a tailored version".
    pub fn make_tailored_version(
        &mut self,
        ledger: &[u8],
        display_name: &str,
        storage_ref: &str,
        now_ms: f64,
    ) -> Reply {
        let prepared = fold(ledger).and_then(|s| Ok((s, name(display_name)?.to_owned())));
        let (state, display_name) = match prepared {
            Ok(p) => p,
            Err(e) => return Reply::failure(&e),
        };
        self.command(now_ms, |ctx, source| {
            commands::make_tailored_version(ctx, &state, &display_name, storage_ref, source)
        })
    }

    /// "What changed" since the file's latest checkpoint.
    #[must_use]
    pub fn what_changed(&self, ledger: &[u8], node: &str, file: &[u8]) -> Reply {
        let prepared = fold(ledger).and_then(|s| Ok((s, id(node)?)));
        let (state, node) = match prepared {
            Ok(p) => p,
            Err(e) => return Reply::failure(&e),
        };
        self.view(|source| {
            commands::what_changed(&state, &node, file, source).map(|v| WhatChangedDto::from(&v))
        })
    }

    /// The master panel.
    #[must_use]
    pub fn master_panel(&self, ledger: &[u8]) -> Reply {
        let state = match fold(ledger) {
            Ok(s) => s,
            Err(e) => return Reply::failure(&e),
        };
        self.view(|source| {
            let master = state.master()?.id.clone();
            views::master_panel(&state, source).map(|p| MasterPanelDto::new(&p, &state, master))
        })
    }

    /// A tailored version's panel.
    #[must_use]
    pub fn version_panel(&self, ledger: &[u8], node: &str) -> Reply {
        let prepared = fold(ledger).and_then(|s| Ok((s, id(node)?)));
        let (state, node) = match prepared {
            Ok(p) => p,
            Err(e) => return Reply::failure(&e),
        };
        self.view(|source| {
            views::version_panel(&state, &node, source)
                .map(|p| VersionPanelDto::new(&p, node.clone()))
        })
    }

    /// "Offer it to …": `versions` is a JSON array of node ids.
    pub fn offer_to_versions(&mut self, ledger: &[u8], versions: &str, now_ms: f64) -> Reply {
        let prepared = fold(ledger).and_then(|s| Ok((s, json::<Vec<Id>>(versions)?)));
        let (state, versions) = match prepared {
            Ok(p) => p,
            Err(e) => return Reply::failure(&e),
        };
        self.command(now_ms, |ctx, source| {
            commands::offer_to_versions(ctx, &state, &versions, source)
        })
    }

    /// The updates a tailored version has been offered.
    #[must_use]
    pub fn updates(&self, ledger: &[u8], node: &str, file: &[u8]) -> Reply {
        let prepared = fold(ledger).and_then(|s| Ok((s, id(node)?)));
        let (state, node) = match prepared {
            Ok(p) => p,
            Err(e) => return Reply::failure(&e),
        };
        self.view(|source| {
            commands::updates(&state, &node, file, source).map(|u| UpdatesDto::from(&u))
        })
    }

    /// "Bring in" and "Decline": `cards` is a JSON array of `{card, decision}`.
    pub fn decide(
        &mut self,
        ledger: &[u8],
        node: &str,
        file: &[u8],
        cards: &str,
        now_ms: f64,
    ) -> Reply {
        let prepared = fold(ledger).and_then(|s| {
            let cards: Vec<CardDecision> = json(cards)?;
            Ok((
                s,
                id(node)?,
                cards
                    .into_iter()
                    .map(|c| (c.card, c.decision.into()))
                    .collect::<Vec<_>>(),
            ))
        });
        let (state, node, cards) = match prepared {
            Ok(p) => p,
            Err(e) => return Reply::failure(&e),
        };
        self.command(now_ms, |ctx, source| {
            commands::decide_cards(ctx, &state, &node, file, &cards, source)
        })
    }

    /// "Save copy and record it": `args` is `{recipient, purpose, embedId}`.
    pub fn send_copy(
        &mut self,
        ledger: &[u8],
        node: &str,
        file: &[u8],
        args: &str,
        now_ms: f64,
    ) -> Reply {
        let prepared = fold(ledger).and_then(|s| {
            let args: SendArgs = json(args)?;
            let recipient = name(&args.recipient)?.to_owned();
            let purpose = args
                .purpose
                .map(|p| p.trim().to_owned())
                .filter(|p| !p.is_empty());
            Ok((s, id(node)?, recipient, purpose, args.embed_id))
        });
        let (state, node, recipient, purpose, embed_id) = match prepared {
            Ok(p) => p,
            Err(e) => return Reply::failure(&e),
        };
        self.command(now_ms, |ctx, source| {
            commands::send_copy(
                ctx,
                &state,
                &node,
                file,
                &recipient,
                purpose.as_deref(),
                embed_id,
                source,
            )
        })
    }

    /// "What I've sent".
    #[must_use]
    pub fn sent_list(&self, ledger: &[u8], search: Option<&str>) -> Reply {
        match fold(ledger) {
            Ok(state) => Reply::ok(&SentListDto::from(&views::sent_list(&state, search))),
            Err(e) => Reply::failure(&e),
        }
    }

    /// "Open" on a sent copy: the exact bytes they received, in `file`.
    #[must_use]
    pub fn open_sent_copy(&self, ledger: &[u8], sent: &str) -> Reply {
        let prepared = fold(ledger).and_then(|s| Ok((s, id(sent)?)));
        let (state, sent) = match prepared {
            Ok(p) => p,
            Err(e) => return Reply::failure(&e),
        };
        let source = Recording::new(&self.cache);
        let result = commands::open_sent_copy(&state, &sent, &source).map_err(Failure::from);
        match Self::finish(result, &source) {
            Ok(bytes) => {
                let mut reply = Reply::ok(&serde_json::Value::Null);
                reply.file = Some(bytes);
                reply
            }
            Err(e) => Reply::failure(&e),
        }
    }

    /// "A file came back": which copy `file` is. `ledgers` is a JSON array of
    /// `{family, ledger}`, every family in storage, the ledger as text.
    #[must_use]
    pub fn recognise(&self, file: &[u8], ledgers: &str) -> Reply {
        let result = (|| -> Result<RecognitionDto> {
            let ledgers: Vec<LedgerDto> = json(ledgers)?;
            let mut states = Vec::with_capacity(ledgers.len());
            for l in &ledgers {
                let state = fold(l.ledger.as_bytes())?;
                if state.family_id != l.family {
                    return Err(Failure::BadInput(
                        "a ledger is not the family it was given as".to_owned(),
                    ));
                }
                states.push(state);
            }
            let refs: Vec<&FamilyState> = states.iter().collect();
            let prepared = returned::prepare(file)?;
            let found = match returned::by_ids(&prepared, &refs) {
                Some(found) => found,
                None => {
                    let mut all = Vec::new();
                    for (family, checkpoints) in returned::shortlist(&prepared, &refs) {
                        let Some(state) = refs.iter().find(|s| s.family_id == family) else {
                            continue;
                        };
                        let source = Recording::new(&self.cache);
                        match returned::score(&prepared, state, &checkpoints, &source) {
                            Ok(found) => all.extend(found),
                            Err(e) => {
                                let missed = source.missed();
                                return Err(if missed.is_empty() {
                                    Failure::Lineage(e)
                                } else {
                                    Failure::Missing(missed, Some(family))
                                });
                            }
                        }
                    }
                    returned::verdict(all)
                }
            };
            Ok(RecognitionDto::from(&returned::describe(&found, &refs)))
        })();
        result.map_or_else(|e| Reply::failure(&e), |v| Reply::ok(&v))
    }

    /// "Review the changes" on a recognised file: keeps it exactly as it came
    /// back. `args` is `{name, checkpoint, sent, matchedBy}`.
    pub fn record_returned(
        &mut self,
        ledger: &[u8],
        file: &[u8],
        args: &str,
        now_ms: f64,
    ) -> Reply {
        let prepared = fold(ledger).and_then(|s| Ok((s, json::<RecordArgs>(args)?)));
        let (state, args) = match prepared {
            Ok(p) => p,
            Err(e) => return Reply::failure(&e),
        };
        let name = args.name.trim().chars().take(200).collect::<String>();
        self.command(now_ms, |ctx, source| {
            returned::record(
                ctx,
                &state,
                file,
                &name,
                &args.checkpoint,
                args.sent.as_ref(),
                args.matched_by.into(),
                source,
            )
        })
    }

    /// The review of a returned file, judged against the open file of `node`.
    #[must_use]
    pub fn returned_review(&self, ledger: &[u8], node: &str, file: &[u8], returned: &str) -> Reply {
        let prepared = fold(ledger).and_then(|s| Ok((s, id(node)?, id(returned)?)));
        let (state, node, back) = match prepared {
            Ok(p) => p,
            Err(e) => return Reply::failure(&e),
        };
        self.view(|source| {
            returned::review(&state, &back, &node, file, source).map(|r| ReturnReviewDto::from(&r))
        })
    }

    /// "Apply" on a review: `cards` is a JSON array of `{card, decision}`.
    pub fn decide_returned(
        &mut self,
        ledger: &[u8],
        node: &str,
        file: &[u8],
        returned: &str,
        cards: &str,
        now_ms: f64,
    ) -> Reply {
        let prepared = fold(ledger).and_then(|s| {
            let cards: Vec<CardDecision> = json(cards)?;
            Ok((
                s,
                id(node)?,
                id(returned)?,
                cards
                    .into_iter()
                    .map(|c| (c.card, c.decision.into()))
                    .collect::<Vec<_>>(),
            ))
        });
        let (state, node, back, cards) = match prepared {
            Ok(p) => p,
            Err(e) => return Reply::failure(&e),
        };
        self.command(now_ms, |ctx, source| {
            returned::decide(ctx, &state, &back, &node, file, &cards, source)
        })
    }

    /// "Open what came back": the exact bytes, in `file`.
    #[must_use]
    pub fn open_returned(&self, ledger: &[u8], returned: &str) -> Reply {
        let prepared = fold(ledger).and_then(|s| Ok((s, id(returned)?)));
        let (state, back) = match prepared {
            Ok(p) => p,
            Err(e) => return Reply::failure(&e),
        };
        let source = Recording::new(&self.cache);
        let result = returned::open(&state, &back, &source).map_err(Failure::from);
        match Self::finish(result, &source) {
            Ok(bytes) => {
                let mut reply = Reply::ok(&serde_json::Value::Null);
                reply.file = Some(bytes);
                reply
            }
            Err(e) => Reply::failure(&e),
        }
    }

    /// Did a write land in Word as written?
    #[must_use]
    pub fn check_written(&self, expected: &[u8], actual: &[u8]) -> Reply {
        match lineage_core::check::written(expected, actual) {
            Ok(w) => Reply::ok(&WrittenDto::from(&w)),
            Err(e) => Reply::failure(&Failure::Lineage(e.into())),
        }
    }
}

#[cfg(test)]
mod tests;
