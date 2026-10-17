// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Keyboard only: the list, the tabs and the tree, with focus always visible.

import { expect, test } from "@playwright/test";
import type { Page } from "@playwright/test";
import { PORTS } from "../playwright.config";

const BASE = `http://localhost:${String(PORTS["three-families"])}`;

async function tabTo(page: Page, name: RegExp, limit = 30): Promise<void> {
  for (let i = 0; i < limit; i++) {
    await page.keyboard.press("Tab");
    const label = await page.evaluate(() => {
      const el = document.activeElement as HTMLElement | null;
      return `${el?.getAttribute("aria-label") ?? ""} ${el?.textContent ?? ""}`;
    });
    if (name.test(label)) {
      return;
    }
  }
  throw new Error(`Tab never reached ${String(name)}`);
}

test("the list, the tabs and the tree work with the keyboard alone", async ({ page }) => {
  await page.goto(BASE);
  await expect(page.getByText("3 documents")).toBeVisible();
  await tabTo(page, /^ WResume/);
  const outline = await page.evaluate(() => {
    const focused = document.activeElement;
    return focused ? getComputedStyle(focused).outlineStyle : "none";
  });
  expect(outline).not.toBe("none");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("heading", { name: "Resume", level: 1 })).toBeVisible();

  await tabTo(page, /Versions$/);
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("tab", { name: "Sent copies" })).toBeFocused();
  await expect(page.getByRole("tab", { name: "Sent copies" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("tab", { name: "Family tree" })).toBeFocused();

  await tabTo(page, /^Resume.docx v|^File that came back|^Copy sent/);
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("ArrowLeft");
  await page.keyboard.press(" ");
  await expect(page.getByRole("region", { name: "Details" })).toBeVisible();
  // The tree is one tab stop: Tab leaves it.
  await page.keyboard.press("Tab");
  expect(
    await page.evaluate(() => document.activeElement?.classList.contains("node") ?? false),
  ).toBe(false);
});
