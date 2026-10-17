// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The Phase 3B "done when", in a real browser on the three-family account:
// see the resume and deck families, open one, compare two versions, open a
// sent copy exactly as sent, and walk the tree. Then the edge cases: a stale
// index, a moved file, restore and milestones. Read-only checks come first;
// the ones that write come last.

import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { expect, test } from "@playwright/test";
import { ACCOUNTS, PORTS } from "../playwright.config";

const BASE = `http://localhost:${String(PORTS["three-families"])}`;
const ROOT = path.join(ACCOUNTS, "three-families");
const sha = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");

test.describe.configure({ mode: "serial" });

/** Leaves the index out of date: the deck missing, the letter at an old version. */
function staleIndex(): string {
  const file = path.join(ROOT, "Lineage", "index.json");
  const index = JSON.parse(fs.readFileSync(file, "utf8")) as {
    families: { ledger_version: string; facts: { name: string } | null }[];
  };
  index.families = index.families.filter((f) => f.facts?.name !== "Pitch deck");
  for (const f of index.families) {
    if (f.facts?.name === "Cover letter") {
      f.ledger_version = "1";
    }
  }
  const text = `${JSON.stringify(index)}\n`;
  fs.writeFileSync(file, text);
  return text;
}

test("lists the families, rebuilding the out-of-date index", async ({ page }) => {
  const before = staleIndex();
  expect(before).not.toContain('"name":"Pitch deck"');
  await page.goto(BASE);
  const list = page.getByRole("list", { name: "Documents" });
  await expect(page.getByText("3 documents")).toBeVisible();
  await expect(list.getByRole("link")).toHaveText([/^WCover letter/, /^WResume/, /^PPitch deck/]);
  await expect(list.getByRole("link", { name: /Resume/ })).toContainText(
    "2 tailored versions · 9 checkpoints · 4 sent · 2 came back",
  );
  const after = fs.readFileSync(path.join(ROOT, "Lineage", "index.json"), "utf8");
  expect(after).not.toBe(before);
  expect(after).toContain('"name":"Pitch deck"');

  await page.getByLabel("Format").selectOption("pptx");
  await expect(list.getByRole("link")).toHaveCount(1);
  await page.getByLabel("Format").selectOption("all");
  await page.getByLabel("Search").fill("letter");
  await expect(list.getByRole("link")).toHaveText([/Cover letter/]);
  await page.getByLabel("Search").fill("");
  await page.getByLabel("Sort by").selectOption("name");
  await expect(list.getByRole("link")).toHaveText([/Cover letter/, /Pitch deck/, /Resume/]);
});

test("opens the resume and shows each file's versions", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("link", { name: /Resume/ }).click();
  await expect(page.getByRole("heading", { name: "Resume", level: 1 })).toBeVisible();
  const master = page.getByRole("region", { name: "Resume.docx" });
  await expect(master.locator(".timeline__entry strong")).toHaveText(["v4", "v3", "v2", "v1"]);
  await expect(master.getByText("Sent to ten companies")).toBeVisible();
  await expect(page.getByRole("region", { name: "Resume – old.docx" })).toHaveCount(0);
  await page.getByLabel(/Show archived/).check();
  await expect(page.getByRole("region", { name: "Resume – old.docx" })).toBeVisible();
});

test("compares two versions", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("link", { name: /Resume/ }).click();
  const master = page.getByRole("region", { name: "Resume.docx" });
  await master.getByRole("checkbox", { name: "Compare Resume v1" }).check();
  await master.getByRole("checkbox", { name: "Compare Resume v3" }).check();
  await page.getByRole("button", { name: "Compare Resume v1 with Resume v3" }).click();
  await expect(page.getByRole("heading", { name: /from Resume v1 to Resume v3$/ })).toBeVisible();
  await expect(page.locator(".what ins").first()).toBeVisible();
});

test("opens a sent copy exactly as sent", async ({ page }) => {
  await page.goto(`${BASE}/#/`);
  await page.getByRole("link", { name: /Resume/ }).click();
  await page.getByRole("tab", { name: "Sent copies" }).click();
  await expect(page.getByText("Each copy is kept exactly as sent.")).toBeVisible();
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Open as sent to Pellow Ventures" }).click();
  const file = await download;
  expect(file.suggestedFilename()).toBe("Resume v3 (Pellow Ventures).docx");
  const got = fs.readFileSync(await file.path());
  const sent = fs.readFileSync(path.join(ROOT, "Resume (Pellow Ventures).docx"));
  expect(sha(got)).toBe(sha(sent));
});

test("walks the family tree by keyboard", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("link", { name: /Resume/ }).click();
  await page.getByRole("tab", { name: "Family tree" }).click();
  const tree = page.getByRole("group", { name: "Family tree" });
  const focusable = tree.locator('button[tabindex="0"]');
  await expect(focusable).toHaveCount(1);
  await focusable.focus();
  await page.keyboard.press("Home");
  await expect(page.locator(":focus")).toHaveAttribute(
    "aria-label",
    /^File that came back, From Mentor/,
  );
  await page.keyboard.press("ArrowUp");
  await expect(page.locator(":focus")).toHaveAttribute("aria-label", /^Copy sent to/);
  await page.keyboard.press("Home");
  await page.keyboard.press("Enter");
  const details = page.getByRole("region", { name: "Details" });
  await expect(details).toContainText("Copy sent to Mentor");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("ArrowUp");
  await page.keyboard.press("ArrowUp");
  await expect(page.locator(":focus")).toHaveAttribute("aria-label", /^Resume.docx v/);
  await expect(details).toContainText("Resume.docx");
});

test("compares a deck's checkpoints, slide by slide", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("link", { name: /Pitch deck/ }).click();
  await expect(page.getByText(/^Comes with PowerPoint support/)).toHaveCount(0);
  const master = page.getByRole("region", { name: "Pitch deck.pptx" });
  await expect(master.locator(".timeline__entry strong")).toHaveText(["v3", "v2", "v1"]);
  await master.getByRole("checkbox", { name: "Compare Pitch deck v1" }).check();
  await master.getByRole("checkbox", { name: "Compare Pitch deck v2" }).check();
  await page.getByRole("button", { name: "Compare Pitch deck v1 with Pitch deck v2" }).click();
  await expect(
    page.getByRole("heading", { name: /from Pitch deck v1 to Pitch deck v2$/ }),
  ).toBeVisible();
  await expect(page.getByText("Slide 6 · Traction").first()).toBeVisible();
  await expect(page.locator(".what ins").first()).toBeVisible();
});

test("opens a deck sent to an investor exactly as sent", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("link", { name: /Pitch deck/ }).click();
  await page.getByRole("tab", { name: "Sent copies" }).click();
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Open as sent to Northgate Capital" }).click();
  const file = await download;
  expect(file.suggestedFilename()).toMatch(
    /^Pitch deck – Northgate Capital v\d+ \(Northgate Capital\)\.pptx$/,
  );
  const got = fs.readFileSync(await file.path());
  const sent = fs.readFileSync(
    path.join(ROOT, "Pitch deck – Northgate Capital (Northgate Capital).pptx"),
  );
  expect(sha(got)).toBe(sha(sent));
});

test("relinks a moved file to the file the person picks", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("link", { name: /Resume/ }).click();
  const missing = page.getByRole("group", { name: "Resume – platform roles.docx is not found" });
  await expect(missing).toContainText("File not found.");
  await missing.getByRole("button", { name: "Choose the file" }).click();
  await missing.getByRole("radio", { name: "Platform roles (renamed).docx" }).check();
  await missing.getByRole("button", { name: "Link this file" }).click();
  await expect(page.getByText(/is linked to “Platform roles \(renamed\).docx” now/)).toBeVisible();
  await expect(page.getByRole("group", { name: /is not found/ })).toHaveCount(0);
});

test("restores an old deck as a new tailored version", async ({ page }) => {
  const decks = (dir: string) =>
    fs
      .readdirSync(dir)
      .filter((n) => n.endsWith(".pptx"))
      .map((n) => [n, sha(fs.readFileSync(path.join(dir, n)))]);
  const before = [...decks(ROOT), ...decks(path.join(ROOT, "Tailored versions"))];
  await page.goto(BASE);
  await page.getByRole("link", { name: /Pitch deck/ }).click();
  const master = page.getByRole("region", { name: "Pitch deck.pptx" });
  await master.getByRole("button", { name: "Restore as a new version from Pitch deck v1" }).click();
  await expect(page.getByLabel("Name of the new version")).toHaveValue("Pitch deck (v1)");
  await page.getByLabel("Name of the new version").fill("Pitch deck – first deck");
  await page.getByRole("button", { name: "Save the new version" }).click();
  await expect(
    page.getByText(/Saved “Pitch deck – first deck.pptx” as a new tailored version/),
  ).toBeVisible();
  const after = [...decks(ROOT), ...decks(path.join(ROOT, "Tailored versions"))];
  for (const entry of before) {
    expect(after).toContainEqual(entry);
  }
  expect(fs.existsSync(path.join(ROOT, "Tailored versions", "Pitch deck – first deck.pptx"))).toBe(
    true,
  );
});

test("restores an old version as a new one, changing nothing else", async ({ page }) => {
  const files = (dir: string) =>
    fs
      .readdirSync(dir)
      .filter((n) => n.endsWith(".docx"))
      .map((n) => [n, sha(fs.readFileSync(path.join(dir, n)))]);
  const before = [...files(ROOT), ...files(path.join(ROOT, "Tailored versions"))];
  await page.goto(BASE);
  await page.getByRole("link", { name: /Resume/ }).click();
  const master = page.getByRole("region", { name: "Resume.docx" });
  await master.getByRole("button", { name: "Restore as a new version from Resume v1" }).click();
  await page.getByLabel("Name of the new version").fill("Resume – first draft");
  await page.getByRole("button", { name: "Save the new version" }).click();
  await expect(
    page.getByText(/Saved “Resume – first draft.docx” as a new tailored version/),
  ).toBeVisible();
  await expect(page.getByRole("region", { name: "Resume – first draft.docx" })).toContainText(
    "Restored from Resume v1",
  );
  const after = [...files(ROOT), ...files(path.join(ROOT, "Tailored versions"))];
  for (const entry of before) {
    expect(after).toContainEqual(entry);
  }
  expect(fs.existsSync(path.join(ROOT, "Tailored versions", "Resume – first draft.docx"))).toBe(
    true,
  );
});

test("marks a milestone", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("link", { name: /Cover letter/ }).click();
  const letter = page.getByRole("region", { name: "Cover letter.docx" });
  await letter.getByRole("button", { name: "Mark as milestone on Cover letter v2" }).click();
  await page.getByLabel("Milestone", { exact: true }).fill("First applications");
  await page.getByRole("button", { name: "Save milestone" }).click();
  await expect(letter.getByText("First applications")).toBeVisible();
});
