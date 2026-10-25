// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Stopped } from "./stopped";

function Broken(): never {
  throw new TypeError("cannot read properties of undefined");
}

describe("when a screen throws", () => {
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
    expect(screen.getByRole("button", { name: "Reload Lineage" })).toBeTruthy();
    expect(screen.getByText(/TypeError: cannot read properties of undefined/)).toBeTruthy();
    expect(screen.getAllByText(/Lineage development/).length).toBeGreaterThan(0);
    expect(writeText).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Copy details" }));
    await waitFor(() => {
      expect(screen.getByText("Copied. Nothing was sent anywhere.")).toBeTruthy();
    });
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining("Lineage development"));
  });

  it("says how to copy by hand when the clipboard is blocked", async () => {
    vi.spyOn(console, "error").mockImplementation(() => undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: () => Promise.reject(new Error("blocked")) },
      configurable: true,
    });
    render(
      <Stopped>
        <Broken />
      </Stopped>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Copy details" }));
    await waitFor(() => {
      expect(screen.getByText(/Select the details above and copy them/)).toBeTruthy();
    });
  });

  it("draws its children when nothing throws", () => {
    render(
      <Stopped>
        <p>The panel</p>
      </Stopped>,
    );
    expect(screen.getByText("The panel")).toBeTruthy();
  });
});
