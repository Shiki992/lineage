// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Same check as crates/core/tests/spec_examples.rs, with a different validator: every
// valid example passes, every invalid example fails.

import { readdirSync, readFileSync } from "node:fs";
import { Ajv2020 } from "ajv/dist/2020.js";
import { describe, expect, it } from "vitest";

const specDir = new URL("../../../spec/format/v0/", import.meta.url);

function readJson(url: URL): unknown {
  return JSON.parse(readFileSync(url, "utf8"));
}

function examples(kind: "valid" | "invalid"): string[] {
  const dir = new URL(`examples/${kind}/`, specDir);
  const files = readdirSync(dir)
    .filter((name) => name.endsWith(".json"))
    .sort();
  expect(files.length).toBeGreaterThan(0);
  return files;
}

// strictRequired is off because "move requires fromLocation" sits in an `if/then`
// whose properties are declared on the parent object, which ajv's strict mode flags.
const ajv = new Ajv2020({
  allErrors: true,
  strict: true,
  strictRequired: false,
  allowUnionTypes: true,
});
const validate = ajv.compile(readJson(new URL("change-list.schema.json", specDir)) as object);

describe("change-list schema v0", () => {
  it.each(examples("valid"))("accepts valid/%s", (name) => {
    const ok = validate(readJson(new URL(`examples/valid/${name}`, specDir)));
    expect(validate.errors ?? []).toEqual([]);
    expect(ok).toBe(true);
  });

  it.each(examples("invalid"))("rejects invalid/%s", (name) => {
    expect(validate(readJson(new URL(`examples/invalid/${name}`, specDir)))).toBe(false);
  });
});
