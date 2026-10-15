// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Did a write land as written? The task pane hands the engine's output to
//! Word, which re-saves it in its own way, so the bytes never match. What must
//! match is what the person sees: the text of every story once every revision
//! is accepted, the text once every revision is rejected, and Lineage's
//! revisions still being Lineage's. If Word dropped, accepted or re-attributed
//! a revision, or lost a paragraph, this says so and the host puts the original
//! back (decision 0008).

use std::collections::BTreeMap;

use crate::docx::{Docx, StoryKey, View};
use crate::error::Result;
use crate::xml::{self, ns};

/// Elements that carry a tracked change (ECMA-376 Part 1, 17.13.5).
const REVISIONS: [&str; 15] = [
    "ins",
    "del",
    "moveFrom",
    "moveTo",
    "rPrChange",
    "pPrChange",
    "sectPrChange",
    "tblPrChange",
    "tblGridChange",
    "trPrChange",
    "tcPrChange",
    "tblPrExChange",
    "numberingChange",
    "cellIns",
    "cellDel",
];

/// The author the writer puts on every revision it makes.
pub const LINEAGE_AUTHOR: &str = "Lineage";

/// What the comparison found. Counts only: never document text or names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Written {
    /// Every story reads the same with all revisions accepted.
    pub same_accepted: bool,
    /// Every story reads the same with all revisions rejected.
    pub same_rejected: bool,
    /// Revisions by Lineage in what was meant to be written.
    pub lineage_expected: usize,
    /// Revisions by Lineage in what is there now.
    pub lineage_found: usize,
    /// Revisions by any other author that the expected file did not have.
    pub other_added: usize,
}

impl Written {
    /// True when the person sees exactly what was meant, with Lineage's
    /// changes still shown as Lineage's.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.same_accepted
            && self.same_rejected
            && (self.lineage_expected == 0 || self.lineage_found > 0)
            && self.other_added == 0
    }
}

/// Compares what was meant to be written with what is there now.
///
/// # Errors
/// When either file is not a Word document the engine can read.
pub fn written(expected: &[u8], actual: &[u8]) -> Result<Written> {
    let same = |view| -> Result<bool> {
        Ok(story_lines(&Docx::open_view(expected, view)?)
            == story_lines(&Docx::open_view(actual, view)?))
    };
    let same_accepted = same(View::Accepted)?;
    let same_rejected = same(View::Rejected)?;
    let want = authors(&Docx::open(expected)?)?;
    let have = authors(&Docx::open(actual)?)?;
    let other_added = have
        .iter()
        .filter(|(author, _)| author.as_str() != LINEAGE_AUTHOR)
        .map(|(author, count)| count.saturating_sub(want.get(author).copied().unwrap_or(0)))
        .sum();
    Ok(Written {
        same_accepted,
        same_rejected,
        lineage_expected: want.get(LINEAGE_AUTHOR).copied().unwrap_or(0),
        lineage_found: have.get(LINEAGE_AUTHOR).copied().unwrap_or(0),
        other_added,
    })
}

/// Every story's lines, keyed by story. Keys, not part names: Word renames
/// header and footer parts when it saves.
fn story_lines(docx: &Docx) -> BTreeMap<StoryKey, Vec<String>> {
    docx.stories
        .iter()
        .map(|story| (story.key.clone(), story.lines()))
        .collect()
}

/// Revision counts by author, across every story part.
fn authors(docx: &Docx) -> Result<BTreeMap<String, usize>> {
    let mut counts = BTreeMap::new();
    let mut seen: Vec<&str> = Vec::new();
    for story in &docx.stories {
        if seen.contains(&story.part.as_str()) {
            continue;
        }
        seen.push(&story.part);
        let text = docx.part_text(&story.part)?;
        let doc = xml::parse(&story.part, text)?;
        for node in doc.descendants().filter(|n| {
            n.is_element()
                && n.tag_name().namespace() == Some(ns::W)
                && REVISIONS.contains(&n.tag_name().name())
        }) {
            let author = node.attribute((ns::W, "author")).unwrap_or_default();
            *counts.entry(author.to_owned()).or_insert(0) += 1;
        }
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use lineage_testkit::resume;

    use super::*;
    use crate::Package;
    use crate::merge::{self, Decision, Status};
    use crate::testutil::{open, resume_bytes};
    use crate::write::{Revision, apply};

    /// The master's new job brought into a tailored version, as tracked changes.
    fn merged() -> (Vec<u8>, Vec<u8>) {
        let base = resume_bytes(&resume::master_v1_lines());
        let theirs = resume_bytes(&resume::master_v2_lines());
        let (base_doc, ours_doc, theirs_doc) = (open(&base), open(&base), open(&theirs));
        let plan = merge::plan(&base_doc, &ours_doc, &theirs_doc);
        let decisions: Vec<(String, Decision)> = plan
            .items
            .iter()
            .filter(|i| i.status == Status::Clean)
            .map(|i| (i.change.id.clone(), Decision::BringIn))
            .collect();
        let out = apply(
            &plan,
            &decisions,
            &Revision::lineage("2026-10-15T09:30:00Z"),
        )
        .expect("writes");
        (base, out)
    }

    fn replace_in_main(bytes: &[u8], from: &str, to: &str) -> Vec<u8> {
        let mut package = Package::read(bytes).expect("reads");
        let text = String::from_utf8(package.part("word/document.xml").expect("main").to_vec())
            .expect("utf-8");
        assert!(text.contains(from), "the file has {from}");
        package.set_part("word/document.xml", text.replace(from, to).into_bytes());
        package.to_bytes().expect("bytes")
    }

    #[test]
    fn the_same_file_is_written() {
        let (_, out) = merged();
        let found = written(&out, &out).expect("compares");
        assert!(found.ok(), "{found:?}");
        assert!(found.lineage_expected > 0);
        assert_eq!(found.lineage_expected, found.lineage_found);
    }

    #[test]
    fn dropped_changes_are_caught() {
        let (original, out) = merged();
        let found = written(&out, &original).expect("compares");
        assert!(!found.same_accepted);
        assert!(!found.ok());
    }

    #[test]
    fn changes_accepted_on_the_way_in_are_caught() {
        let (_, out) = merged();
        let flattened = crate::clean::clean_copy(&out, None).expect("cleans").0;
        let found = written(&out, &flattened).expect("compares");
        assert!(found.same_accepted, "the accepted text is the same");
        assert!(
            !found.same_rejected,
            "but rejecting no longer gives the original"
        );
        assert_eq!(found.lineage_found, 0);
        assert!(!found.ok());
    }

    #[test]
    fn changes_given_to_someone_else_are_caught() {
        let (_, out) = merged();
        let renamed = replace_in_main(&out, "w:author=\"Lineage\"", "w:author=\"Someone\"");
        let found = written(&out, &renamed).expect("compares");
        assert!(found.same_accepted && found.same_rejected);
        assert_eq!(found.lineage_found, 0);
        assert!(found.other_added > 0);
        assert!(!found.ok());
    }

    #[test]
    fn a_file_with_no_changes_matches_itself() {
        let bytes = resume_bytes(&resume::master_v1_lines());
        let found = written(&bytes, &bytes).expect("compares");
        assert!(found.ok());
        assert_eq!(found.lineage_expected, 0);
    }
}
