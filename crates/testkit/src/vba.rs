// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Builds VBA projects (`vbaProject.bin`) for synthetic macro workbooks:
//! a compound file writer ([MS-CFB]), the VBA compression ([MS-OVBA] 2.4.1),
//! and the project's streams in the form the spec asks of a writer
//! ([MS-OVBA] 2.3.4.1: `_VBA_PROJECT` with version 0xFFFF and no
//! performance cache, module streams with no cache, no SRP streams). That is
//! the form that makes Excel compile the code again when it opens the file,
//! and the form the Phase 5B spike opens in real Excel (decision 0028).
//!
//! Every module, name and line of code here is invented. Nothing is run.
//!
//! [MS-CFB]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-cfb/53989ce4-7b05-4f8d-829b-d08d6148375b
//! [MS-OVBA]: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-ovba/575462ba-bf67-4190-9fac-c275523c75fc

use std::collections::BTreeSet;

// ---------------------------------------------------------------------------
// Compression (MS-OVBA 2.4.1)
// ---------------------------------------------------------------------------

const CHUNK: usize = 4096;

/// The copy-token split for a position `difference` bytes into its chunk
/// (2.4.1.3.19.1): (length mask, offset mask, bit count, maximum length).
fn copy_token_help(difference: usize) -> (u16, u16, u32, usize) {
    let mut bit_count = 4u32;
    while (1usize << bit_count) < difference {
        bit_count += 1;
    }
    let length_mask = 0xFFFFu16 >> bit_count;
    let offset_mask = !length_mask;
    let maximum = usize::from(length_mask) + 3;
    (length_mask, offset_mask, bit_count, maximum)
}

/// Compresses `data` as a CompressedContainer (2.4.1.3.6).
///
/// The longest match is searched naively: fixtures are small, and the result
/// only has to be valid, not the smallest.
#[must_use]
pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut out = vec![0x01];
    let mut start = 0;
    loop {
        let end = (start + CHUNK).min(data.len());
        out.extend(compress_chunk(&data[start..end]));
        start = end;
        if start >= data.len() {
            break;
        }
    }
    out
}

/// Most earlier positions tried for a match. The spec examples are found
/// exactly; longer inputs may get a shorter match, which is still valid.
const CHAIN: usize = 64;

fn compress_chunk(chunk: &[u8]) -> Vec<u8> {
    let mut body = Vec::new();
    let mut current = 0;
    // Earlier positions by their first three bytes, nearest last.
    let mut seen: std::collections::HashMap<[u8; 3], Vec<usize>> = std::collections::HashMap::new();
    let mut indexed = 0;
    while current < chunk.len() {
        let flag_at = body.len();
        body.push(0u8);
        let mut flags = 0u8;
        for bit in 0..8 {
            if current >= chunk.len() {
                break;
            }
            let (length_mask, _, bit_count, maximum) = copy_token_help(current);
            // The longest earlier match of at least 3 bytes, the nearest on a
            // tie (2.4.1.3.19.4).
            while indexed < current {
                if indexed + 3 <= chunk.len() {
                    let key = [chunk[indexed], chunk[indexed + 1], chunk[indexed + 2]];
                    seen.entry(key).or_default().push(indexed);
                }
                indexed += 1;
            }
            let mut best = (0usize, 0usize);
            let candidates: Vec<usize> = if current + 3 <= chunk.len() {
                let key = [chunk[current], chunk[current + 1], chunk[current + 2]];
                seen.get(&key)
                    .map(|v| v.iter().rev().take(CHAIN).copied().collect())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            for candidate in candidates {
                let mut len = 0;
                while current + len < chunk.len()
                    && len < maximum
                    && chunk[candidate + len] == chunk[current + len]
                {
                    len += 1;
                }
                if len > best.1 {
                    best = (candidate, len);
                }
            }
            if best.1 >= 3 {
                let offset = current - best.0;
                let token = (u16::try_from(offset - 1).unwrap_or(0) << (16 - bit_count))
                    | (u16::try_from(best.1 - 3).unwrap_or(0) & length_mask);
                body.extend_from_slice(&token.to_le_bytes());
                flags |= 1 << bit;
                current += best.1;
            } else {
                body.push(chunk[current]);
                current += 1;
            }
        }
        body[flag_at] = flags;
    }
    if body.len() > CHUNK && chunk.len() == CHUNK {
        // A raw chunk: flag bit 0, 4096 bytes as they are.
        let header: u16 = 0x3000 | 0x0FFF;
        let mut out = header.to_le_bytes().to_vec();
        out.extend_from_slice(chunk);
        return out;
    }
    assert!(
        body.len() <= CHUNK,
        "a short chunk that doesn't compress can't be written"
    );
    let size = u16::try_from(body.len() + 2 - 3).unwrap_or(0);
    let header: u16 = 0x8000 | 0x3000 | size;
    let mut out = header.to_le_bytes().to_vec();
    out.extend(body);
    out
}

// ---------------------------------------------------------------------------
// Compound file writer (MS-CFB), version 3, 512-byte sectors
// ---------------------------------------------------------------------------

const SECTOR: usize = 512;
const MINI: usize = 64;
const MINI_CUTOFF: usize = 4096;
const FREESECT: u32 = 0xFFFF_FFFF;
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FATSECT: u32 = 0xFFFF_FFFD;
const NOSTREAM: u32 = 0xFFFF_FFFF;

/// One stream of a compound file, by its path ("VBA/dir"). Storages are
/// implied by the paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CfbStream {
    pub path: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug)]
struct DirEntry {
    name: String,
    storage: bool,
    data: Vec<u8>,
    children: Vec<usize>,
    left: u32,
    right: u32,
    child: u32,
    black: bool,
    start: u32,
}

/// The order of siblings in a compound file's red-black tree: shorter names
/// first, then by upper-case UTF-16 code units (MS-CFB 2.6.4).
fn cfb_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let ua: Vec<u16> = a.to_uppercase().encode_utf16().collect();
    let ub: Vec<u16> = b.to_uppercase().encode_utf16().collect();
    ua.len().cmp(&ub.len()).then(ua.cmp(&ub))
}

/// Writes a compound file holding `streams`.
///
/// # Panics
/// When a name is longer than 31 UTF-16 units, or the file would need more
/// FAT sectors than the header holds (about 7 MB), which no fixture does.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn write_cfb(streams: &[CfbStream]) -> Vec<u8> {
    let mut entries = vec![DirEntry {
        name: "Root Entry".to_owned(),
        storage: true,
        data: Vec::new(),
        children: Vec::new(),
        left: NOSTREAM,
        right: NOSTREAM,
        child: NOSTREAM,
        black: true,
        start: ENDOFCHAIN,
    }];
    for stream in streams {
        let mut parent = 0;
        let parts: Vec<&str> = stream.path.split('/').collect();
        for (n, part) in parts.iter().enumerate() {
            assert!(part.encode_utf16().count() <= 31, "name too long: {part}");
            let last = n + 1 == parts.len();
            let found = entries[parent]
                .children
                .iter()
                .copied()
                .find(|&c| entries[c].name.eq_ignore_ascii_case(part));
            let index = if let Some(c) = found {
                c
            } else {
                entries.push(DirEntry {
                    name: (*part).to_owned(),
                    storage: !last,
                    data: Vec::new(),
                    children: Vec::new(),
                    left: NOSTREAM,
                    right: NOSTREAM,
                    child: NOSTREAM,
                    black: true,
                    start: ENDOFCHAIN,
                });
                let c = entries.len() - 1;
                entries[parent].children.push(c);
                c
            };
            if last {
                entries[index].data.clone_from(&stream.data);
            }
            parent = index;
        }
    }
    // Each storage's children as a balanced tree, its deepest partial level
    // red: a valid red-black tree.
    for parent in 0..entries.len() {
        let mut kids = entries[parent].children.clone();
        if kids.is_empty() {
            continue;
        }
        kids.sort_by(|&a, &b| cfb_cmp(&entries[a].name, &entries[b].name));
        let full_depth = usize::BITS - 1 - (kids.len() + 1).leading_zeros();
        let root = balance(&mut entries, &kids, 1, full_depth);
        entries[parent].child = root;
    }

    // Small streams in the mini stream, the rest in sectors.
    let mut mini_stream: Vec<u8> = Vec::new();
    let mut mini_fat: Vec<u32> = Vec::new();
    let mut big: Vec<usize> = Vec::new();
    for (i, e) in entries.iter_mut().enumerate() {
        if e.storage || e.data.is_empty() {
            continue;
        }
        if e.data.len() < MINI_CUTOFF {
            let first = u32::try_from(mini_fat.len()).unwrap_or(0);
            let count = e.data.len().div_ceil(MINI);
            for k in 0..count {
                let next = if k + 1 == count {
                    ENDOFCHAIN
                } else {
                    first + u32::try_from(k + 1).unwrap_or(0)
                };
                mini_fat.push(next);
            }
            e.start = first;
            mini_stream.extend_from_slice(&e.data);
            mini_stream.resize(mini_fat.len() * MINI, 0);
        } else {
            big.push(i);
        }
    }

    let sectors = |bytes: usize| bytes.div_ceil(SECTOR);
    let dir_sectors = sectors(entries.len() * 128);
    let mini_fat_sectors = sectors(mini_fat.len() * 4);
    let mini_stream_sectors = sectors(mini_stream.len());
    let big_sectors: usize = big.iter().map(|&i| sectors(entries[i].data.len())).sum();
    let content = dir_sectors + mini_fat_sectors + mini_stream_sectors + big_sectors;
    let mut fat_sectors = 1;
    while fat_sectors * (SECTOR / 4) < content + fat_sectors {
        fat_sectors += 1;
    }
    assert!(fat_sectors <= 109, "too large for a fixture compound file");

    let total = fat_sectors + content;
    let mut fat = vec![FREESECT; fat_sectors * (SECTOR / 4)];
    for f in fat.iter_mut().take(fat_sectors) {
        *f = FATSECT;
    }
    let mut next_sector = fat_sectors;
    let mut chain = |fat: &mut Vec<u32>, count: usize| -> u32 {
        if count == 0 {
            return ENDOFCHAIN;
        }
        let first = next_sector;
        for k in 0..count {
            fat[first + k] = if k + 1 == count {
                ENDOFCHAIN
            } else {
                u32::try_from(first + k + 1).unwrap_or(0)
            };
        }
        next_sector += count;
        u32::try_from(first).unwrap_or(0)
    };
    let dir_start = chain(&mut fat, dir_sectors);
    let mini_fat_start = chain(&mut fat, mini_fat_sectors);
    let mini_stream_start = chain(&mut fat, mini_stream_sectors);
    for &i in &big {
        let count = sectors(entries[i].data.len());
        entries[i].start = chain(&mut fat, count);
    }
    entries[0].start = mini_stream_start;
    debug_assert_eq!(next_sector, total);

    let mut out = Vec::with_capacity((total + 1) * SECTOR);
    // Header.
    out.extend_from_slice(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    out.extend_from_slice(&[0; 16]);
    out.extend_from_slice(&0x003Eu16.to_le_bytes());
    out.extend_from_slice(&0x0003u16.to_le_bytes());
    out.extend_from_slice(&0xFFFEu16.to_le_bytes());
    out.extend_from_slice(&9u16.to_le_bytes());
    out.extend_from_slice(&6u16.to_le_bytes());
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&0u32.to_le_bytes()); // directory sectors (v3: 0)
    out.extend_from_slice(&u32::try_from(fat_sectors).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(&dir_start.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&u32::try_from(MINI_CUTOFF).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(&mini_fat_start.to_le_bytes());
    out.extend_from_slice(&u32::try_from(mini_fat_sectors).unwrap_or(0).to_le_bytes());
    out.extend_from_slice(&ENDOFCHAIN.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    for k in 0..109 {
        let v = if k < fat_sectors {
            u32::try_from(k).unwrap_or(0)
        } else {
            FREESECT
        };
        out.extend_from_slice(&v.to_le_bytes());
    }
    debug_assert_eq!(out.len(), SECTOR);
    // FAT.
    for v in &fat {
        out.extend_from_slice(&v.to_le_bytes());
    }
    // Directory.
    let mut dir = Vec::new();
    for e in &entries {
        let name: Vec<u16> = e.name.encode_utf16().collect();
        let mut raw = [0u8; 128];
        for (k, unit) in name.iter().enumerate() {
            raw[k * 2..k * 2 + 2].copy_from_slice(&unit.to_le_bytes());
        }
        let name_len = u16::try_from((name.len() + 1) * 2).unwrap_or(0);
        raw[64..66].copy_from_slice(&name_len.to_le_bytes());
        raw[66] = if e.name == "Root Entry" && e.storage {
            5
        } else if e.storage {
            1
        } else {
            2
        };
        raw[67] = u8::from(e.black);
        raw[68..72].copy_from_slice(&e.left.to_le_bytes());
        raw[72..76].copy_from_slice(&e.right.to_le_bytes());
        raw[76..80].copy_from_slice(&e.child.to_le_bytes());
        let (start, size) = if e.name == "Root Entry" && e.storage {
            (
                if mini_stream.is_empty() {
                    ENDOFCHAIN
                } else {
                    e.start
                },
                mini_stream.len(),
            )
        } else if e.storage {
            (0, 0)
        } else if e.data.is_empty() {
            (ENDOFCHAIN, 0)
        } else {
            (e.start, e.data.len())
        };
        raw[116..120].copy_from_slice(&start.to_le_bytes());
        raw[120..128].copy_from_slice(&(size as u64).to_le_bytes());
        dir.extend_from_slice(&raw);
    }
    // Unused directory slots are empty entries with no siblings.
    while dir.len() < dir_sectors * SECTOR {
        let mut raw = [0u8; 128];
        raw[68..80].copy_from_slice(&[0xFF; 12]);
        dir.extend_from_slice(&raw);
    }
    out.extend(dir);
    // Mini FAT.
    let mut mf = Vec::new();
    for v in &mini_fat {
        mf.extend_from_slice(&v.to_le_bytes());
    }
    mf.resize(mini_fat_sectors * SECTOR, 0xFF);
    out.extend(mf);
    // Mini stream.
    let mut ms = mini_stream.clone();
    ms.resize(mini_stream_sectors * SECTOR, 0);
    out.extend(ms);
    // Big streams.
    for &i in &big {
        let mut d = entries[i].data.clone();
        d.resize(sectors(d.len()) * SECTOR, 0);
        out.extend(d);
    }
    out
}

fn balance(entries: &mut [DirEntry], sorted: &[usize], depth: u32, full_depth: u32) -> u32 {
    if sorted.is_empty() {
        return NOSTREAM;
    }
    let mid = sorted.len() / 2;
    let node = sorted[mid];
    entries[node].black = depth <= full_depth;
    entries[node].left = balance(entries, &sorted[..mid], depth + 1, full_depth);
    entries[node].right = balance(entries, &sorted[mid + 1..], depth + 1, full_depth);
    u32::try_from(node).unwrap_or(NOSTREAM)
}

// ---------------------------------------------------------------------------
// The VBA project
// ---------------------------------------------------------------------------

/// What kind of module, as the VBA editor groups them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModuleKind {
    /// A standard module (`Module=`).
    Standard,
    /// A class module (`Class=`).
    Class,
    /// The workbook's own code (`Document=`, VB_Base of a Workbook).
    Workbook,
    /// A sheet's code (`Document=`, VB_Base of a Worksheet).
    Sheet,
    /// A user form (`BaseClass=`), with a designer storage.
    Form,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub kind: ModuleKind,
    /// The code after the `Attribute` lines, with `\n` line ends.
    pub code: String,
    /// For a form: something to put in its designer streams, so a changed
    /// layout changes the bytes. Not a real form layout.
    pub layout: String,
}

impl Module {
    #[must_use]
    pub fn new(name: &str, kind: ModuleKind, code: &str) -> Self {
        Self {
            name: name.to_owned(),
            kind,
            code: code.to_owned(),
            layout: String::new(),
        }
    }

    /// The source as the module stream stores it: the `Attribute` lines,
    /// then the code, CRLF line ends.
    #[must_use]
    pub fn source(&self) -> String {
        let mut lines = vec![format!("Attribute VB_Name = \"{}\"", self.name)];
        let base = match self.kind {
            ModuleKind::Workbook => Some("0{00020819-0000-0000-C000-000000000046}"),
            ModuleKind::Sheet => Some("0{00020820-0000-0000-C000-000000000046}"),
            ModuleKind::Form => Some(
                "0{8A3C1E55-2D6B-4F60-9C1A-5E7B2D4F9A10}{0E4B7C2A-9D31-4A85-B6F2-3C8D1E5A7B94}",
            ),
            _ => None,
        };
        if let Some(base) = base {
            lines.push(format!("Attribute VB_Base = \"{base}\""));
        }
        if self.kind != ModuleKind::Standard {
            lines.push("Attribute VB_GlobalNameSpace = False".to_owned());
            lines.push("Attribute VB_Creatable = False".to_owned());
            let predeclared = matches!(
                self.kind,
                ModuleKind::Workbook | ModuleKind::Sheet | ModuleKind::Form
            );
            lines.push(format!(
                "Attribute VB_PredeclaredId = {}",
                if predeclared { "True" } else { "False" }
            ));
            let exposed = matches!(self.kind, ModuleKind::Workbook | ModuleKind::Sheet);
            lines.push(format!(
                "Attribute VB_Exposed = {}",
                if exposed { "True" } else { "False" }
            ));
        }
        if matches!(self.kind, ModuleKind::Workbook | ModuleKind::Sheet) {
            lines.push("Attribute VB_TemplateDerived = False".to_owned());
            lines.push("Attribute VB_Customizable = True".to_owned());
        }
        let mut out = lines.join("\r\n");
        out.push_str("\r\n");
        for line in self.code.lines() {
            out.push_str(line);
            out.push_str("\r\n");
        }
        out
    }
}

/// A reference to a type library, the way the VBA editor's References list
/// shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub name: String,
    pub libid: String,
}

impl Reference {
    #[must_use]
    pub fn stdole() -> Self {
        Self {
            name: "stdole".to_owned(),
            libid: r"*\G{00020430-0000-0000-C000-000000000046}#2.0#0#C:\Windows\System32\stdole2.tlb#OLE Automation".to_owned(),
        }
    }

    #[must_use]
    pub fn office() -> Self {
        Self {
            name: "Office".to_owned(),
            libid: r"*\G{2DF8D04C-5BFA-101B-BDE5-00AA0044DE52}#2.8#0#C:\Program Files\Common Files\Microsoft Shared\OFFICE16\MSO.DLL#Microsoft Office 16.0 Object Library".to_owned(),
        }
    }

    #[must_use]
    pub fn scripting() -> Self {
        Self {
            name: "Scripting".to_owned(),
            libid: r"*\G{420B2830-E718-11CF-893D-00A0C9054228}#1.0#0#C:\Windows\System32\scrrun.dll#Microsoft Scripting Runtime".to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    pub name: String,
    /// `{…}` in the PROJECT stream; it seeds the protection bytes.
    pub id: String,
    pub codepage: u16,
    pub modules: Vec<Module>,
    pub references: Vec<Reference>,
    /// "Lock project for viewing" (the visibility state is 0x00).
    pub locked: bool,
}

impl Project {
    #[must_use]
    pub fn new(modules: Vec<Module>) -> Self {
        Self {
            name: "VBAProject".to_owned(),
            id: "{5D2B8E41-7A3C-4F19-9B06-2E8C4D1A7F53}".to_owned(),
            codepage: 1252,
            modules,
            references: vec![Reference::stdole(), Reference::office()],
            locked: false,
        }
    }

    #[must_use]
    pub fn module(&self, name: &str) -> Option<&Module> {
        self.modules.iter().find(|m| m.name == name)
    }

    pub fn module_mut(&mut self, name: &str) -> Option<&mut Module> {
        self.modules.iter_mut().find(|m| m.name == name)
    }

    /// The `vbaProject.bin` bytes.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut streams = vec![
            CfbStream {
                path: "PROJECT".to_owned(),
                data: self.project_stream(),
            },
            CfbStream {
                path: "PROJECTwm".to_owned(),
                data: self.project_wm(),
            },
            CfbStream {
                path: "VBA/_VBA_PROJECT".to_owned(),
                // Reserved1 0x61CC, Version 0xFFFF on write, Reserved2,
                // Reserved3; no performance cache (2.3.4.1).
                data: vec![0xCC, 0x61, 0xFF, 0xFF, 0x00, 0x00, 0x00],
            },
            CfbStream {
                path: "VBA/dir".to_owned(),
                data: compress(&self.dir_stream()),
            },
        ];
        for m in &self.modules {
            streams.push(CfbStream {
                path: format!("VBA/{}", m.name),
                data: compress(&mbcs(&m.source(), self.codepage)),
            });
            if m.kind == ModuleKind::Form {
                // A designer storage named after the form. These bytes are
                // stand-ins, not a real form ([MS-OFORMS]); such a file is for
                // the engine's tests only and is never opened in Excel.
                let comp = format!("Lineage test form stand-in: {}", m.name).into_bytes();
                let frame = format!(
                    "VERSION 5.00\r\nBegin {{C62A69F0-16DC-11CE-9E98-00AA00574A4F}} {}\r\n   Caption = \"{}\"\r\nEnd\r\n",
                    m.name, m.name
                )
                .into_bytes();
                streams.push(CfbStream {
                    path: format!("{}/\u{1}CompObj", m.name),
                    data: comp,
                });
                streams.push(CfbStream {
                    path: format!("{}/\u{3}VBFrame", m.name),
                    data: frame,
                });
                streams.push(CfbStream {
                    path: format!("{}/f", m.name),
                    data: format!("layout:{}", m.layout).into_bytes(),
                });
                streams.push(CfbStream {
                    path: format!("{}/o", m.name),
                    data: vec![0; 16],
                });
            }
        }
        write_cfb(&streams)
    }

    fn project_stream(&self) -> Vec<u8> {
        let mut lines = vec![format!("ID=\"{}\"", self.id)];
        let has_form = self.modules.iter().any(|m| m.kind == ModuleKind::Form);
        for m in &self.modules {
            match m.kind {
                ModuleKind::Workbook | ModuleKind::Sheet => {
                    lines.push(format!("Document={}/&H00000000", m.name));
                }
                _ => {}
            }
        }
        if has_form {
            lines.push("Package={AC9F2F90-E877-11CE-9F68-00AA00574A4F}".to_owned());
        }
        for m in &self.modules {
            match m.kind {
                ModuleKind::Standard => lines.push(format!("Module={}", m.name)),
                ModuleKind::Class => lines.push(format!("Class={}", m.name)),
                ModuleKind::Form => lines.push(format!("BaseClass={}", m.name)),
                _ => {}
            }
        }
        lines.push(format!("Name=\"{}\"", self.name));
        lines.push("HelpContextID=\"0\"".to_owned());
        lines.push("VersionCompatible32=\"393222000\"".to_owned());
        let key = project_key(&self.id);
        lines.push(format!(
            "CMG=\"{}\"",
            hex(&encrypt(&[0, 0, 0, 0], key, 0x5B))
        ));
        lines.push(format!("DPB=\"{}\"", hex(&encrypt(&[0], key, 0x39))));
        let visibility = if self.locked { 0x00 } else { 0xFF };
        lines.push(format!(
            "GC=\"{}\"",
            hex(&encrypt(&[visibility], key, 0x1D))
        ));
        lines.push(String::new());
        lines.push("[Host Extender Info]".to_owned());
        lines.push("&H00000001={3832D640-CF90-11CF-8E43-00A0C911005A};VBE;&H00000000".to_owned());
        lines.push(String::new());
        lines.push("[Workspace]".to_owned());
        for m in &self.modules {
            lines.push(format!("{}=0, 0, 0, 0, C", m.name));
        }
        let mut out = lines.join("\r\n");
        out.push_str("\r\n");
        mbcs(&out, self.codepage)
    }

    fn project_wm(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for m in &self.modules {
            out.extend(mbcs(&m.name, self.codepage));
            out.push(0);
            for unit in m.name.encode_utf16() {
                out.extend_from_slice(&unit.to_le_bytes());
            }
            out.extend_from_slice(&[0, 0]);
        }
        out.extend_from_slice(&[0, 0]);
        out
    }

    fn dir_stream(&self) -> Vec<u8> {
        let mut d = Vec::new();
        let rec = |d: &mut Vec<u8>, id: u16, data: &[u8]| {
            d.extend_from_slice(&id.to_le_bytes());
            d.extend_from_slice(&u32::try_from(data.len()).unwrap_or(0).to_le_bytes());
            d.extend_from_slice(data);
        };
        let utf16 = |s: &str| -> Vec<u8> { s.encode_utf16().flat_map(u16::to_le_bytes).collect() };
        rec(&mut d, 0x0001, &3u32.to_le_bytes()); // SysKind: 64-bit Windows
        rec(&mut d, 0x0002, &0x0409u32.to_le_bytes());
        rec(&mut d, 0x0014, &0x0409u32.to_le_bytes());
        rec(&mut d, 0x0003, &self.codepage.to_le_bytes());
        rec(&mut d, 0x0004, &mbcs(&self.name, self.codepage));
        rec(&mut d, 0x0005, &[]);
        rec(&mut d, 0x0040, &[]);
        rec(&mut d, 0x0006, &[]);
        rec(&mut d, 0x003D, &[]);
        rec(&mut d, 0x0007, &0u32.to_le_bytes());
        rec(&mut d, 0x0008, &0u32.to_le_bytes());
        // PROJECTVERSION: Reserved (4) is 4, then major (4) and minor (2).
        d.extend_from_slice(&0x0009u16.to_le_bytes());
        d.extend_from_slice(&4u32.to_le_bytes());
        d.extend_from_slice(&1u32.to_le_bytes());
        d.extend_from_slice(&0u16.to_le_bytes());
        rec(&mut d, 0x000C, &[]);
        rec(&mut d, 0x003C, &[]);
        for r in &self.references {
            rec(&mut d, 0x0016, &mbcs(&r.name, self.codepage));
            rec(&mut d, 0x003E, &utf16(&r.name));
            let libid = mbcs(&r.libid, self.codepage);
            let mut reg = u32::try_from(libid.len())
                .unwrap_or(0)
                .to_le_bytes()
                .to_vec();
            reg.extend(libid);
            reg.extend_from_slice(&[0; 6]);
            rec(&mut d, 0x000D, &reg);
        }
        rec(
            &mut d,
            0x000F,
            &u16::try_from(self.modules.len()).unwrap_or(0).to_le_bytes(),
        );
        rec(&mut d, 0x0013, &0xFFFFu16.to_le_bytes());
        for m in &self.modules {
            rec(&mut d, 0x0019, &mbcs(&m.name, self.codepage));
            rec(&mut d, 0x0047, &utf16(&m.name));
            rec(&mut d, 0x001A, &mbcs(&m.name, self.codepage));
            rec(&mut d, 0x0032, &utf16(&m.name));
            rec(&mut d, 0x001C, &[]);
            rec(&mut d, 0x0048, &[]);
            rec(&mut d, 0x0031, &0u32.to_le_bytes()); // no performance cache
            rec(&mut d, 0x001E, &0u32.to_le_bytes());
            rec(&mut d, 0x002C, &0xFFFFu16.to_le_bytes());
            let kind = if m.kind == ModuleKind::Standard {
                0x0021
            } else {
                0x0022
            };
            rec(&mut d, kind, &[]);
            rec(&mut d, 0x002B, &[]);
        }
        rec(&mut d, 0x0010, &[]);
        d
    }
}

/// Text in the project's code page. Fixtures use 1252 and 65001 only.
fn mbcs(text: &str, codepage: u16) -> Vec<u8> {
    if codepage == 65001 {
        return text.as_bytes().to_vec();
    }
    text.chars()
        .map(|c| {
            let n = u32::from(c);
            match n {
                0..=0x7F | 0xA0..=0xFF => u8::try_from(n).unwrap_or(b'?'),
                0x20AC => 0x80,
                0x2018 => 0x91,
                0x2019 => 0x92,
                0x201C => 0x93,
                0x201D => 0x94,
                0x2013 => 0x96,
                0x2014 => 0x97,
                _ => b'?',
            }
        })
        .collect()
}

/// The project key (2.4.3.2): the sum of the project id's bytes.
fn project_key(id: &str) -> u8 {
    id.bytes().fold(0u8, u8::wrapping_add)
}

/// The data encryption of 2.4.3.2, with the ignored bytes all zero.
#[must_use]
pub fn encrypt(data: &[u8], key: u8, seed: u8) -> Vec<u8> {
    let version: u8 = 2;
    let version_enc = seed ^ version;
    let key_enc = seed ^ key;
    let mut out = vec![seed, version_enc, key_enc];
    let mut unenc1 = key;
    let mut enc1 = key_enc;
    let mut enc2 = version_enc;
    let mut push = |out: &mut Vec<u8>, byte: u8| {
        let e = byte ^ enc2.wrapping_add(unenc1);
        out.push(e);
        enc2 = enc1;
        enc1 = e;
        unenc1 = byte;
    };
    let ignored = (seed & 6) / 2;
    for _ in 0..ignored {
        push(&mut out, 0);
    }
    for byte in u32::try_from(data.len()).unwrap_or(0).to_le_bytes() {
        push(&mut out, byte);
    }
    for &byte in data {
        push(&mut out, byte);
    }
    out
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    for b in bytes {
        let _ = write!(s, "{b:02X}");
    }
    s
}

// ---------------------------------------------------------------------------
// A signature part
// ---------------------------------------------------------------------------

fn der(tag: u8, content: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    let len = content.len();
    if len < 0x80 {
        out.push(u8::try_from(len).unwrap_or(0));
    } else if len < 0x100 {
        out.extend_from_slice(&[0x81, u8::try_from(len).unwrap_or(0)]);
    } else {
        let l = u16::try_from(len).unwrap_or(0);
        out.push(0x82);
        out.extend_from_slice(&l.to_be_bytes());
    }
    out.extend_from_slice(content);
    out
}

fn seq(items: &[Vec<u8>]) -> Vec<u8> {
    der(0x30, &items.concat())
}

fn name(common: &str) -> Vec<u8> {
    // Name ::= SEQUENCE OF SET OF SEQUENCE { OID 2.5.4.3, UTF8String }
    let atv = seq(&[der(0x06, &[0x55, 0x04, 0x03]), der(0x0C, common.as_bytes())]);
    seq(&[der(0x31, &atv)])
}

/// A `vbaProjectSignature.bin` part: the DigSigInfoSerialized wrapper
/// around a PKCS#7 SignedData with one certificate, for `signer` issued by
/// `issuer`. The signature bytes are not a real signature: Lineage only
/// reads who signed, it never checks a signature.
#[must_use]
pub fn signature_part(signer: &str, issuer: &str) -> Vec<u8> {
    let serial = der(0x02, &[0x01, 0x23, 0x45]);
    let algorithm = seq(&[
        der(
            0x06,
            &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x0B],
        ),
        der(0x05, &[]),
    ]);
    let validity = seq(&[der(0x17, b"260101000000Z"), der(0x17, b"280101000000Z")]);
    let key_info = seq(&[algorithm.clone(), der(0x03, &[0x00, 0x30, 0x00])]);
    let tbs = seq(&[
        der(0xA0, &der(0x02, &[0x02])),
        serial.clone(),
        algorithm.clone(),
        name(issuer),
        validity,
        name(signer),
        key_info,
    ]);
    let cert = seq(&[tbs, algorithm.clone(), der(0x03, &[0x00; 9])]);
    let signer_info = seq(&[
        der(0x02, &[0x01]),
        seq(&[name(issuer), serial]),
        seq(&[der(
            0x06,
            &[0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01],
        )]),
        algorithm,
        der(0x04, &[0x5A; 16]),
    ]);
    let signed_data = seq(&[
        der(0x02, &[0x01]),
        der(0x31, &[]),
        seq(&[der(
            0x06,
            &[0x2B, 0x06, 0x01, 0x04, 0x01, 0x82, 0x37, 0x02, 0x01, 0x04],
        )]),
        der(0xA0, &cert),
        der(0x31, &signer_info),
    ]);
    let content_info = seq(&[
        der(
            0x06,
            &[0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x07, 0x02],
        ),
        der(0xA0, &signed_data),
    ]);
    // DigSigInfoSerialized ([MS-OSHARED] 2.3.2.2): sizes and offsets, then
    // the signature, an empty certificate store and the project name.
    let header = 9 * 4;
    let sig_len = content_info.len();
    let project: Vec<u8> = "VBAProject\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut out = Vec::new();
    let mut put = |v: usize| out.extend_from_slice(&u32::try_from(v).unwrap_or(0).to_le_bytes());
    put(sig_len);
    put(header);
    put(0);
    put(header + sig_len);
    put(project.len());
    put(header + sig_len);
    put(0);
    put(0);
    put(header + sig_len + project.len());
    out.extend(content_info);
    out.extend(project);
    out
}

/// Every stream path in a compound file written by [`write_cfb`], for tests.
#[must_use]
pub fn paths(streams: &[CfbStream]) -> BTreeSet<String> {
    streams.iter().map(|s| s.path.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_token_split_follows_the_spec() {
        // 2.4.1.3.19.1: at least 4 bits for the offset.
        assert_eq!(copy_token_help(1), (0x0FFF, 0xF000, 4, 4098));
        assert_eq!(copy_token_help(16), (0x0FFF, 0xF000, 4, 4098));
        assert_eq!(copy_token_help(17), (0x07FF, 0xF800, 5, 2050));
        assert_eq!(copy_token_help(4096), (0x000F, 0xFFF0, 12, 18));
    }

    #[test]
    fn spec_examples_compress_exactly() {
        // MS-OVBA 3.2.1 to 3.2.3.
        assert_eq!(
            compress(b"abcdefghijklmnopqrstuv."),
            [
                0x01, 0x19, 0xB0, 0x00, 0x61, 0x62, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x00, 0x69,
                0x6A, 0x6B, 0x6C, 0x6D, 0x6E, 0x6F, 0x70, 0x00, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76,
                0x2E
            ]
        );
        assert_eq!(
            compress(b"#aaabcdefaaaaghijaaaaaklaaamnopqaaaaaaaaaaaarstuvwxyzaaa"),
            [
                0x01, 0x2F, 0xB0, 0x00, 0x23, 0x61, 0x61, 0x61, 0x62, 0x63, 0x64, 0x65, 0x82, 0x66,
                0x00, 0x70, 0x61, 0x67, 0x68, 0x69, 0x6A, 0x01, 0x38, 0x08, 0x61, 0x6B, 0x6C, 0x00,
                0x20, 0x6D, 0x6E, 0x6F, 0x70, 0x06, 0x71, 0x02, 0x70, 0x04, 0x00, 0x72, 0x73, 0x74,
                0x75, 0x76, 0x10, 0x77, 0x78, 0x79, 0x7A, 0x00, 0x2C
            ]
        );
        assert_eq!(
            compress(&[b'a'; 73]),
            [0x01, 0x03, 0xB0, 0x02, 0x61, 0x45, 0x00]
        );
    }

    #[test]
    fn repeated_text_compresses() {
        let text = "Sub RefreshForecast()\r\n".repeat(50);
        let packed = compress(text.as_bytes());
        assert_eq!(packed[0], 1);
        assert!(packed.len() < text.len() / 4);
    }

    #[test]
    fn compound_file_header_is_well_formed() {
        let bytes = write_cfb(&[
            CfbStream {
                path: "VBA/dir".to_owned(),
                data: vec![1; 100],
            },
            CfbStream {
                path: "PROJECT".to_owned(),
                data: vec![2; 5000],
            },
        ]);
        assert_eq!(
            &bytes[..8],
            &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]
        );
        assert_eq!(bytes.len() % SECTOR, 0);
    }

    #[test]
    fn encryption_round_trips_by_hand() {
        // Decrypt with the 2.4.3.3 steps and check the data comes back.
        let enc = encrypt(&[0xFF], project_key("{X}"), 0x1D);
        let seed = enc[0];
        let version = seed ^ enc[1];
        let key = seed ^ enc[2];
        assert_eq!(version, 2);
        let (mut unenc1, mut enc1, mut enc2) = (key, enc[2], enc[1]);
        let mut plain = Vec::new();
        for &e in &enc[3..] {
            let b = e ^ enc2.wrapping_add(unenc1);
            enc2 = enc1;
            enc1 = e;
            unenc1 = b;
            plain.push(b);
        }
        let ignored = usize::from((seed & 6) / 2);
        assert_eq!(&plain[ignored..ignored + 4], &1u32.to_le_bytes());
        assert_eq!(plain[ignored + 4], 0xFF);
    }
}
