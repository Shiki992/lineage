// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Every text colour on every background it is used on reaches 4.5:1 (WCAG 2.1
// AA for normal text), computed from tokens.css itself.

import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(path.resolve(process.cwd(), "src/styles/tokens.css"), "utf8");
const tokens = new Map(
  [...css.matchAll(/--([a-z-]+):\s*(#[0-9a-fA-F]{6})/g)].map((m) => [m[1] ?? "", m[2] ?? ""]),
);

function token(name: string): string {
  const value = tokens.get(name) ?? (name === "white" ? "#ffffff" : undefined);
  if (!value) {
    throw new Error(`no token --${name}`);
  }
  return value;
}

function luminance(hex: string): number {
  const channel = (i: number) => {
    const c = parseInt(hex.slice(i, i + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5);
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

const pairs: [string, string][] = [
  ["ink", "page"],
  ["ink", "surface"],
  ["text-secondary", "surface"],
  ["text-secondary", "page"],
  ["accent-text", "accent-tint"],
  ["accent-text", "surface"],
  ["white", "accent"],
  ["variant-text", "variant-tint"],
  ["ink", "master-tint"],
  ["milestone-text", "milestone"],
  ["warning-text", "warning-tint"],
  ["destructive", "surface"],
  ["removed-text", "removed-tint"],
  ["added-text", "added-tint"],
];

describe("design tokens", () => {
  it.each(pairs)("--%s on --%s is at least 4.5:1", (text, background) => {
    expect(contrast(token(text), token(background))).toBeGreaterThanOrEqual(4.5);
  });

  it("keep touch targets at 44 px", () => {
    expect(css).toContain("--target: 44px");
  });
});
