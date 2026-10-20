// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! The PROJECT stream ([MS-OVBA] 2.3.1): which modules are documents
//! (workbook and sheet code), standard modules, classes and forms, and
//! whether the project is locked for viewing.
//!
//! [MS-OVBA]: https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-ovba/d88cb9d8-a475-423d-b370-cc0caaf78628

/// What the PROJECT stream says about a module.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Declared {
    Document,
    Standard,
    Class,
    Designer,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectInfo {
    /// Module names (as bytes in the code page) and what they are.
    pub modules: Vec<(Vec<u8>, Declared)>,
    /// "Lock project for viewing": the visibility state decrypts to 0x00.
    pub locked: bool,
}

/// Reads the properties section of the PROJECT stream, up to its first
/// `[` section. Lines it doesn't know are skipped.
#[must_use]
pub fn parse(stream: &[u8]) -> ProjectInfo {
    let mut out = ProjectInfo::default();
    for line in stream.split(|&b| b == b'\n') {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.first() == Some(&b'[') {
            break;
        }
        let Some(eq) = line.iter().position(|&b| b == b'=') else {
            continue;
        };
        let (key, value) = (&line[..eq], &line[eq + 1..]);
        let declared = match key {
            b"Document" => Some(Declared::Document),
            b"Module" => Some(Declared::Standard),
            b"Class" => Some(Declared::Class),
            b"BaseClass" => Some(Declared::Designer),
            _ => None,
        };
        if let Some(kind) = declared {
            // "Document=Sheet1/&H00000000"
            let name = match kind {
                Declared::Document => value.split(|&b| b == b'/').next().unwrap_or(value),
                _ => value,
            };
            out.modules.push((name.to_vec(), kind));
        } else if key == b"GC" {
            let hex = value.strip_prefix(b"\"").unwrap_or(value);
            let hex = hex.strip_suffix(b"\"").unwrap_or(hex);
            if let Some(data) = unhex(hex).and_then(|enc| decrypt(&enc)) {
                out.locked = data.first() == Some(&0x00);
            }
        }
    }
    out
}

fn unhex(hex: &[u8]) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) || hex.len() > 1024 {
        return None;
    }
    let digit = |c: u8| match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    };
    hex.as_chunks::<2>()
        .0
        .iter()
        .map(|p| Some((digit(p[0])? << 4) | digit(p[1])?))
        .collect()
}

/// The data decryption of [MS-OVBA] 2.4.3.3. `None` when the structure
/// doesn't hold together.
#[must_use]
pub fn decrypt(enc: &[u8]) -> Option<Vec<u8>> {
    if enc.len() < 3 {
        return None;
    }
    let seed = enc[0];
    let version = seed ^ enc[1];
    let key = seed ^ enc[2];
    if version != 2 {
        return None;
    }
    let mut unenc1 = key;
    let mut enc1 = enc[2];
    let mut enc2 = enc[1];
    let mut plain = Vec::with_capacity(enc.len());
    for &e in &enc[3..] {
        let b = e ^ enc2.wrapping_add(unenc1);
        enc2 = enc1;
        enc1 = e;
        unenc1 = b;
        plain.push(b);
    }
    let ignored = usize::from((seed & 6) / 2);
    let length_at = ignored;
    if plain.len() < length_at + 4 {
        return None;
    }
    let length = usize::try_from(u32::from_le_bytes([
        plain[length_at],
        plain[length_at + 1],
        plain[length_at + 2],
        plain[length_at + 3],
    ]))
    .ok()?;
    let data = &plain[length_at + 4..];
    (data.len() == length).then(|| data.to_vec())
}
