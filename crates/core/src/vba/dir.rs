// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The `dir` stream ([MS-OVBA] 2.3.4.2): the project's code page and name,
//! its references, and one MODULE record per module.
//!
//! Every record is an id (2 bytes), a size (4 bytes) and that many bytes,
//! except PROJECTVERSION, whose "size" is a reserved 4 followed by 6 bytes.
//! Unknown records are skipped by their size, so a later Office's records
//! don't stop the read.
//!
//! [MS-OVBA]: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-ovba/672d276c-d83c-4452-993b-ca9eca3d8917

use crate::error::{Error, Result};

/// Most modules or references a project may have.
pub const MAX_ITEMS: usize = 10_000;

fn damaged(detail: &str) -> Error {
    Error::Damaged(format!("VBA dir stream: {detail}"))
}

/// What a reference points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceKind {
    /// A registered type library (REFERENCEREGISTERED).
    Registered,
    /// Another VBA project (REFERENCEPROJECT).
    Project,
    /// A control library (REFERENCECONTROL / REFERENCEORIGINAL).
    Control,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DirReference {
    /// The name as bytes in the project's code page.
    pub name: Vec<u8>,
    pub name_unicode: Option<String>,
    pub kind: ReferenceKind,
    /// The library id, in the code page ("*\G{…}#2.0#0#path#Description").
    pub libid: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirModule {
    pub name: Vec<u8>,
    pub name_unicode: Option<String>,
    pub stream: Vec<u8>,
    pub stream_unicode: Option<String>,
    /// Where the compressed source starts in the module stream.
    pub offset: u32,
    /// MODULETYPE 0x0021: a standard (procedural) module.
    pub procedural: bool,
    pub read_only: bool,
    pub private: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Dir {
    pub codepage: u16,
    pub name: Vec<u8>,
    pub references: Vec<DirReference>,
    pub modules: Vec<DirModule>,
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .collect();
    String::from_utf16_lossy(&units)
}

fn u32_of(data: &[u8]) -> u32 {
    if data.len() >= 4 {
        u32::from_le_bytes([data[0], data[1], data[2], data[3]])
    } else {
        0
    }
}

/// The first `SizeOf…`-prefixed string in `data`, and the rest.
fn sized(data: &[u8]) -> Result<(&[u8], &[u8])> {
    if data.len() < 4 {
        return Err(damaged("a string cut short"));
    }
    let n = usize::try_from(u32_of(data)).map_err(|_| damaged("string size"))?;
    let rest = &data[4..];
    if n > rest.len() {
        return Err(damaged("a string longer than its record"));
    }
    Ok((&rest[..n], &rest[n..]))
}

/// Parses a decompressed `dir` stream.
///
/// # Errors
/// [`Error::Damaged`] when a record runs past the stream or a required
/// record is missing.
pub fn parse(dir: &[u8]) -> Result<Dir> {
    let mut out = Dir::default();
    let mut at = 0;
    let mut pending_name: Option<(Vec<u8>, Option<String>)> = None;
    let mut module: Option<DirModule> = None;
    let mut saw_codepage = false;
    while at + 6 <= dir.len() {
        let id = u16::from_le_bytes([dir[at], dir[at + 1]]);
        let size = usize::try_from(u32_of(&dir[at + 2..])).map_err(|_| damaged("size"))?;
        at += 6;
        // PROJECTVERSION: "Reserved" is 4, then 4 + 2 bytes.
        let len = if id == 0x0009 { 6 } else { size };
        if at + len > dir.len() {
            return Err(damaged("a record runs past the end"));
        }
        let data = &dir[at..at + len];
        at += len;
        match id {
            0x0003 => {
                if data.len() < 2 {
                    return Err(damaged("code page"));
                }
                out.codepage = u16::from_le_bytes([data[0], data[1]]);
                saw_codepage = true;
            }
            0x0004 => out.name = data.to_vec(),
            // REFERENCENAME and its Unicode form.
            0x0016 => pending_name = Some((data.to_vec(), None)),
            0x003E => {
                if let Some((_, u)) = pending_name.as_mut() {
                    *u = Some(utf16(data));
                }
            }
            0x000D | 0x000E | 0x002F | 0x0033 => {
                let kind = match id {
                    0x000D => ReferenceKind::Registered,
                    0x000E => ReferenceKind::Project,
                    _ => ReferenceKind::Control,
                };
                let libid = match id {
                    // REFERENCEORIGINAL is the libid itself; the others
                    // start with SizeOfLibid…, after their own size.
                    0x0033 => data.to_vec(),
                    _ => sized(data)?.0.to_vec(),
                };
                // A REFERENCECONTROL right after its REFERENCEORIGINAL is the
                // same reference.
                let same = id == 0x002F
                    && out.references.last().is_some_and(|r| {
                        r.kind == ReferenceKind::Control && pending_name.is_none()
                    });
                if !same {
                    if out.references.len() == MAX_ITEMS {
                        return Err(Error::TooLarge("too many VBA references".to_owned()));
                    }
                    let (name, name_unicode) = pending_name.take().unwrap_or_default();
                    out.references.push(DirReference {
                        name,
                        name_unicode,
                        kind,
                        libid,
                    });
                } else {
                    pending_name = None;
                }
            }
            // The extended part of a REFERENCECONTROL: its own name record
            // came just before, and belongs to it.
            0x0030 => pending_name = None,
            // MODULENAME starts a module.
            0x0019 => {
                if let Some(m) = module.take() {
                    out.modules.push(m);
                }
                if out.modules.len() == MAX_ITEMS {
                    return Err(Error::TooLarge("too many VBA modules".to_owned()));
                }
                module = Some(DirModule {
                    name: data.to_vec(),
                    ..DirModule::default()
                });
            }
            0x0047 => {
                if let Some(m) = module.as_mut() {
                    m.name_unicode = Some(utf16(data));
                }
            }
            0x001A => {
                if let Some(m) = module.as_mut() {
                    m.stream = data.to_vec();
                }
            }
            0x0032 => {
                if let Some(m) = module.as_mut() {
                    m.stream_unicode = Some(utf16(data));
                }
            }
            0x0031 => {
                if let Some(m) = module.as_mut() {
                    m.offset = u32_of(data);
                }
            }
            0x0021 => {
                if let Some(m) = module.as_mut() {
                    m.procedural = true;
                }
            }
            0x0025 => {
                if let Some(m) = module.as_mut() {
                    m.read_only = true;
                }
            }
            0x0028 => {
                if let Some(m) = module.as_mut() {
                    m.private = true;
                }
            }
            0x002B => {
                if let Some(m) = module.take() {
                    out.modules.push(m);
                }
            }
            // The dir stream's terminator.
            0x0010 => break,
            _ => {}
        }
    }
    if let Some(m) = module.take() {
        out.modules.push(m);
    }
    if !saw_codepage {
        return Err(damaged("no code page"));
    }
    Ok(out)
}
