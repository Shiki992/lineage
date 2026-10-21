// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Stopped } from "./stopped";

function Broken(): never {
  throw new RangeError("invalid array length");
}

describe("when a page throws", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("says Lineage stopped, offers a reload, and copies the details only when asked", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(
      <Stopped>
        <Broken />
      </Stopped>,
    );
    expect(screen.getByRole("heading", { name: "Lineage stopped working" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Reload the page" })).toBeTruthy();
    expect(screen.getByText(/RangeError: invalid array length/)).toBeTruthy();
    expect(writeText).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Copy details" }));
    await waitFor(() => {
      expect(screen.getByText("Copied. Nothing was sent anywhere.")).toBeTruthy();
    });
  });
});
