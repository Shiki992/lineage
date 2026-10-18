// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Repository tasks. Run with `cargo xtask <task>`:
//!
//! - `fixtures`: write the synthetic fixtures to `fixtures/docx/synthetic/`,
//!   `fixtures/pptx/synthetic/` and `fixtures/xlsx/synthetic/`.
//! - `check-fixtures`: fail if the committed fixtures differ from what the generator
//!   produces now, or if the folder holds files the generator doesn't make.
//! - `check-headers`: fail if any source file lacks the licence header.
//! - `returned-inputs <folder>`: write the files `scripts/libreoffice-resave.ps1`
//!   re-saves with LibreOffice into `fixtures/docx/returned/libreoffice/`.

#![forbid(unsafe_code)]

mod headers;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lineage_testkit::{decks, fixtures, returned, workbooks};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level below the repository root")
        .to_path_buf()
}

/// A fixture folder's files: names and bytes.
type Files = Vec<(&'static str, Vec<u8>)>;

/// Every generated fixture folder, with its files.
fn generated() -> Vec<(&'static str, Files)> {
    vec![
        (
            fixtures::OUT_DIR,
            fixtures::all()
                .into_iter()
                .map(|(name, package)| (name, package.to_bytes()))
                .collect(),
        ),
        (
            decks::OUT_DIR,
            decks::all()
                .into_iter()
                .map(|(name, deck)| (name, deck.to_bytes()))
                .collect(),
        ),
        (
            workbooks::OUT_DIR,
            workbooks::all()
                .into_iter()
                .map(|(name, workbook)| (name, workbook.to_bytes()))
                .collect(),
        ),
    ]
}

fn write_fixtures(root: &Path) -> std::io::Result<()> {
    for (out_dir, files) in generated() {
        let dir = root.join(out_dir);
        fs::create_dir_all(&dir)?;
        for (name, bytes) in files {
            fs::write(dir.join(name), bytes)?;
            println!("wrote {out_dir}/{name}");
        }
    }
    Ok(())
}

fn check_fixtures(root: &Path) -> std::io::Result<Vec<String>> {
    let mut problems = Vec::new();
    for (out_dir, files) in generated() {
        let dir = root.join(out_dir);
        for (name, bytes) in &files {
            match fs::read(dir.join(name)) {
                Ok(on_disk) if on_disk == *bytes => {}
                Ok(_) => problems.push(format!(
                    "{out_dir}/{name} differs from the generator output"
                )),
                Err(_) => problems.push(format!("{out_dir}/{name} is missing")),
            }
        }
        if dir.is_dir() {
            for entry in fs::read_dir(&dir)? {
                let file_name = entry?.file_name();
                let name = file_name.to_string_lossy();
                if !files.iter().any(|(known, _)| *known == name) {
                    problems.push(format!("{out_dir}/{name} is not made by the generator"));
                }
            }
        }
    }
    Ok(problems)
}

fn write_returned_inputs(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    for input in &returned::LIBREOFFICE {
        fs::write(dir.join(input.name), input.package().to_bytes())?;
        println!("wrote {}", dir.join(input.name).display());
    }
    Ok(())
}

/// Writes the "My documents" test accounts (invented content only) into
/// `<dir>/three-families`, `<dir>/damaged` and `<dir>/large`, in the dev
/// folder's layout, for the family app's Playwright runs and by-hand checks.
fn write_accounts(dir: &Path) -> Result<(), String> {
    use lineage_harness::accounts::{self, START};
    use lineage_harness::{LocalFolderStorage, Panel};
    let fresh = |name: &str| -> Result<Panel<LocalFolderStorage>, String> {
        let at = dir.join(name);
        if at.exists() {
            fs::remove_dir_all(&at).map_err(|e| e.to_string())?;
        }
        let storage = LocalFolderStorage::new(&at).map_err(|e| e.to_string())?;
        Ok(Panel::new(storage, START))
    };
    let mut panel = fresh("three-families")?;
    accounts::three_families(&mut panel).map_err(|e| e.to_string())?;
    let mut panel = fresh("damaged")?;
    accounts::damaged(&mut panel).map_err(|e| e.to_string())?;
    let mut panel = fresh("large")?;
    accounts::large(&mut panel, 300, 50).map_err(|e| e.to_string())?;
    for name in ["three-families", "damaged", "large"] {
        println!("wrote {}", dir.join(name).display());
    }
    Ok(())
}

fn report(problems: &[String], fix: &str) -> ExitCode {
    if problems.is_empty() {
        return ExitCode::SUCCESS;
    }
    for problem in problems {
        eprintln!("  {problem}");
    }
    eprintln!("{fix}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let root = repo_root();
    let task = std::env::args().nth(1);
    let result = match task.as_deref() {
        Some("fixtures") => write_fixtures(&root).map(|()| ExitCode::SUCCESS),
        Some("check-fixtures") => check_fixtures(&root).map(|problems| {
            report(
                &problems,
                "Run `cargo xtask fixtures` and commit the result.",
            )
        }),
        Some("check-headers") => headers::missing(&root).map(|files| {
            let problems: Vec<String> = files.iter().map(|f| f.display().to_string()).collect();
            report(
                &problems,
                &format!(
                    "Add this header to the files above:\n{}\n{}",
                    headers::HEADER[0],
                    headers::HEADER[1]
                ),
            )
        }),
        Some("returned-inputs") => match std::env::args().nth(2) {
            Some(dir) => write_returned_inputs(Path::new(&dir)).map(|()| ExitCode::SUCCESS),
            None => {
                eprintln!("usage: cargo xtask returned-inputs <folder>");
                Ok(ExitCode::FAILURE)
            }
        },
        Some("accounts") => match std::env::args().nth(2) {
            Some(dir) => Ok(match write_accounts(Path::new(&dir)) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }),
            None => {
                eprintln!("usage: cargo xtask accounts <folder>");
                Ok(ExitCode::FAILURE)
            }
        },
        _ => {
            eprintln!(
                "usage: cargo xtask <fixtures | check-fixtures | check-headers | returned-inputs <folder> | accounts <folder>>"
            );
            Ok(ExitCode::FAILURE)
        }
    };
    result.unwrap_or_else(|error| {
        eprintln!("error: {error}");
        ExitCode::FAILURE
    })
}
