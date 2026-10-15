// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! What Word holds after the task pane writes a file into the open document
//! (decision 0008). The diagnostics screen in the development add-in writes each
//! file from `.dev-storage/spike` into Word, reads it back, and keeps the
//! read-back of every file that didn't land exactly in `.dev-storage/spike-actual`.
//! This prints, story by story and in both views, how each read-back differs.
//! The spike files are the engine's own outputs on synthetic fixtures, so
//! printing their text is fine; never point this at personal documents.
//!
//!   cargo test -p lineage-core --test word_readback -- --ignored --nocapture

use std::collections::BTreeMap;
use std::path::PathBuf;

use lineage_core::docx::StoryKey;
use lineage_core::{Docx, View};

fn stories(bytes: &[u8], view: View) -> BTreeMap<StoryKey, Vec<String>> {
    Docx::open_view(bytes, view)
        .expect("opens")
        .stories
        .iter()
        .map(|s| (s.key.clone(), s.lines()))
        .collect()
}

#[test]
#[ignore = "reads files the development add-in saved from Word; run by hand"]
fn how_word_read_backs_differ() {
    let root =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/office-addin/.dev-storage");
    let actual_dir = root.join("spike-actual");
    let mut names: Vec<_> = std::fs::read_dir(&actual_dir)
        .expect("spike-actual")
        .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    for name in names {
        let expected = std::fs::read(root.join("spike").join(&name)).expect("expected");
        let actual = std::fs::read(actual_dir.join(&name)).expect("actual");
        println!("== {name}");
        for view in [View::Accepted, View::Rejected] {
            let (want, have) = (stories(&expected, view), stories(&actual, view));
            for key in want
                .keys()
                .chain(have.keys().filter(|k| !want.contains_key(*k)))
            {
                match (want.get(key), have.get(key)) {
                    (Some(w), Some(h)) if w == h => {}
                    (Some(w), Some(h)) => {
                        let first = w.iter().zip(h).position(|(a, b)| a != b);
                        println!(
                            "  {view:?} {key:?}: {} lines expected, {} found; first difference at {first:?}",
                            w.len(),
                            h.len()
                        );
                        if let Some(i) = first {
                            println!("    expected {:?}", w.get(i));
                            println!("    found    {:?}", h.get(i));
                        } else {
                            println!(
                                "    extra: {:?}",
                                if h.len() > w.len() {
                                    &h[w.len()..]
                                } else {
                                    &w[h.len()..]
                                }
                            );
                        }
                    }
                    (Some(_), None) => println!("  {view:?} {key:?}: missing in Word"),
                    (None, Some(h)) => println!(
                        "  {view:?} {key:?}: only in Word ({} lines) {:?}",
                        h.len(),
                        h.iter().take(3).collect::<Vec<_>>()
                    ),
                    (None, None) => {}
                }
            }
        }
    }
}
