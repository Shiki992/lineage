// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! VBA projects (`vbaProject.bin`), read as text (Phase 5B, decision 0028).
//!
//! Lineage never runs, compiles or evaluates macros. It reads the stored
//! source of each module, the project's references and who signed it, and
//! compares them as code. It doesn't write VBA: a project changes only in
//! Excel, by the person.
//!
//! The compound file, the compression and the dir stream are all untrusted
//! input with limits ([`cfb`], [`compress`], [`dir`]). Nothing here depends
//! on the host application, so `.docm` and `.pptm` can use it later.

pub mod cfb;
pub mod compress;
pub mod diff;
pub mod dir;
pub mod project;
pub mod signature;

#[cfg(test)]
mod tests;

use crate::error::{Error, Result};
use crate::hash::Fnv;

pub use diff::{CodeLine, LineKind, ModuleChange, ModuleChangeKind, VbaDiff};
pub use dir::ReferenceKind;
pub use signature::Signer;

/// Largest decompressed module source.
pub const MAX_MODULE_BYTES: usize = 16 * 1024 * 1024;
/// Largest decompressed source for the whole project.
pub const MAX_PROJECT_BYTES: usize = 64 * 1024 * 1024;

/// What a module is, as the VBA editor groups them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ModuleKind {
    /// A standard module.
    Standard,
    /// A class module.
    Class,
    /// A user form: code and a designer (its layout and controls).
    Form,
    /// The workbook's own code (ThisWorkbook).
    Workbook,
    /// A sheet's code.
    Sheet,
    /// Another document's code (a chart, or a host Lineage doesn't know).
    Document,
}

impl ModuleKind {
    /// How the panel names one: "Module Forecast", "Class Scenario".
    #[must_use]
    pub fn noun(self) -> &'static str {
        match self {
            Self::Standard => "Module",
            Self::Class => "Class",
            Self::Form => "Form",
            Self::Workbook => "Workbook code",
            Self::Sheet => "Sheet code",
            Self::Document => "Document code",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub kind: ModuleKind,
    /// The `Attribute VB_…` lines at the top, which the VBA editor hides.
    pub attributes: Vec<String>,
    /// The code the editor shows, one entry per line.
    pub lines: Vec<String>,
    /// For a form: a key of its designer storage (layout and controls),
    /// which Lineage reports only as changed or not changed.
    pub designer: Option<u64>,
    pub private: bool,
    pub read_only: bool,
    /// The source's bytes as stored, so a change the code page hides
    /// still shows.
    pub key: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub name: String,
    pub kind: ReferenceKind,
    /// "*\G{…}#2.0#0#C:\…\scrrun.dll#Microsoft Scripting Runtime"
    pub libid: String,
}

impl Reference {
    /// What the References dialog shows: the libid's description, or the
    /// name.
    #[must_use]
    pub fn label(&self) -> String {
        match self.kind {
            ReferenceKind::Registered | ReferenceKind::Control => self
                .libid
                .rsplit('#')
                .next()
                .filter(|d| !d.is_empty() && !d.contains('\\'))
                .map_or_else(|| self.name.clone(), str::to_owned),
            ReferenceKind::Project => self.name.clone(),
        }
    }

    /// The library's identity without its version or path: the GUID part.
    #[must_use]
    pub fn identity(&self) -> String {
        let id = self.libid.split('#').next().unwrap_or(&self.libid);
        id.to_ascii_uppercase()
    }
}

/// A digital signature on the project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Signature {
    pub signer: Signer,
    /// A key of the signature bytes, so a re-signed project shows.
    pub key: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VbaProject {
    pub name: String,
    pub codepage: u16,
    pub modules: Vec<Module>,
    pub references: Vec<Reference>,
    /// Locked for viewing in the VBA editor. The source is still stored,
    /// and Lineage still compares it.
    pub locked: bool,
    pub signature: Option<Signature>,
    /// Some characters are in a code page Lineage doesn't show exactly;
    /// they appear as "?". Changes are still found by the bytes.
    pub lossy: bool,
}

impl VbaProject {
    #[must_use]
    pub fn module(&self, name: &str) -> Option<&Module> {
        self.modules
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(name))
    }
}

/// Windows-1252's characters at 0x80 to 0x9F (0 where it has none).
const CP1252: [u16; 32] = [
    0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0, 0x017D, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC,
    0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
];

/// Text from the project's code page, and whether anything was shown as
/// "?". 1252 and UTF-8 are exact; other code pages are exact for ASCII.
#[must_use]
pub fn decode(bytes: &[u8], codepage: u16) -> (String, bool) {
    match codepage {
        65001 => {
            let s = String::from_utf8_lossy(bytes);
            let lossy = matches!(s, std::borrow::Cow::Owned(_));
            (s.into_owned(), lossy)
        }
        1252 | 28591 => {
            let mut lossy = false;
            let s = bytes
                .iter()
                .map(|&b| match b {
                    0x80..=0x9F if codepage == 1252 => {
                        let u = CP1252[usize::from(b - 0x80)];
                        if u == 0 {
                            lossy = true;
                            '?'
                        } else {
                            char::from_u32(u32::from(u)).unwrap_or('?')
                        }
                    }
                    _ => char::from(b),
                })
                .collect();
            (s, lossy)
        }
        _ => {
            let mut lossy = false;
            let s = bytes
                .iter()
                .map(|&b| {
                    if b.is_ascii() {
                        char::from(b)
                    } else {
                        lossy = true;
                        '?'
                    }
                })
                .collect();
            (s, lossy)
        }
    }
}

/// Splits source into lines (CRLF, CR or LF).
fn lines_of(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                out.push(std::mem::take(&mut current));
            }
            '\n' => out.push(std::mem::take(&mut current)),
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// The `Attribute` lines at the top, and the code after them.
fn split_attributes(lines: Vec<String>) -> (Vec<String>, Vec<String>) {
    let header = lines
        .iter()
        .take_while(|l| l.trim_start().starts_with("Attribute VB_"))
        .count();
    let mut lines = lines;
    let code = lines.split_off(header);
    (lines, code)
}

/// The module's `VB_Base` GUID, which says whose document code it is.
fn kind_of_document(attributes: &[String]) -> ModuleKind {
    let base = attributes
        .iter()
        .find(|a| a.trim_start().starts_with("Attribute VB_Base"))
        .map(|a| a.to_ascii_uppercase());
    match base {
        Some(b) if b.contains("00020819-0000-0000-C000-000000000046") => ModuleKind::Workbook,
        Some(b) if b.contains("00020820-0000-0000-C000-000000000046") => ModuleKind::Sheet,
        _ => ModuleKind::Document,
    }
}

/// Reads a `vbaProject.bin`. `signatures` are the bytes of any signature
/// parts the package relates to it.
///
/// # Errors
/// [`Error::Damaged`] when the compound file, the dir stream or a module
/// is malformed, [`Error::TooLarge`] past the limits.
pub fn read(bin: &[u8], signatures: &[&[u8]]) -> Result<VbaProject> {
    let cfb = cfb::read(bin)?;
    let dir_bytes = cfb
        .stream("VBA/dir")
        .ok_or_else(|| Error::Damaged("VBA project: no dir stream".to_owned()))?;
    let dir = dir::parse(&compress::decompress(dir_bytes, MAX_MODULE_BYTES)?)?;
    let info = cfb
        .stream("PROJECT")
        .map(project::parse)
        .unwrap_or_default();
    let cp = dir.codepage;
    let mut lossy = false;
    let mut text = |bytes: &[u8]| {
        let (s, l) = decode(bytes, cp);
        lossy |= l;
        s
    };
    let mut total = 0usize;
    let mut modules = Vec::new();
    for m in &dir.modules {
        let name = m.name_unicode.clone().unwrap_or_else(|| text(&m.name));
        let stream_name = m.stream_unicode.clone().unwrap_or_else(|| text(&m.stream));
        let stream = cfb
            .stream(&format!("VBA/{stream_name}"))
            .ok_or_else(|| Error::Damaged("VBA project: a module stream is missing".to_owned()))?;
        let offset = usize::try_from(m.offset).unwrap_or(usize::MAX);
        if offset > stream.len() {
            return Err(Error::Damaged(
                "VBA project: a module's source starts past its stream".to_owned(),
            ));
        }
        let source = compress::decompress(&stream[offset..], MAX_MODULE_BYTES)?;
        total += source.len();
        if total > MAX_PROJECT_BYTES {
            return Err(Error::TooLarge("VBA project too large".to_owned()));
        }
        let key = Fnv::new().bytes(&source).finish();
        let (attributes, lines) = split_attributes(lines_of(&text(&source)));
        let declared = info
            .modules
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(&m.name))
            .map(|(_, d)| *d);
        let kind = match declared {
            Some(project::Declared::Standard) => ModuleKind::Standard,
            Some(project::Declared::Class) => ModuleKind::Class,
            Some(project::Declared::Designer) => ModuleKind::Form,
            Some(project::Declared::Document) => kind_of_document(&attributes),
            None if m.procedural => ModuleKind::Standard,
            None => ModuleKind::Class,
        };
        let designer = (kind == ModuleKind::Form).then(|| {
            let mut h = Fnv::new();
            for e in cfb.under(&stream_name) {
                h.str(&e.path.to_ascii_lowercase()).bytes(&e.data);
            }
            h.finish()
        });
        modules.push(Module {
            name,
            kind,
            attributes,
            lines,
            designer,
            private: m.private,
            read_only: m.read_only,
            key,
        });
    }
    let references = dir
        .references
        .iter()
        .map(|r| Reference {
            name: r.name_unicode.clone().unwrap_or_else(|| text(&r.name)),
            kind: r.kind,
            libid: text(&r.libid),
        })
        .collect();
    // Signatures: the package's parts first, then the legacy streams.
    let mut sig_bytes: Vec<&[u8]> = signatures.to_vec();
    for legacy in [
        "\u{5}DigitalSignature",
        "\u{5}DigitalSignatureEx",
        "\u{5}DigitalSignatureAgile",
        "\u{5}DigitalSignatureV3",
    ] {
        if let Some(s) = cfb.stream(legacy) {
            sig_bytes.push(s);
        }
    }
    let signature = sig_bytes.first().map(|bytes| Signature {
        signer: signature::signer(bytes).unwrap_or_default(),
        key: sig_bytes
            .iter()
            .fold(Fnv::new(), |mut h, b| {
                h.bytes(b);
                h
            })
            .finish(),
    });
    let name = text(&dir.name);
    Ok(VbaProject {
        name,
        codepage: cp,
        modules,
        references,
        locked: info.locked,
        signature,
        lossy,
    })
}
