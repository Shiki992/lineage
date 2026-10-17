// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Signing in and out: there are no Lineage accounts, and signing out clears
// what this browser kept.

import { fireEvent, render, screen } from "@testing-library/react";
import type { Engine, Storage } from "@lineage/shell";
import { MemoryStorage } from "@lineage/shell";
import { describe, expect, it, vi } from "vitest";
import type { AccountProvider, AccountStatus } from "../account/account";
import { App } from "./App";
import { fakeDocuments } from "./fixtures";

function account(start: AccountStatus): AccountProvider & { now: AccountStatus } {
  const a = {
    now: start,
    name: "Microsoft",
    signInLabel: "Sign in with Microsoft",
    where: "your OneDrive",
    status: vi.fn(() => Promise.resolve(a.now)),
    signIn: vi.fn(() => {
      a.now = { kind: "signedIn", person: { name: "Avery Quinn", username: "avery@example.com" } };
      return Promise.resolve();
    }),
    signOut: vi.fn(() => {
      a.now = { kind: "signedOut" };
      return Promise.resolve();
    }),
    storage: (): Storage => new MemoryStorage(),
  };
  return a;
}

const engine = {} as Engine;

describe("My documents sign-in", () => {
  it("asks to sign in, with no Lineage account, then shows the documents", async () => {
    const who = account({ kind: "signedOut" });
    render(<App account={who} engine={engine} documents={() => fakeDocuments()} />);
    expect(await screen.findByText(/There's no Lineage account/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Sign in with Microsoft" }));
    expect(await screen.findByText("3 documents")).toBeTruthy();
    expect(screen.getByText("Avery Quinn")).toBeTruthy();
  });

  it("signs out and forgets", async () => {
    const who = account({
      kind: "signedIn",
      person: { name: "Avery Quinn", username: "avery@example.com" },
    });
    render(<App account={who} engine={engine} documents={() => fakeDocuments()} />);
    expect(await screen.findByText("3 documents")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Sign out" }));
    expect(await screen.findByRole("button", { name: "Sign in with Microsoft" })).toBeTruthy();
    expect(who.now).toEqual({ kind: "signedOut" });
    expect(screen.queryByText("3 documents")).toBeNull();
  });

  it("says why sign-in isn't working", async () => {
    const who = account({
      kind: "failed",
      message: "Sign-in isn't set up in this copy of Lineage yet.",
    });
    render(<App account={who} engine={engine} documents={() => fakeDocuments()} />);
    expect((await screen.findByRole("alert")).textContent).toBe(
      "Sign-in isn't set up in this copy of Lineage yet.",
    );
  });
});
