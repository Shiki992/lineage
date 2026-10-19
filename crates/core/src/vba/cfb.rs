// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Reading compound files ([MS-CFB]), the container `vbaProject.bin` is.
//!
//! A returned file is untrusted, so every sector number, chain and size is
//! checked before it is followed: chains may not loop or run past the file,
//! the directory tree may not loop, and streams may not claim more bytes
//! than their chains hold.
//!
//! [MS-CFB]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-cfb/53989ce4-7b05-4f8d-829b-d08d6148375b

use crate::error::{Error, Result};

const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const MAXREGSECT: u32 = 0xFFFF_FFFA;
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const NOSTREAM: u32 = 0xFFFF_FFFF;
const MINI_SECTOR: usize = 64;

/// Largest compound file the engine reads.
pub const MAX_FILE_BYTES: usize = 64 * 1024 * 1024;
/// Most directory entries.
pub const MAX_ENTRIES: usize = 65_536;
/// Deepest storage nesting.
const MAX_DEPTH: usize = 32;

fn damaged(detail: &str) -> Error {
    Error::Damaged(format!("compound file: {detail}"))
}

/// A stream or storage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// "VBA/dir"; storages have no data.
    pub path: String,
    pub storage: bool,
    pub data: Vec<u8>,
}

/// Every entry of a compound file except the root, with its path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cfb {
    pub entries: Vec<Entry>,
}

impl Cfb {
    /// A stream by path, ignoring case as compound files do.
    #[must_use]
    pub fn stream(&self, path: &str) -> Option<&[u8]> {
        self.entries
            .iter()
            .find(|e| !e.storage && e.path.eq_ignore_ascii_case(path))
            .map(|e| e.data.as_slice())
    }

    /// The streams inside a storage (and its storages), in path order.
    pub fn under<'a>(&'a self, storage: &'a str) -> impl Iterator<Item = &'a Entry> + 'a {
        self.entries.iter().filter(move |e| {
            !e.storage
                && e.path.len() > storage.len()
                && e.path.as_bytes()[storage.len()] == b'/'
                && e.path[..storage.len()].eq_ignore_ascii_case(storage)
        })
    }

    /// Whether a storage of this name sits at the root.
    #[must_use]
    pub fn has_storage(&self, path: &str) -> bool {
        self.entries
            .iter()
            .any(|e| e.storage && e.path.eq_ignore_ascii_case(path))
    }
}

fn u16_at(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn u32_at(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn to_usize(v: u32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}

struct Reader<'a> {
    bytes: &'a [u8],
    sector: usize,
    sectors: usize,
    fat: Vec<u32>,
}

impl Reader<'_> {
    fn sector_bytes(&self, n: u32) -> Result<&[u8]> {
        let n = to_usize(n);
        if n >= self.sectors {
            return Err(damaged("a sector past the end"));
        }
        let start = (n + 1) * self.sector;
        Ok(&self.bytes[start..start + self.sector])
    }

    /// The sectors of a chain, never more than the file holds.
    fn chain(&self, first: u32) -> Result<Vec<u32>> {
        let mut out = Vec::new();
        let mut seen = vec![false; self.sectors];
        let mut at = first;
        while at != ENDOFCHAIN {
            if at > MAXREGSECT {
                return Err(damaged("a chain with a reserved sector"));
            }
            let i = to_usize(at);
            if i >= self.sectors || i >= self.fat.len() {
                return Err(damaged("a chain past the end"));
            }
            if seen[i] {
                return Err(damaged("a chain that loops"));
            }
            seen[i] = true;
            out.push(at);
            at = self.fat[i];
        }
        Ok(out)
    }

    fn read_chain(&self, first: u32, size: usize) -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(size.min(MAX_FILE_BYTES));
        for s in self.chain(first)? {
            out.extend_from_slice(self.sector_bytes(s)?);
            if out.len() >= size {
                break;
            }
        }
        if out.len() < size {
            return Err(damaged("a stream longer than its chain"));
        }
        out.truncate(size);
        Ok(out)
    }
}

struct Raw {
    name: String,
    kind: u8,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    size: u64,
}

/// Reads a compound file.
///
/// # Errors
/// [`Error::Damaged`] for anything malformed, [`Error::TooLarge`] past the
/// limits.
#[allow(clippy::too_many_lines)]
pub fn read(bytes: &[u8]) -> Result<Cfb> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Error::TooLarge("a compound file is too large".to_owned()));
    }
    if bytes.len() < 512 || bytes[..8] != SIGNATURE {
        return Err(damaged("no compound file signature"));
    }
    if u16_at(bytes, 28) != 0xFFFE {
        return Err(damaged("wrong byte order"));
    }
    let major = u16_at(bytes, 26);
    let shift = u16_at(bytes, 30);
    let sector = match (major, shift) {
        (3, 9) => 512,
        (4, 12) => 4096,
        _ => return Err(damaged("unknown version or sector size")),
    };
    if u16_at(bytes, 32) != 6 {
        return Err(damaged("unknown mini sector size"));
    }
    let cutoff = to_usize(u32_at(bytes, 56));
    if cutoff != 4096 {
        return Err(damaged("unknown mini stream cutoff"));
    }
    if bytes.len() < sector * 2 {
        return Err(damaged("too short"));
    }
    // A last partial sector is allowed and ignored.
    let sectors = bytes.len() / sector - 1;
    let fat_count = to_usize(u32_at(bytes, 44));
    let dir_start = u32_at(bytes, 48);
    let mini_fat_start = u32_at(bytes, 60);
    let mini_fat_count = to_usize(u32_at(bytes, 64));
    let difat_start = u32_at(bytes, 68);
    let difat_count = to_usize(u32_at(bytes, 72));
    if fat_count > sectors || difat_count > sectors || mini_fat_count > sectors {
        return Err(damaged("more table sectors than the file has"));
    }

    // The FAT's own sectors, from the header's DIFAT and its chain.
    let mut fat_sectors = Vec::new();
    for k in 0..109 {
        let v = u32_at(bytes, 76 + k * 4);
        if v <= MAXREGSECT {
            fat_sectors.push(v);
        }
    }
    let mut reader = Reader {
        bytes,
        sector,
        sectors,
        fat: Vec::new(),
    };
    let mut next = difat_start;
    let mut seen = 0;
    while next <= MAXREGSECT && fat_sectors.len() < fat_count {
        seen += 1;
        if seen > difat_count.max(1) || seen > sectors {
            return Err(damaged("the DIFAT chain loops"));
        }
        let s = reader.sector_bytes(next)?;
        let per = sector / 4 - 1;
        for k in 0..per {
            let v = u32_at(s, k * 4);
            if v <= MAXREGSECT {
                fat_sectors.push(v);
            }
        }
        next = u32_at(s, per * 4);
    }
    fat_sectors.truncate(fat_count);
    let mut fat = Vec::with_capacity(fat_sectors.len() * sector / 4);
    for &s in &fat_sectors {
        let data = reader.sector_bytes(s)?;
        for k in 0..sector / 4 {
            fat.push(u32_at(data, k * 4));
        }
    }
    reader.fat = fat;

    // The directory.
    let dir = {
        let chain = reader.chain(dir_start)?;
        if chain.len() * sector / 128 > MAX_ENTRIES {
            return Err(Error::TooLarge("too many compound file entries".to_owned()));
        }
        let mut out = Vec::new();
        for s in chain {
            out.extend_from_slice(reader.sector_bytes(s)?);
        }
        out
    };
    let mut raws = Vec::new();
    for e in dir.as_chunks::<128>().0 {
        let name_len = usize::from(u16_at(e, 64));
        let units = (name_len / 2).saturating_sub(1).min(31);
        let name: Vec<u16> = (0..units).map(|k| u16_at(e, k * 2)).collect();
        let size = u64::from(u32_at(e, 120))
            | if sector == 4096 {
                u64::from(u32_at(e, 124)) << 32
            } else {
                0
            };
        raws.push(Raw {
            name: String::from_utf16_lossy(&name),
            kind: e[66],
            left: u32_at(e, 68),
            right: u32_at(e, 72),
            child: u32_at(e, 76),
            start: u32_at(e, 116),
            size,
        });
    }
    let root = raws.first().ok_or_else(|| damaged("no root entry"))?;
    if root.kind != 5 {
        return Err(damaged("the first entry is not the root"));
    }

    // The mini stream and its FAT.
    let mini_stream = if root.size > 0 {
        let size = usize::try_from(root.size).map_err(|_| damaged("mini stream size"))?;
        if size > MAX_FILE_BYTES {
            return Err(damaged("mini stream too large"));
        }
        reader.read_chain(root.start, size)?
    } else {
        Vec::new()
    };
    let mini_fat = if mini_fat_count > 0 {
        let raw = reader.read_chain(mini_fat_start, mini_fat_count * sector)?;
        raw.as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect()
    } else {
        Vec::new()
    };
    let mini_count = mini_stream.len() / MINI_SECTOR;
    let read_mini = |first: u32, size: usize| -> Result<Vec<u8>> {
        let mut out = Vec::with_capacity(size);
        let mut seen = vec![false; mini_count];
        let mut at = first;
        while out.len() < size {
            let i = to_usize(at);
            if at > MAXREGSECT || i >= mini_count || i >= mini_fat.len() {
                return Err(damaged("a mini chain past the end"));
            }
            if seen[i] {
                return Err(damaged("a mini chain that loops"));
            }
            seen[i] = true;
            out.extend_from_slice(&mini_stream[i * MINI_SECTOR..(i + 1) * MINI_SECTOR]);
            at = mini_fat[i];
        }
        out.truncate(size);
        Ok(out)
    };

    // Walk the tree from the root's child, never visiting an entry twice.
    let mut entries = Vec::new();
    let mut visited = vec![false; raws.len()];
    visited[0] = true;
    let mut total: usize = 0;
    // (entry, parent path, depth)
    let mut stack: Vec<(u32, String, usize)> = vec![(root.child, String::new(), 1)];
    while let Some((id, parent, depth)) = stack.pop() {
        if id == NOSTREAM {
            continue;
        }
        let i = to_usize(id);
        let raw = raws
            .get(i)
            .ok_or_else(|| damaged("an entry past the end"))?;
        if visited[i] {
            return Err(damaged("the directory tree loops"));
        }
        visited[i] = true;
        if depth > MAX_DEPTH {
            return Err(damaged("storages nested too deep"));
        }
        stack.push((raw.left, parent.clone(), depth));
        stack.push((raw.right, parent.clone(), depth));
        let path = if parent.is_empty() {
            raw.name.clone()
        } else {
            format!("{parent}/{}", raw.name)
        };
        match raw.kind {
            1 => {
                stack.push((raw.child, path.clone(), depth + 1));
                entries.push(Entry {
                    path,
                    storage: true,
                    data: Vec::new(),
                });
            }
            2 => {
                let size = usize::try_from(raw.size).map_err(|_| damaged("stream size"))?;
                total = total.saturating_add(size);
                if size > MAX_FILE_BYTES || total > MAX_FILE_BYTES * 2 {
                    return Err(Error::TooLarge(
                        "compound file streams too large".to_owned(),
                    ));
                }
                let data = if size == 0 {
                    Vec::new()
                } else if size < cutoff {
                    read_mini(raw.start, size)?
                } else {
                    reader.read_chain(raw.start, size)?
                };
                entries.push(Entry {
                    path,
                    storage: false,
                    data,
                });
            }
            // Unknown or unallocated entries in the tree are skipped.
            _ => {}
        }
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(Cfb { entries })
}
