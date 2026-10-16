// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Reading a ledger that may be damaged, for screens that only show history
//! (decision 0015). Records are folded until the first line that can't be read
//! or breaks a rule, and the rest is reported, never guessed at.
//!
//! Commands keep using the strict [`crate::event::decode`] and
//! [`FamilyState::from_records`], so nothing is ever added to a damaged ledger.

use crate::event::{Record, decode};
use crate::ids::Timestamp;
use crate::state::FamilyState;

/// Where a ledger stops being readable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Damage {
    /// The first line that couldn't be used, counting from 1.
    pub line: usize,
    /// Lines from that one to the end, blank lines left out.
    pub lines_lost: usize,
    /// Structural, for logs; never document text.
    pub detail: String,
}

/// What could be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Readable {
    /// The family as far as it could be read; none when not even its first
    /// record could.
    pub state: Option<FamilyState>,
    /// How many records were folded.
    pub records: usize,
    pub damage: Option<Damage>,
}

impl Readable {
    /// "Lineage could read this history up to 14 Oct. 3 later entries are
    /// damaged, so nothing will be added to it."
    #[must_use]
    pub fn note(&self) -> Option<String> {
        let damage = self.damage.as_ref()?;
        let lost = match damage.lines_lost {
            1 => "1 later entry is".to_owned(),
            n => format!("{n} later entries are"),
        };
        Some(match &self.state {
            Some(state) => format!(
                "Lineage could read this history up to {}. {lost} damaged, so nothing will be added to it.",
                Timestamp::day_label(state.last_at)
            ),
            None => "Lineage can't read this history. Nothing will be added to it.".to_owned(),
        })
    }
}

/// Reads as much of a ledger as can be trusted.
#[must_use]
pub fn read_lenient(bytes: &[u8]) -> Readable {
    let lines: Vec<(usize, &[u8])> = bytes
        .split(|b| *b == b'\n')
        .enumerate()
        .map(|(i, line)| (i + 1, line.strip_suffix(b"\r").unwrap_or(line)))
        .filter(|(_, line)| !line.iter().all(u8::is_ascii_whitespace))
        .collect();
    let mut state: Option<FamilyState> = None;
    let mut records = 0;
    for (index, (number, line)) in lines.iter().enumerate() {
        let folded = one(line).and_then(|record| match state.as_mut() {
            None => FamilyState::from_records(std::slice::from_ref(&record))
                .map(|s| state = Some(s))
                .map_err(|e| e.to_string()),
            Some(s) => s.apply(&record).map_err(|e| e.to_string()),
        });
        if let Err(detail) = folded {
            return Readable {
                state,
                records,
                damage: Some(Damage {
                    line: *number,
                    lines_lost: lines.len() - index,
                    detail,
                }),
            };
        }
        records += 1;
    }
    let damage = state.is_none().then(|| Damage {
        line: 1,
        lines_lost: 0,
        detail: "the ledger is empty".to_owned(),
    });
    Readable {
        state,
        records,
        damage,
    }
}

fn one(line: &[u8]) -> Result<Record, String> {
    let mut decoded = decode(line).map_err(|e| e.to_string())?;
    match (decoded.pop(), decoded.is_empty()) {
        (Some(record), true) => Ok(record),
        _ => Err("not one record".to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Event, RECORD_VERSION, Role, encode};
    use crate::ids::Id;

    fn id(n: u8) -> Id {
        Id::parse(&format!("{n:032x}")).expect("id")
    }

    fn ledger() -> Vec<Record> {
        let at = Timestamp(1_792_056_600_000);
        let events = [
            Event::FamilyStarted {
                family_id: id(1),
                name: "Resume".into(),
                format: "docx".into(),
            },
            Event::NodeAdded {
                node_id: id(2),
                role: Role::Master,
                display_name: "Resume.docx".into(),
                storage_ref: "Resume.docx".into(),
                made_from_checkpoint_id: None,
                follows_master: false,
            },
            Event::NodeArchived { node_id: id(2) },
            Event::NodeRelinked {
                node_id: id(2),
                storage_ref: "Moved/Resume.docx".into(),
            },
        ];
        events
            .into_iter()
            .enumerate()
            .map(|(i, event)| Record {
                v: RECORD_VERSION,
                seq: i as u64 + 1,
                at: Timestamp(at.0 + i as i64 * 86_400_000),
                event,
            })
            .collect()
    }

    #[test]
    fn a_whole_ledger_reads_with_no_damage() {
        let bytes = encode(&ledger()).expect("encodes");
        let read = read_lenient(&bytes);
        assert_eq!(read.records, 4);
        assert_eq!(read.damage, None);
        assert_eq!(read.note(), None);
        let state = read.state.expect("state");
        assert_eq!(state.nodes[0].storage_ref, "Moved/Resume.docx");
    }

    #[test]
    fn a_cut_off_last_line_keeps_everything_before_it() {
        let mut bytes = encode(&ledger()).expect("encodes");
        bytes.truncate(bytes.len() - 20);
        let read = read_lenient(&bytes);
        assert_eq!(read.records, 3);
        let damage = read.damage.clone().expect("damage");
        assert_eq!((damage.line, damage.lines_lost), (4, 1));
        assert_eq!(
            read.note().expect("note"),
            "Lineage could read this history up to 17 Oct. 1 later entry is damaged, so nothing will be added to it."
        );
    }

    #[test]
    fn a_gap_or_an_unknown_record_stops_the_reading_there() {
        let mut records = ledger();
        records.remove(2);
        let read = read_lenient(&encode(&records).expect("encodes"));
        assert_eq!(read.records, 2);
        assert_eq!(read.damage.expect("damage").lines_lost, 1);

        let mut text = String::from_utf8(encode(&ledger()).expect("encodes")).expect("utf-8");
        text = text.replacen("NodeArchived", "NodeVanished", 1);
        text.push_str("\r\n\n");
        let read = read_lenient(text.as_bytes());
        assert_eq!(read.records, 2);
        let damage = read.damage.expect("damage");
        assert_eq!((damage.line, damage.lines_lost), (3, 2));
    }

    #[test]
    fn nothing_readable_has_no_state() {
        for bytes in [&b""[..], b"\n\n", b"not json\n", &[0xff, 0xfe, b'\n']] {
            let read = read_lenient(bytes);
            assert!(read.state.is_none());
            assert!(read.damage.is_some());
            assert_eq!(
                read.note().expect("note"),
                "Lineage can't read this history. Nothing will be added to it."
            );
        }
    }
}
