// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Local-only fidelity run over real documents (CLAUDE.md "Fidelity checks").
//!
//! The maintainer's own documents live in the git-ignored `test/` folder and
//! hold real personal data, so this test:
//!
//! - runs only when asked: `LINEAGE_LOCAL_DOCS=test cargo test -p lineage-core
//!   --test local_fidelity -- --ignored --nocapture`;
//! - prints counts and error categories only, with files numbered rather than
//!   named, and never prints any text from them;
//! - writes outputs only if `LINEAGE_LOCAL_OUT` is set (to a folder outside the
//!   repository's tracked files, such as `target/local-fidelity`), named by
//!   number, for `scripts/word-smoke.ps1` to open.
//!
//! Checks per file: it reads; writing it unchanged keeps every part (and every
//! compressed entry) identical; both views open; it has no changes against
//! itself; a clean copy opens with the same text and no revisions. Between
//! neighbouring files (tailored versions of one resume, in practice), bringing
//! every change from one into the other must reproduce it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lineage_core::clean::clean_copy;
use lineage_core::diff::diff;
use lineage_core::merge::{self, Decision, Status};
use lineage_core::write::{Revision, apply};
use lineage_core::{Docx, Error, Package, View};

fn category(error: &Error) -> &'static str {
    match error {
        Error::NotAZip(_) => "not a zip",
        Error::Damaged(_) => "damaged",
        Error::Encrypted => "encrypted",
        Error::TooLarge(_) => "too large",
        Error::Unsupported(_) => "unsupported",
        Error::NotAWordDocument(_) => "not a Word document",
        Error::BadXml { .. } => "bad XML",
        Error::CannotEdit(_) => "cannot edit",
    }
}

fn docx_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            docx_files(&path, out);
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("docx"))
            && !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("~$"))
        {
            out.push(path);
        }
    }
}

#[derive(Default)]
struct Tally {
    counts: BTreeMap<&'static str, usize>,
    failures: Vec<String>,
}

impl Tally {
    fn ok(&mut self, check: &'static str) {
        *self.counts.entry(check).or_default() += 1;
    }

    fn fail(&mut self, file: usize, check: &'static str, why: &str) {
        self.failures
            .push(format!("file {file:03}: {check}: {why}"));
    }
}

fn revisions_in(docx: &Docx) -> bool {
    docx.stories.iter().any(|s| {
        s.blocks.iter().any(|b| match b {
            lineage_core::docx::Block::Paragraph(p) => p.has_revisions,
            lineage_core::docx::Block::Table(t) => t.has_revisions,
            _ => false,
        })
    })
}

#[test]
#[ignore = "local only: set LINEAGE_LOCAL_DOCS to the maintainer's test/ folder"]
fn local_documents_round_trip() {
    let Some(root) = std::env::var_os("LINEAGE_LOCAL_DOCS") else {
        eprintln!("LINEAGE_LOCAL_DOCS is not set; nothing to do");
        return;
    };
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(root);
    let out_dir = std::env::var_os("LINEAGE_LOCAL_OUT").map(PathBuf::from);
    if let Some(dir) = &out_dir {
        std::fs::create_dir_all(dir).expect("output folder");
    }
    let mut files = Vec::new();
    docx_files(&root, &mut files);
    let mut tally = Tally::default();
    let mut opened: Vec<(usize, Docx)> = Vec::new();

    for (n, path) in files.iter().enumerate() {
        let n = n + 1;
        let Ok(bytes) = std::fs::read(path) else {
            tally.fail(n, "read", "unreadable");
            continue;
        };
        let package = match Package::read(&bytes) {
            Ok(p) => p,
            Err(e) => {
                tally.fail(n, "package", category(&e));
                continue;
            }
        };
        tally.ok("package reads");
        match package.to_bytes().and_then(|b| Package::read(&b)) {
            Ok(again) => {
                let same_parts = package.parts() == again.parts();
                let same_entries = package
                    .part_names()
                    .all(|name| package.original_entry(name) == again.original_entry(name));
                if same_parts && same_entries {
                    tally.ok("unchanged round trip: every part and entry identical");
                } else {
                    tally.fail(n, "round trip", "a part changed");
                }
            }
            Err(e) => tally.fail(n, "round trip", category(&e)),
        }
        let accepted = match Docx::open_view(&bytes, View::Accepted) {
            Ok(d) => d,
            Err(e) => {
                tally.fail(n, "model", category(&e));
                continue;
            }
        };
        tally.ok("model opens (accepted view)");
        match Docx::open_view(&bytes, View::Rejected) {
            Ok(_) => tally.ok("model opens (rejected view)"),
            Err(e) => tally.fail(n, "rejected view", category(&e)),
        }
        if diff(&accepted, &accepted).is_empty() {
            tally.ok("no changes against itself");
        } else {
            tally.fail(n, "diff", "changes against itself");
        }
        if merge::plan(&accepted, &accepted, &accepted)
            .items
            .is_empty()
        {
            tally.ok("merge with itself is empty");
        } else {
            tally.fail(n, "merge", "items against itself");
        }
        // Start tracking writes the two ids into the custom properties. Added
        // in Phase 2, after it failed on files with an empty <Properties/>.
        let ids = lineage_core::docx::props::LineageIds {
            family: "0".repeat(32),
            checkpoint: "1".repeat(32),
        };
        let mut with_ids = package.clone();
        match lineage_core::docx::props::write_ids(&mut with_ids, &ids)
            .and_then(|()| with_ids.to_bytes())
            .and_then(|b| {
                Docx::open(&b)?;
                lineage_core::docx::props::read_ids(&Package::read(&b)?)
            }) {
            Ok(Some(read)) if read == ids => tally.ok("ids write, reread and the file still opens"),
            Ok(_) => tally.fail(n, "ids", "read back differently"),
            Err(e) => tally.fail(n, "ids", category(&e)),
        }
        match clean_copy(&bytes, None) {
            Ok((clean, report)) => match Docx::open(&clean) {
                Ok(copy) => {
                    let same_text =
                        report.hidden_runs > 0 || copy.body_lines() == accepted.body_lines();
                    if same_text && !revisions_in(&copy) {
                        tally.ok("clean copy opens, same text, no revisions");
                    } else {
                        tally.fail(n, "clean copy", "text or revisions differ");
                    }
                    if let Some(dir) = &out_dir {
                        let _ = std::fs::write(dir.join(format!("clean-{n:03}.docx")), &clean);
                    }
                }
                Err(e) => tally.fail(n, "clean copy reopens", category(&e)),
            },
            Err(e) => tally.fail(n, "clean copy", category(&e)),
        }
        opened.push((n, accepted));
    }

    // Neighbouring files: bring everything from one into the other.
    let mut by_hand = 0;
    for pair in opened.windows(2) {
        let ((na, a), (nb, b)) = (&pair[0], &pair[1]);
        let plan = merge::plan(a, a, b);
        by_hand += plan
            .items
            .iter()
            .filter(|i| matches!(i.status, Status::ByHand(_)))
            .count();
        let decisions: Vec<(String, Decision)> = plan
            .items
            .iter()
            .filter(|i| i.status == Status::Clean)
            .map(|i| (i.change.id.clone(), Decision::BringIn))
            .collect();
        let all_clean = plan.items.iter().all(|i| i.status == Status::Clean);
        match apply(
            &plan,
            &decisions,
            &Revision::lineage("2026-10-15T09:30:00Z"),
        ) {
            Ok(out) => {
                match (
                    Docx::open_view(&out, View::Accepted),
                    Docx::open_view(&out, View::Rejected),
                ) {
                    (Ok(acc), Ok(rej)) => {
                        let rejected_ok = rej.body_lines() == a.body_lines();
                        let accepted_ok = !all_clean || acc.body_lines() == b.body_lines();
                        if rejected_ok && accepted_ok {
                            tally.ok("bring everything in: accepted = target, rejected = original");
                        } else {
                            tally.fail(*na, "bring in", &format!("from file {nb:03}: accepted {accepted_ok}, rejected {rejected_ok}"));
                        }
                        if let Some(dir) = &out_dir {
                            // For `word-smoke.ps1 -Compare -Private`: the text Word
                            // should show after accepting and after rejecting. Local
                            // only, in a git-ignored folder; delete it after the run.
                            let name = format!("merged-{na:03}-{nb:03}");
                            let _ = std::fs::write(dir.join(format!("{name}.docx")), &out);
                            for (view, suffix) in [(&acc, "accepted"), (&rej, "rejected")] {
                                let lines: Vec<String> = view
                                    .body_lines()
                                    .into_iter()
                                    .map(|l| l.replace('\n', "\u{b}"))
                                    .collect();
                                let _ = std::fs::write(
                                    dir.join(format!("{name}.{suffix}.txt")),
                                    lines.join("\n"),
                                );
                            }
                        }
                    }
                    _ => tally.fail(*na, "bring in", "output does not reopen"),
                }
            }
            Err(e) => tally.fail(*na, "bring in", category(&e)),
        }
    }

    println!("local fidelity run: {} files", files.len());
    for (check, count) in &tally.counts {
        println!("  {count:>4}  {check}");
    }
    println!("  {by_hand:>4}  changes between neighbours left to bring in by hand");
    println!("  {:>4}  failures", tally.failures.len());
    for failure in &tally.failures {
        println!("        {failure}");
    }
    assert!(
        tally.failures.is_empty(),
        "{} failures",
        tally.failures.len()
    );
}
