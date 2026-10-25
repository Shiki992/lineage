// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import { NotFound } from "@lineage/shell";
import { describe, expect, it } from "vitest";
import { Graph } from "./graph.js";
import { base64url, itemPath, savedFile } from "./locate.js";
import { FakeGraph } from "./testing/fake-graph.js";

const text = new TextEncoder();

describe("an open document as a Graph item", () => {
  it("finds personal OneDrive by path or by id, and anything else through the shares API", () => {
    expect(itemPath("https://d.docs.live.net/a1b2c3d4e5f60718/Documents/Q3%20model.xlsx")).toBe(
      "/drives/a1b2c3d4e5f60718/root:/Documents/Q3%20model.xlsx",
    );
    expect(
      itemPath(
        "https://onedrive.live.com/edit.aspx?resid=A1B2C3D4E5F60718!101&cid=a1b2c3d4e5f60718",
      ),
    ).toBe("/drives/A1B2C3D4E5F60718/items/A1B2C3D4E5F60718!101");
    const work = "https://contoso-my.sharepoint.com/personal/pat_contoso_com/Documents/Model.xlsx";
    expect(itemPath(work)).toBe(`/shares/u!${base64url(work)}/driveItem`);
    expect(base64url(work)).not.toMatch(/[+/=]/);
  });

  it("refuses what isn't in OneDrive", () => {
    for (const address of [
      "C:\\Users\\pat\\Model.xlsx",
      "file:///Users/pat/Model.xlsx",
      "http://d.docs.live.net/a1/Model.xlsx",
      "https://d.docs.live.net/a1",
    ]) {
      expect(itemPath(address), address).toBeUndefined();
    }
  });
});

describe("the open workbook as last saved", () => {
  function setup() {
    const fake = new FakeGraph();
    const graph = new Graph({
      token: () => Promise.resolve("token"),
      fetch: fake.fetch,
      sleep: () => Promise.resolve(),
    });
    return { fake, graph };
  }

  it("reads the saved bytes with their content tag, which changes when Excel saves", async () => {
    const { fake, graph } = setup();
    fake.file(fake.drive, "Documents/Model.xlsx", text.encode("v1"));
    const saved = savedFile(graph, `https://d.docs.live.net/${fake.drive}/Documents/Model.xlsx`);
    const first = await saved.read();
    expect(new TextDecoder().decode(first.bytes)).toBe("v1");
    expect(await saved.tag()).toBe(first.tag);
    fake.file(fake.drive, "Documents/Model.xlsx", text.encode("v2"));
    const second = await saved.read();
    expect(new TextDecoder().decode(second.bytes)).toBe("v2");
    expect(second.tag).not.toBe(first.tag);
  });

  it("says plainly when the workbook isn't saved in OneDrive, or isn't there", async () => {
    const { fake, graph } = setup();
    await expect(savedFile(graph, "C:\\Book1.xlsx").read()).rejects.toMatchObject({
      userMessage: "Lineage can read this workbook on the web only once it's saved in OneDrive.",
    });
    await expect(
      savedFile(graph, `https://d.docs.live.net/${fake.drive}/Gone.xlsx`).tag(),
    ).rejects.toBeInstanceOf(NotFound);
  });
});
