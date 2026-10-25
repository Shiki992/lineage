// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The help, privacy and terms pages the Marketplace listing and the manifest
// link to (decision 0031): each loads with its heading and the shared footer,
// My documents links to them, and none scrolls sideways at phone width.

import { expect, test } from "@playwright/test";
import { PORTS } from "../playwright.config";

const BASE = `http://localhost:${String(PORTS["three-families"])}`;

const PAGES = [
  { path: "/support.html", heading: "Help" },
  { path: "/privacy.html", heading: "Privacy" },
  { path: "/terms.html", heading: "Terms of use" },
] as const;

for (const { path, heading } of PAGES) {
  test(`${heading} loads, with the footer, and fits a phone`, async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 });
    await page.goto(`${BASE}${path}`);
    await expect(page.getByRole("heading", { level: 1, name: heading })).toBeVisible();
    const nav = page.getByRole("navigation", { name: "Lineage pages" });
    await expect(nav.getByRole("link")).toHaveText(["My documents", "Help", "Privacy", "Terms"]);
    await expect(nav.locator('[aria-current="page"]')).toHaveCount(1);
    await expect(page.getByText(/^Lineage /).last()).toBeVisible();
    const sideways = await page.evaluate(
      () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
    );
    expect(sideways).toBeLessThanOrEqual(0);
  });
}

test("My documents links to help, privacy and terms", async ({ page }) => {
  await page.goto(BASE);
  const about = page.getByRole("navigation", { name: "About Lineage" });
  await expect(about.getByRole("link")).toHaveText(["Help", "Privacy", "Terms"]);
  await about.getByRole("link", { name: "Privacy" }).click();
  await expect(page.getByRole("heading", { level: 1, name: "Privacy" })).toBeVisible();
  await expect(page.getByText("Files.ReadWrite.AppFolder")).toBeVisible();
});
