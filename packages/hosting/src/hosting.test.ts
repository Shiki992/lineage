// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import { distProblems } from "./dist.ts";
import { DEVELOPMENT, manifestProblems, releaseValues, renderManifest } from "./manifest.ts";
import { MY_DOCUMENTS_HEADER_ONLY, myDocumentsPolicy, render, taskPanePolicy } from "./policy.ts";
import type { Release } from "./release.ts";
import { releaseFrom } from "./release.ts";
import { CONFIG_FILE, CONFIG_LIMIT, myDocumentsSite, serialise, taskPaneSite } from "./site.ts";

const addin = new URL("../../../apps/office-addin/", import.meta.url);
const template = fs.readFileSync(new URL("manifest.template.xml", addin), "utf8");

const ENV = {
  LINEAGE_RELEASE: "1",
  LINEAGE_CHANNEL: "production",
  VITE_LINEAGE_VERSION: "1.4.0",
  LINEAGE_BUILD: "27",
  LINEAGE_ADDIN_ORIGIN: "https://addin.example.org",
  LINEAGE_APP_ORIGIN: "https://app.example.org",
  VITE_LINEAGE_CLIENT_ID: "00000000-1111-2222-3333-444444444444",
  VITE_LINEAGE_APP_URL: "https://app.example.org/",
};
const RELEASE: Release = (() => {
  const release = releaseFrom(ENV);
  if (!release) {
    throw new Error("ENV is a release");
  }
  return release;
})();

describe("the security policy", () => {
  it("reaches only Microsoft, and the dev socket only in development", () => {
    const pane = render(taskPanePolicy(false));
    expect(pane).toContain("connect-src 'self' https://graph.microsoft.com");
    expect(pane).toContain("https://*.microsoftpersonalcontent.com");
    expect(pane).not.toContain("localhost");
    expect(render(taskPanePolicy(true))).toContain("wss://localhost:3000");
    expect(render(myDocumentsPolicy())).not.toContain("localhost");
    expect(render(myDocumentsPolicy(3005))).toContain("ws://localhost:3005");
  });

  it("compiles wasm without eval, and runs no inline script", () => {
    for (const policy of [taskPanePolicy(false), myDocumentsPolicy()]) {
      const scripts = policy.find((d) => d.startsWith("script-src")) ?? "";
      expect(scripts).toContain("'wasm-unsafe-eval'");
      expect(scripts).not.toMatch(/'unsafe-eval'|'unsafe-inline'/);
      expect(policy).toContain("object-src 'none'");
    }
  });
});

describe("the Static Web Apps configuration", () => {
  it("sends the meta tag's policy as a header, with My documents never framed", () => {
    const pane = taskPaneSite(taskPanePolicy(false));
    expect(pane.globalHeaders["Content-Security-Policy"]).toBe(render(taskPanePolicy(false)));
    expect(pane.globalHeaders["Content-Security-Policy"]).not.toContain("frame-ancestors");
    expect(pane.globalHeaders["X-Frame-Options"]).toBeUndefined();

    const app = myDocumentsSite(myDocumentsPolicy());
    expect(app.globalHeaders["Content-Security-Policy"]).toBe(
      render([...myDocumentsPolicy(), ...MY_DOCUMENTS_HEADER_ONLY]),
    );
    expect(app.globalHeaders["X-Frame-Options"]).toBe("DENY");
  });

  it("keeps the ribbon icons cacheable, and matches them before the hashed files", () => {
    const routes = taskPaneSite(taskPanePolicy(false)).routes.map((r) => r.route);
    expect(routes.indexOf("/assets/icon-*")).toBeLessThan(routes.indexOf("/assets/*"));
    const icon = taskPaneSite(taskPanePolicy(false)).routes.find(
      (r) => r.route === "/assets/icon-*",
    );
    expect(icon?.headers["Cache-Control"]).not.toMatch(/no-cache|no-store/);
  });

  it("asks for the task pane's index page again each time, like its other pages", () => {
    const index = taskPaneSite(taskPanePolicy(false)).routes.find((r) => r.route === "/index.html");
    expect(index?.headers["Cache-Control"]).toBe("no-cache");
  });

  it("never caches the sign-in bridge, and matches it before the other pages", () => {
    const routes = myDocumentsSite(myDocumentsPolicy()).routes;
    expect(routes[0]).toEqual({
      route: "/redirect.html",
      headers: { "Cache-Control": "no-store" },
    });
    const all = JSON.stringify(myDocumentsSite(myDocumentsPolicy()));
    expect(all).not.toContain("Cross-Origin-Opener-Policy");
  });

  it("serves wasm as wasm, and fits Static Web Apps' size limit", () => {
    for (const site of [
      taskPaneSite(taskPanePolicy(false)),
      myDocumentsSite(myDocumentsPolicy()),
    ]) {
      expect(site.mimeTypes[".wasm"]).toBe("application/wasm");
      expect(Buffer.byteLength(serialise(site))).toBeLessThan(CONFIG_LIMIT / 4);
    }
  });
});

describe("a release build's settings", () => {
  it("are none outside a release", () => {
    expect(releaseFrom({})).toBeUndefined();
    expect(releaseFrom({ ...ENV, LINEAGE_RELEASE: "0" })).toBeUndefined();
  });

  it("are read whole", () => {
    expect(RELEASE).toEqual({
      channel: "production",
      version: "1.4.0",
      build: 27,
      addinOrigin: "https://addin.example.org",
      appOrigin: "https://app.example.org",
      clientId: "00000000-1111-2222-3333-444444444444",
    });
  });

  it("list everything missing or malformed at once", () => {
    const error = (() => {
      try {
        releaseFrom({ LINEAGE_RELEASE: "1", LINEAGE_ADDIN_ORIGIN: "http://addin.example.org/" });
      } catch (e) {
        return (e as Error).message;
      }
      return "";
    })();
    for (const name of [
      "LINEAGE_CHANNEL",
      "VITE_LINEAGE_VERSION",
      "LINEAGE_ADDIN_ORIGIN",
      "LINEAGE_APP_ORIGIN",
      "VITE_LINEAGE_CLIENT_ID",
    ]) {
      expect(error).toContain(name);
    }
  });

  it("refuse localhost, a path, a shared origin and a stray My documents link", () => {
    expect(() => releaseFrom({ ...ENV, LINEAGE_ADDIN_ORIGIN: "https://localhost:3000" })).toThrow();
    expect(() =>
      releaseFrom({ ...ENV, LINEAGE_APP_ORIGIN: "https://app.example.org/x" }),
    ).toThrow();
    expect(() => releaseFrom({ ...ENV, LINEAGE_ADDIN_ORIGIN: "https://app.example.org" })).toThrow(
      /origins of their own/,
    );
    expect(() => releaseFrom({ ...ENV, VITE_LINEAGE_APP_URL: "https://elsewhere.org/" })).toThrow(
      /VITE_LINEAGE_APP_URL/,
    );
  });
});

describe("the manifests", () => {
  it("development is the committed manifest.xml (run `pnpm --filter @lineage/hosting manifest`)", () => {
    const committed = fs.readFileSync(new URL("manifest.xml", addin), "utf8");
    expect(renderManifest(template, DEVELOPMENT)).toBe(committed);
  });

  it("a release names its own origins, id, version and name", () => {
    const xml = renderManifest(template, releaseValues(RELEASE));
    expect(xml).toContain("<Id>98fed49a-79e0-4bda-b5af-d1eefbbda353</Id>");
    expect(xml).toContain("<Version>1.4.0.27</Version>");
    expect(xml).toContain('<DisplayName DefaultValue="Lineage"/>');
    expect(xml).toContain(
      '<SourceLocation DefaultValue="https://addin.example.org/taskpane.html"/>',
    );
    expect(xml).toContain('<SupportUrl DefaultValue="https://app.example.org/support.html"/>');
    expect(manifestProblems(xml, RELEASE, () => true)).toEqual([]);

    const staging = releaseValues({ ...RELEASE, channel: "staging" });
    expect(staging.name).toBe("Lineage (staging)");
    expect(staging.id).not.toBe(releaseValues(RELEASE).id);
    expect(staging.id).not.toBe(DEVELOPMENT.id);
  });

  it("a release manifest that names localhost, a stray host or a missing icon doesn't pass", () => {
    const dev = renderManifest(template, DEVELOPMENT);
    const problems = manifestProblems(dev, RELEASE, () => true);
    expect(problems).toContain("it names localhost");
    expect(problems.join("\n")).toMatch(/development one/);

    const xml = renderManifest(template, releaseValues(RELEASE));
    const missing = manifestProblems(xml, RELEASE, (f) => f !== "assets/icon-80.png");
    expect(missing).toEqual(["/assets/icon-80.png isn't in the build"]);

    const stray = xml.replace("https://app.example.org/support.html", "https://elsewhere.org/");
    expect(manifestProblems(stray, RELEASE, () => true)).toEqual([
      "https://elsewhere.org/ is on a host this release doesn't serve",
    ]);
    const twoHosts = xml.replace('<Host Name="Workbook"/>', "");
    expect(manifestProblems(twoHosts, RELEASE, () => true)).toEqual([
      "it doesn't give the Workbook host",
    ]);
  });

  it("refuses a template that names an unknown value", () => {
    expect(() => renderManifest("<Id>{{nope}}</Id>", DEVELOPMENT)).toThrow(/nope/);
  });
});

describe("the checks on a finished build", () => {
  const dirs: string[] = [];
  afterEach(() => {
    for (const d of dirs.splice(0)) {
      fs.rmSync(d, { recursive: true, force: true });
    }
  });
  function build(files: Record<string, string>): string {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "lineage-dist-"));
    dirs.push(dir);
    for (const [name, text] of Object.entries(files)) {
      fs.mkdirSync(path.dirname(path.join(dir, name)), { recursive: true });
      fs.writeFileSync(path.join(dir, name), text);
    }
    return dir;
  }

  it("find development code in any text file", () => {
    const dir = build({ "404.html": "", "assets/a.js": 'fetch("/__lineage-dev/ledger")' });
    expect(distProblems(dir, "myDocuments", undefined)).toEqual([
      'assets/a.js contains "/__lineage-dev", which only development builds may',
    ]);
  });

  it("need the hosting file, an index page, and for the task pane a manifest that passes", () => {
    const release = build({ "404.html": "" });
    expect(distProblems(release, "taskPane", RELEASE)).toEqual([
      `there is no ${CONFIG_FILE}`,
      "there is no index.html, which Static Web Apps needs before it deploys a site",
      "there is no manifest.xml",
    ]);
    const xml = renderManifest(template, releaseValues(RELEASE));
    const icons = Object.fromEntries(
      [16, 32, 64, 80].map((n) => [`assets/icon-${String(n)}.png`, "png"]),
    );
    const good = build({
      "404.html": "",
      "index.html": "",
      [CONFIG_FILE]: "{}",
      "manifest.xml": xml,
      "taskpane.html": "",
      ...icons,
    });
    expect(distProblems(good, "taskPane", RELEASE)).toEqual([]);
    expect(distProblems(build({}), "taskPane", undefined)).toEqual(["there is no 404.html"]);
  });
});
