// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Helpers shared by the engine's unit tests.

use std::path::PathBuf;

use lineage_testkit::docx::Package;
use lineage_testkit::{fixtures, resume};

use crate::docx::{Docx, View};

/// A synthetic fixture's builder, by file name.
pub fn fixture(name: &str) -> Package {
    fixtures::all()
        .into_iter()
        .find(|(n, _)| *n == name)
        .unwrap_or_else(|| panic!("no fixture {name}"))
        .1
}

/// A resume built from lines, as bytes.
pub fn resume_bytes(lines: &[resume::Line]) -> Vec<u8> {
    resume::package("Test resume", lines).to_bytes()
}

pub fn open(bytes: &[u8]) -> Docx {
    Docx::open(bytes).unwrap_or_else(|e| panic!("opens: {e}"))
}

pub fn open_view(bytes: &[u8], view: View) -> Docx {
    Docx::open_view(bytes, view).unwrap_or_else(|e| panic!("opens: {e}"))
}

pub fn open_lines(lines: &[resume::Line]) -> Docx {
    open(&resume_bytes(lines))
}

/// Writes an output for the Word smoke test when `LINEAGE_SMOKE_DIR` is set,
/// with the paragraph text Word should show once every revision is accepted and
/// once every revision is rejected.
pub fn smoke(name: &str, bytes: &[u8]) {
    let Some(dir) = std::env::var_os("LINEAGE_SMOKE_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("smoke folder");
    std::fs::write(dir.join(format!("{name}.docx")), bytes).expect("write output");
    // Word updates DATE fields when it opens a file, and shows maths and symbols
    // in its own characters, so those files are opened but not compared.
    let docx = open(bytes);
    let main = docx.part_text(&docx.main_part).unwrap_or_default();
    if [" DATE ", "oMath", ":sym "]
        .iter()
        .any(|s| main.contains(s))
    {
        return;
    }
    for (view, suffix) in [(View::Accepted, "accepted"), (View::Rejected, "rejected")] {
        // Line breaks inside a paragraph are written the way Word shows them
        // (U+000B), so the file has exactly one line per paragraph.
        let lines: Vec<String> = open_view(bytes, view)
            .body_lines()
            .into_iter()
            .map(|line| line.replace('\n', "\u{b}"))
            .collect();
        let lines = lines.join("\n");
        std::fs::write(dir.join(format!("{name}.{suffix}.txt")), lines)
            .expect("write expected text");
    }
}

/// Writes a deck for the PowerPoint smoke test when `LINEAGE_SMOKE_DIR` is set,
/// with the text PowerPoint should show, slide by slide (`Deck::lines`).
pub fn smoke_deck(name: &str, bytes: &[u8]) {
    let Some(dir) = std::env::var_os("LINEAGE_SMOKE_DIR") else {
        return;
    };
    let dir = PathBuf::from(dir);
    std::fs::create_dir_all(&dir).expect("smoke folder");
    std::fs::write(dir.join(format!("{name}.pptx")), bytes).expect("write output");
    let deck = crate::Deck::open(bytes).unwrap_or_else(|e| panic!("opens: {e}"));
    let lines: Vec<String> = deck
        .lines()
        .into_iter()
        .map(|line| line.replace('\n', "\u{b}"))
        .collect();
    std::fs::write(dir.join(format!("{name}.expected.txt")), lines.join("\n"))
        .expect("write expected text");
    // What PowerPoint shows through COM (scripts/powerpoint-smoke.ps1): per
    // slide, its number and whether it is hidden, then the non-empty
    // paragraphs of its text shapes in order. Charts, tables and groups are
    // compared by the engine's own checks instead.
    let mut seen = Vec::new();
    for (index, slide) in deck.slides.iter().enumerate() {
        seen.push(format!(
            "# Slide {}{}",
            index + 1,
            if slide.hidden { " (hidden)" } else { "" }
        ));
        for shape in slide
            .shapes
            .iter()
            .filter(|s| s.kind == crate::pptx::ShapeKind::Text)
        {
            seen.extend(
                shape
                    .paras
                    .iter()
                    .map(|p| p.text.clone())
                    .filter(|t| !t.trim().is_empty()),
            );
        }
    }
    std::fs::write(dir.join(format!("{name}.powerpoint.txt")), seen.join("\n"))
        .expect("write PowerPoint text");
}
