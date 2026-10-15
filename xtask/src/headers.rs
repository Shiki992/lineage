// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Checks that every source file starts with the project's licence header.

use std::fs;
use std::path::{Path, PathBuf};

/// The two lines every Rust, TypeScript and JavaScript source file starts with.
pub const HEADER: [&str; 2] = [
    "// Copyright 2026 Hrishikesh Vedantam",
    "// SPDX-License-Identifier: Apache-2.0",
];

const EXTENSIONS: [&str; 5] = ["rs", "ts", "js", "mjs", "cjs"];

/// Directories never scanned: build output, dependencies, generated code, and the
/// maintainer's local test documents.
const SKIP_DIRS: [&str; 6] = [".git", "target", "node_modules", "wasm-pkg", "test", "dist"];

/// True when `text` starts with the header, after an optional `#!` line.
pub fn has_header(text: &str) -> bool {
    let mut lines = text.lines().peekable();
    if lines.peek().is_some_and(|line| line.starts_with("#!")) {
        lines.next();
    }
    HEADER
        .iter()
        .all(|expected| lines.next().map(str::trim_end) == Some(*expected))
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(fs::DirEntry::path);
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        if entry.file_type()?.is_dir() {
            if !SKIP_DIRS.iter().any(|skip| name == *skip) {
                walk(&path, out)?;
            }
        } else if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| EXTENSIONS.contains(&ext))
        {
            out.push(path);
        }
    }
    Ok(())
}

/// Source files under `root` that lack the header.
pub fn missing(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    walk(root, &mut files)?;
    let mut bad = Vec::new();
    for file in files {
        if !has_header(&fs::read_to_string(&file)?) {
            bad.push(file.strip_prefix(root).unwrap_or(&file).to_path_buf());
        }
    }
    Ok(bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_header() {
        assert!(has_header(&format!(
            "{}\n{}\n\nfn main() {{}}\n",
            HEADER[0], HEADER[1]
        )));
        assert!(has_header(&format!("{}\r\n{}\r\n", HEADER[0], HEADER[1])));
        assert!(has_header(&format!(
            "#!/usr/bin/env node\n{}\n{}\n",
            HEADER[0], HEADER[1]
        )));
    }

    #[test]
    fn rejects_missing_or_partial_header() {
        assert!(!has_header("fn main() {}\n"));
        assert!(!has_header(&format!("{}\n", HEADER[0])));
        assert!(!has_header(&format!(
            "// Copyright 2025 Someone Else\n{}\n",
            HEADER[1]
        )));
    }

    #[test]
    fn repository_sources_all_have_headers() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let bad = missing(&root).expect("scan repository");
        assert!(bad.is_empty(), "missing licence header: {bad:?}");
    }
}
