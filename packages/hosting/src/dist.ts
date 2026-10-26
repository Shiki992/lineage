// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Checks on a finished build, run as its last step (plugin.ts): nothing meant
// only for development may ship, and a release build must carry its hosting
// configuration and, for the task pane, a manifest that passes (decision 0031).

import fs from "node:fs";
import path from "node:path";
import { manifestProblems } from "./manifest.ts";
import type { Release } from "./release.ts";
import { CONFIG_FILE } from "./site.ts";

export type Site = "taskPane" | "myDocuments";

/**
 * Text that only development code contains: the dev servers' addresses, and
 * the dev-folder storage's route and folder.
 */
const DEVELOPMENT_ONLY = ["localhost:3000", "localhost:3001", "/__lineage-dev", ".dev-storage"];

const TEXT = new Set([".html", ".js", ".mjs", ".css", ".json", ".xml", ".txt"]);

function files(dir: string): string[] {
  return fs
    .readdirSync(dir, { withFileTypes: true, recursive: true })
    .flatMap((entry) =>
      entry.isFile() ? [path.relative(dir, path.join(entry.parentPath, entry.name))] : [],
    );
}

/** What is wrong with the build in `dir`, if anything. */
export function distProblems(dir: string, site: Site, release: Release | undefined): string[] {
  const problems: string[] = [];
  const all = files(dir).map((f) => f.split(path.sep).join("/"));
  for (const file of all) {
    if (!TEXT.has(path.extname(file))) {
      continue;
    }
    const text = fs.readFileSync(path.join(dir, file), "utf8");
    for (const marker of DEVELOPMENT_ONLY) {
      if (text.includes(marker)) {
        problems.push(`${file} contains "${marker}", which only development builds may`);
      }
    }
  }
  if (!all.includes("404.html")) {
    problems.push("there is no 404.html");
  }
  if (release) {
    if (!all.includes(CONFIG_FILE)) {
      problems.push(`there is no ${CONFIG_FILE}`);
    }
    // Static Web Apps' upload refuses a folder without one.
    if (!all.includes("index.html")) {
      problems.push("there is no index.html, which Static Web Apps needs before it deploys a site");
    }
    if (site === "taskPane") {
      const manifest = path.join(dir, "manifest.xml");
      if (fs.existsSync(manifest)) {
        const found = manifestProblems(fs.readFileSync(manifest, "utf8"), release, (f) =>
          all.includes(f),
        );
        problems.push(...found.map((p) => `manifest.xml: ${p}`));
      } else {
        problems.push("there is no manifest.xml");
      }
    }
  }
  return problems;
}
