// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The shared-strings table, streamed: a workbook with many distinct texts
//! can have a large one.

use quick_xml::events::Event;

use super::model::SharedString;
use super::stream::Pull;
use crate::error::{Error, Result};
use crate::hash::Fnv;

/// Most entries the table may have.
pub const MAX_STRINGS: usize = 10_000_000;

/// Reads `xl/sharedStrings.xml`.
///
/// # Errors
/// [`Error::BadXml`] when it isn't a shared-strings part.
pub(super) fn read(part: &str, text: &str) -> Result<Vec<SharedString>> {
    let mut pull = Pull::new(part, text);
    let mut out = Vec::new();
    let mut depth = 0usize;
    loop {
        let ev = pull.next()?;
        match ev.event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev.event, Event::Empty(_));
                let local = e.local_name().as_ref().to_owned();
                if depth == 0 {
                    if ev.strict {
                        return Err(Error::Unsupported("Strict Open XML".to_owned()));
                    }
                    if !(ev.main && local == "sst") {
                        return Err(pull.bad("not a shared-strings table"));
                    }
                    if empty {
                        break;
                    }
                    depth = 1;
                    continue;
                }
                if ev.main && local == "si" {
                    if out.len() == MAX_STRINGS {
                        return Err(Error::TooLarge("too many shared strings".to_owned()));
                    }
                    out.push(if empty {
                        SharedString {
                            text: String::new(),
                            rich: 0,
                        }
                    } else {
                        read_si(&mut pull)?
                    });
                } else if !empty {
                    pull.hash_rest(&mut Fnv::new())?;
                }
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    break;
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(out)
}

/// One `<si>`: plain `<t>`, or runs (`<r>`) with their formatting.
fn read_si(pull: &mut Pull<'_>) -> Result<SharedString> {
    let mut text = String::new();
    let mut rich = Fnv::new();
    let mut runs = 0usize;
    loop {
        let ev = pull.next()?;
        match ev.event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev.event, Event::Empty(_));
                let name = e.local_name();
                let local: &str = name.as_ref();
                match (ev.main, local, empty) {
                    (true, "t", false) => {
                        let s = pull.text_until_end(ev.end)?;
                        text.push_str(s.get(pull.text));
                    }
                    (true, "r", false) => {
                        runs += 1;
                        let run = read_run(pull, &mut rich)?;
                        rich.str("|").u64(run.len() as u64);
                        text.push_str(&run);
                    }
                    (_, _, false) => pull.hash_rest(&mut Fnv::new())?,
                    _ => {}
                }
            }
            Event::End(_) => {
                return Ok(SharedString {
                    text,
                    rich: if runs == 0 { 0 } else { rich.finish() },
                });
            }
            Event::Eof => return Err(pull.bad("ends inside a string")),
            _ => {}
        }
    }
}

/// A run's text, hashing its properties into `rich`.
fn read_run(pull: &mut Pull<'_>, rich: &mut Fnv) -> Result<String> {
    let mut text = String::new();
    loop {
        let ev = pull.next()?;
        match ev.event {
            Event::Start(ref e) | Event::Empty(ref e) => {
                let empty = matches!(ev.event, Event::Empty(_));
                let name = e.local_name();
                let local: &str = name.as_ref();
                match (ev.main, local, empty) {
                    (true, "t", false) => {
                        let s = pull.text_until_end(ev.end)?;
                        text.push_str(s.get(pull.text));
                    }
                    (true, "rPr", false) => {
                        pull.hash_start(rich, e);
                        pull.hash_rest(rich)?;
                    }
                    (_, _, false) => pull.hash_rest(&mut Fnv::new())?,
                    _ => {}
                }
            }
            Event::End(_) => return Ok(text),
            Event::Eof => return Err(pull.bad("ends inside a run")),
            _ => {}
        }
    }
}
