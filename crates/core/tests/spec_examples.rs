// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

//! Checks the v0 change-list schema against its examples: every file in
//! `examples/valid/` must pass and every file in `examples/invalid/` must fail.
//! `packages/shell/src/spec.test.ts` runs the same examples through a different
//! validator, so both languages agree on what the format means.

use std::fs;
use std::path::{Path, PathBuf};

use boon::{Compiler, SchemaIndex, Schemas};
use serde_json::Value;

const SCHEMA_ID: &str = "https://github.com/Shiki992/lineage/spec/format/v0/change-list.schema.json";

fn spec_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/format/v0")
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn compile_schema() -> (Schemas, SchemaIndex) {
    let schema = read_json(&spec_dir().join("change-list.schema.json"));
    assert_eq!(schema["$id"], SCHEMA_ID, "schema $id changed; update SCHEMA_ID");
    let mut schemas = Schemas::new();
    let mut compiler = Compiler::new();
    compiler
        .add_resource(SCHEMA_ID, schema)
        .expect("schema resource is accepted");
    let index = compiler
        .compile(SCHEMA_ID, &mut schemas)
        .expect("schema compiles");
    (schemas, index)
}

fn examples(kind: &str) -> Vec<PathBuf> {
    let dir = spec_dir().join("examples").join(kind);
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();
    assert!(!paths.is_empty(), "no examples in {}", dir.display());
    paths
}

#[test]
fn valid_examples_pass() {
    let (schemas, index) = compile_schema();
    for path in examples("valid") {
        let instance = read_json(&path);
        if let Err(error) = schemas.validate(&instance, index) {
            panic!("{} should be valid:\n{error:#}", path.display());
        }
    }
}

#[test]
fn invalid_examples_fail() {
    let (schemas, index) = compile_schema();
    for path in examples("invalid") {
        let instance = read_json(&path);
        assert!(
            schemas.validate(&instance, index).is_err(),
            "{} should be invalid",
            path.display()
        );
    }
}
