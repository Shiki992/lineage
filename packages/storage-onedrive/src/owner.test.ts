// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";
import { Graph } from "./graph.js";
import { appFolder } from "./layout.js";
import { OneDriveStorage } from "./onedrive-storage.js";
import { isOwn, ownerOf } from "./owner.js";
import { FakeGraph } from "./testing/fake-graph.js";

describe("whose drive a document is in", () => {
  it("reads the owner from the addresses Office gives", () => {
    expect(ownerOf("https://d.docs.live.net/A1B2C3D4E5F60718/Documents/Resume.docx")).toEqual({
      kind: "personal",
      cid: "a1b2c3d4e5f60718",
    });
    expect(
      ownerOf(
        "https://onedrive.live.com/edit.aspx?cid=a1b2c3d4e5f60718&resid=A1B2C3D4E5F60718!101",
      ),
    ).toEqual({ kind: "personal", cid: "a1b2c3d4e5f60718" });
    expect(ownerOf("https://onedrive.live.com/edit?resid=00FF00FF00FF00FF!55")).toEqual({
      kind: "personal",
      cid: "00ff00ff00ff00ff",
    });
    expect(
      ownerOf("https://contoso-my.sharepoint.com/personal/pat_contoso_com/Documents/Deck.pptx"),
    ).toEqual({
      kind: "business",
      root: "https://contoso-my.sharepoint.com/personal/pat_contoso_com",
    });
  });

  it("can't place a team site, a local file or anything else", () => {
    for (const address of [
      "https://contoso.sharepoint.com/sites/Finance/Shared Documents/Model.xlsx",
      "C:\\Users\\pat\\Documents\\Resume.docx",
      "file:///Users/pat/Resume.docx",
      "http://d.docs.live.net/a1b2c3d4e5f60718/Resume.docx",
      "https://d.docs.live.net/not-a-cid/Resume.docx",
      "https://onedrive.live.com/",
    ]) {
      expect(ownerOf(address), address).toBeUndefined();
    }
  });

  it("compares a personal cid with the drive's id, and a work site with the folder's address", () => {
    const personal = { kind: "personal", cid: "a1b2c3d4e5f60718" } as const;
    expect(isOwn(personal, { driveId: "A1B2C3D4E5F60718" })).toBe(true);
    expect(isOwn(personal, { driveId: "ffffffffffffffff" })).toBe(false);
    expect(isOwn(personal, {})).toBeUndefined();
    const work = {
      kind: "business",
      root: "https://contoso-my.sharepoint.com/personal/pat_contoso_com",
    } as const;
    const own = "https://contoso-my.sharepoint.com/personal/pat_contoso_com/Documents/Apps/Lineage";
    expect(isOwn(work, { webUrl: own })).toBe(true);
    expect(isOwn(work, { webUrl: own.replace("pat_contoso_com", "pat_contoso_com_2") })).toBe(
      false,
    );
    expect(isOwn(work, {})).toBeUndefined();
  });

  it("OneDrive says a file in someone else's drive isn't the person's own", async () => {
    const fake = new FakeGraph();
    const graph = new Graph({
      token: () => Promise.resolve("token"),
      fetch: fake.fetch,
      sleep: () => Promise.resolve(),
    });
    const storage = new OneDriveStorage(graph, appFolder);
    expect(await storage.holds("https://d.docs.live.net/a1b2c3d4e5f60718/Resume.docx")).toBe(true);
    expect(await storage.holds("https://d.docs.live.net/0000aaaa0000aaaa/Resume.docx")).toBe(false);
    expect(await storage.holds("https://contoso.sharepoint.com/sites/x/a.docx")).toBeUndefined();
    // The app folder is asked for once.
    const asked = fake.requests.filter((r) => r.url.includes("/special/approot"));
    expect(asked).toHaveLength(1);
  });
});
