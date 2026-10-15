// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

use std::io::{Cursor, Read};

use lineage_testkit::{fixtures, zip_writer};

use super::*;
use crate::error::Error;

fn sample() -> Vec<u8> {
    fixtures::all()
        .into_iter()
        .find(|(name, _)| *name == "headings-paragraphs.docx")
        .expect("fixture exists")
        .1
        .to_bytes()
}

/// Reads every entry with the independent `zip` crate (its own inflater and CRC check).
fn independent_parts(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).expect("independent reader opens it");
    (0..archive.len())
        .map(|index| {
            let mut file = archive.by_index(index).expect("entry");
            let name = file.name().expect("name").to_string();
            let mut data = Vec::new();
            file.read_to_end(&mut data).expect("reads and CRC matches");
            (name, data)
        })
        .collect()
}

#[test]
fn unchanged_round_trip_is_byte_identical() {
    let bytes = sample();
    let package = Package::read(&bytes).expect("reads");
    let written = package.to_bytes().expect("writes");
    // The fixture writer and ours lay out stored entries the same way.
    assert_eq!(written, bytes);
}

#[test]
fn changing_one_part_keeps_the_others_compressed_bytes() {
    let mut package = Package::read(&sample()).expect("reads");
    let styles_before = package
        .original_entry("word/styles.xml")
        .map(|(m, raw)| (m, raw.to_vec()));
    let mut document = package
        .part("word/document.xml")
        .expect("document")
        .to_vec();
    document.extend_from_slice(b"<!-- edited -->");
    package.set_part("word/document.xml", document.clone());

    let reread = Package::read(&package.to_bytes().expect("writes")).expect("rereads");
    assert_eq!(reread.part("word/document.xml"), Some(document.as_slice()));
    assert_eq!(
        reread
            .original_entry("word/styles.xml")
            .map(|(m, raw)| (m, raw.to_vec())),
        styles_before
    );
    // The changed part was compressed afresh.
    assert_eq!(
        reread.original_entry("word/document.xml").map(|(m, _)| m),
        Some(8)
    );
}

#[test]
fn writing_is_deterministic_and_idempotent() {
    let mut package = Package::read(&sample()).expect("reads");
    package.set_part("word/new.xml", b"<new/>".repeat(100));
    let once = package.to_bytes().expect("writes");
    let twice = Package::read(&once)
        .expect("rereads")
        .to_bytes()
        .expect("writes again");
    assert_eq!(once, twice);
}

#[test]
fn independent_reader_accepts_our_output() {
    let mut package = Package::read(&sample()).expect("reads");
    package.set_part(
        "word/document.xml",
        "<a>é ünïcode</a>".repeat(500).into_bytes(),
    );
    package.set_part("docProps/custom.xml", b"<x/>".to_vec());
    let bytes = package.to_bytes().expect("writes");
    let ours: Vec<(String, Vec<u8>)> = {
        let reread = Package::read(&bytes).expect("rereads");
        reread
            .part_names()
            .map(|name| (name.to_owned(), reread.part(name).expect("part").to_vec()))
            .collect()
    };
    assert_eq!(independent_parts(&bytes), ours);
}

#[test]
fn setting_the_same_bytes_keeps_the_original_entry() {
    let mut package = Package::read(&sample()).expect("reads");
    let same = package.part("word/styles.xml").expect("styles").to_vec();
    package.set_part("word/styles.xml", same);
    assert!(package.is_unchanged("word/styles.xml"));
}

#[test]
fn names_are_matched_case_insensitively() {
    let package = Package::read(&sample()).expect("reads");
    assert!(package.contains("/Word/Document.XML"));
    assert_eq!(
        package.canonical_name("WORD/styles.xml"),
        Some("word/styles.xml")
    );
}

#[test]
fn remove_part() {
    let mut package = Package::read(&sample()).expect("reads");
    assert!(package.remove_part("docProps/app.xml"));
    assert!(!package.remove_part("docProps/app.xml"));
    let reread = Package::read(&package.to_bytes().expect("writes")).expect("rereads");
    assert!(!reread.contains("docProps/app.xml"));
}

fn entries(list: &[(&str, &[u8])]) -> Vec<(String, Vec<u8>)> {
    list.iter()
        .map(|(n, d)| ((*n).to_owned(), d.to_vec()))
        .collect()
}

#[test]
fn rejects_bad_checksum() {
    let mut bytes = zip_writer::write_stored(&entries(&[("a.xml", b"<hello/>")]));
    // The data starts after the 30-byte local header and the 5-byte name.
    bytes[35] ^= 0x20;
    assert!(matches!(Package::read(&bytes), Err(Error::Damaged(_))));
}

#[test]
fn rejects_truncated_and_non_zip_input() {
    let bytes = sample();
    assert!(Package::read(&bytes[..bytes.len() / 2]).is_err());
    assert!(matches!(
        Package::read(b"not a zip at all"),
        Err(Error::NotAZip(_))
    ));
    assert!(Package::read(&[]).is_err());
}

#[test]
fn rejects_duplicate_names_differing_only_by_case() {
    let bytes = zip_writer::write_stored(&entries(&[("a.xml", b"<a/>"), ("A.XML", b"<b/>")]));
    assert!(matches!(Package::read(&bytes), Err(Error::Damaged(_))));
}

/// Patches a field in the first local header and its central directory record.
fn patch(bytes: &mut [u8], local_offset: usize, central_offset_from_cd: usize, value: &[u8]) {
    bytes[local_offset..local_offset + value.len()].copy_from_slice(value);
    let cd = find(bytes, &[0x50, 0x4B, 0x01, 0x02]).expect("central directory");
    let at = cd + central_offset_from_cd;
    bytes[at..at + value.len()].copy_from_slice(value);
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[test]
fn rejects_encrypted_entries() {
    let mut bytes = zip_writer::write_stored(&entries(&[("a.xml", b"<a/>")]));
    patch(&mut bytes, 6, 8, &1_u16.to_le_bytes());
    assert_eq!(Package::read(&bytes), Err(Error::Encrypted));
}

#[test]
fn rejects_unknown_compression() {
    let mut bytes = zip_writer::write_stored(&entries(&[("a.xml", b"<a/>")]));
    patch(&mut bytes, 8, 10, &12_u16.to_le_bytes()); // bzip2
    assert!(matches!(Package::read(&bytes), Err(Error::Unsupported(_))));
}

#[test]
fn rejects_parts_over_the_size_limit() {
    let mut bytes = zip_writer::write_stored(&entries(&[("a.xml", b"<a/>")]));
    let huge = u32::try_from(MAX_PART_BYTES + 1)
        .expect("fits")
        .to_le_bytes();
    patch(&mut bytes, 22, 24, &huge);
    assert!(matches!(Package::read(&bytes), Err(Error::TooLarge(_))));
}

#[test]
fn rejects_a_deflate_stream_longer_than_declared() {
    let data = b"<a>".repeat(1000);
    let deflated = miniz_oxide::deflate::compress_to_vec(&data, 6);
    // Build a deflated entry by hand that claims a smaller size than it inflates to.
    let mut package =
        Package::read(&zip_writer::write_stored(&entries(&[("a.xml", &data)]))).expect("reads");
    package.set_part("a.xml", data.clone());
    package.set_part("b.xml", data.clone());
    let mut bytes = package.to_bytes().expect("writes");
    assert!(
        find(&bytes, &deflated[..16]).is_some(),
        "entry was deflated"
    );
    patch(&mut bytes, 22, 24, &10_u32.to_le_bytes());
    assert!(matches!(Package::read(&bytes), Err(Error::Damaged(_))));
}

#[test]
fn every_synthetic_fixture_round_trips() {
    for (name, fixture) in fixtures::all() {
        let bytes = fixture.to_bytes();
        let package = Package::read(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        let reread = Package::read(&package.to_bytes().expect("writes")).expect("rereads");
        let names: Vec<&str> = package.part_names().collect();
        assert_eq!(names, reread.part_names().collect::<Vec<_>>(), "{name}");
        for part in names {
            assert_eq!(package.part(part), reread.part(part), "{name}/{part}");
            assert_eq!(
                package.original_entry(part),
                reread.original_entry(part),
                "{name}/{part}"
            );
        }
    }
}
