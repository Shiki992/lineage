// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Workbook families in My documents (Phase 5), in a real browser on the
// workbook account: compare two checkpoints in the five categories, open a
// sent workbook exactly as sent, restore a checkpoint as a new version, and a
// family with macros whose copies stay .xlsm. Read-only checks first.

import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import { expect, test } from "@playwright/test";
import { ACCOUNTS, PORTS } from "../playwright.config";

const BASE = `http://localhost:${String(PORTS.workbooks)}`;
const ROOT = path.join(ACCOUNTS, "workbooks");
const sha = (bytes: Buffer) => createHash("sha256").update(bytes).digest("hex");

test.describe.configure({ mode: "serial" });

test("lists the workbook families", async ({ page }) => {
  await page.goto(BASE);
  const list = page.getByRole("list", { name: "Documents" });
  await expect(page.getByText("2 documents")).toBeVisible();
  await expect(list.getByRole("link", { name: /Startup model/ })).toContainText("X");
  await page.getByLabel("Format").selectOption("xlsx");
  await expect(list.getByRole("link")).toHaveCount(2);
});

test("compares two checkpoints in the five categories", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("link", { name: /Startup model/ }).click();
  await expect(page.getByRole("heading", { name: "Startup model", level: 1 })).toBeVisible();
  const master = page.getByRole("region", { name: "Startup model.xlsx" });
  await master.getByRole("checkbox", { name: "Compare Startup model v1" }).check();
  await master.getByRole("checkbox", { name: "Compare Startup model v2" }).check();
  await page
    .getByRole("button", { name: "Compare Startup model v1 with Startup model v2" })
    .click();
  const inputs = page.getByRole("region", { name: "Inputs" });
  await expect(inputs.getByText("'$49' changed to '$55'")).toBeVisible();
  await expect(page.getByRole("region", { name: "Knock-on changes" })).toContainText(
    "This changed 83 calculated cells",
  );
});

test("opens a sent workbook exactly as sent", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("link", { name: /Startup model/ }).click();
  await page.getByRole("tab", { name: "Sent copies" }).click();
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "Open as sent to Harbor Lane Capital" }).click();
  const file = await download;
  expect(file.suggestedFilename()).toMatch(/\(Harbor Lane Capital\)\.xlsx$/);
  const got = fs.readFileSync(await file.path());
  const sent = fs.readFileSync(
    path.join(ROOT, "Startup model – Harbor Lane (Harbor Lane Capital).xlsx"),
  );
  expect(sha(got)).toBe(sha(sent));
});

test("restores a checkpoint as a new workbook", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("link", { name: /Startup model/ }).click();
  const master = page.getByRole("region", { name: "Startup model.xlsx" });
  await master
    .getByRole("button", { name: "Restore as a new version from Startup model v1" })
    .click();
  await page.getByLabel("Name of the new version").fill("Startup model – first price");
  await page.getByRole("button", { name: "Save the new version" }).click();
  await expect(
    page.getByText(/Saved “Startup model – first price.xlsx” as a new tailored version/),
  ).toBeVisible();
  expect(
    fs.existsSync(path.join(ROOT, "Tailored versions", "Startup model – first price.xlsx")),
  ).toBe(true);
});

test("keeps a family with macros as .xlsm", async ({ page }) => {
  await page.goto(BASE);
  await page.getByRole("link", { name: /Model with macros/ }).click();
  const master = page.getByRole("region", { name: "Model with macros.xlsm" });
  await master
    .getByRole("button", { name: "Restore as a new version from Model with macros v1" })
    .click();
  await page.getByLabel("Name of the new version").fill("Model with macros – again");
  await page.getByRole("button", { name: "Save the new version" }).click();
  await expect(
    page.getByText(/Saved “Model with macros – again.xlsm” as a new tailored version/),
  ).toBeVisible();
});
