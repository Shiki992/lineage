// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// A history cut off part-way through a line, and a family with 300
// checkpoints and 50 copies sent.

import { expect, test } from "@playwright/test";
import { PORTS } from "../playwright.config";

test("a damaged history is shown from what can be read, and nothing is offered", async ({
  page,
}) => {
  await page.goto(`http://localhost:${String(PORTS.damaged)}`);
  const row = page.getByRole("link", { name: /Cover letter/ });
  await expect(row).toContainText(/later entry is damaged/);
  await row.click();
  await expect(page.getByText(/^Lineage could read this history up to/)).toBeVisible();
  await expect(page.getByRole("checkbox", { name: /Compare/ })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Restore/ })).toHaveCount(0);
});

test("300 checkpoints and 50 copies stay quick", async ({ page }) => {
  await page.goto(`http://localhost:${String(PORTS.large)}`);
  const started = Date.now();
  await page.getByRole("link", { name: /Resume/ }).click();
  await expect(page.locator(".timeline__entry")).toHaveCount(300);
  const versions = Date.now() - started;
  const tree = Date.now();
  await page.getByRole("tab", { name: "Family tree" }).click();
  await expect(page.getByRole("group", { name: "Family tree" }).getByRole("button")).toHaveCount(
    350,
  );
  const drawn = Date.now() - tree;
  await page.getByRole("tab", { name: "Sent copies" }).click();
  await expect(page.getByRole("button", { name: /^Open as sent to Recruiter/ })).toHaveCount(50);
  console.log(`large family: versions ${String(versions)} ms, tree ${String(drawn)} ms`);
  expect(versions).toBeLessThan(3000);
  expect(drawn).toBeLessThan(2000);
});
