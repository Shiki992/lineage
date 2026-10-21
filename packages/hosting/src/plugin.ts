// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The Vite plugin both apps use: the policy as a meta tag on every page, and
// in a build the Static Web Apps configuration, the release manifest (task
// pane), and the checks on the finished build, which fail it (decision 0031).

import path from "node:path";
import type { Plugin } from "vite";
import type { Site } from "./dist.ts";
import { distProblems } from "./dist.ts";
import { releaseValues, renderManifest } from "./manifest.ts";
import type { Policy } from "./policy.ts";
import { render } from "./policy.ts";
import type { Release } from "./release.ts";
import type { SiteConfig } from "./site.ts";
import { CONFIG_FILE, serialise } from "./site.ts";

export interface HostingOptions {
  readonly site: Site;
  /** For the meta tag. The site's header may add directives a meta tag can't carry. */
  readonly policy: Policy;
  readonly config: SiteConfig;
  readonly release: Release | undefined;
  /** The task pane's manifest template, written as `manifest.xml` in a release build. */
  readonly manifestTemplate?: string;
}

export function hosting(options: HostingOptions): Plugin {
  let building = false;
  let outDir = "";
  return {
    name: "lineage-hosting",
    configResolved(config) {
      building = config.command === "build";
      outDir = path.resolve(config.root, config.build.outDir);
    },
    transformIndexHtml: () => [
      {
        tag: "meta",
        attrs: { "http-equiv": "Content-Security-Policy", content: render(options.policy) },
        injectTo: "head-prepend",
      },
    ],
    generateBundle() {
      this.emitFile({ type: "asset", fileName: CONFIG_FILE, source: serialise(options.config) });
      if (options.release && options.manifestTemplate !== undefined) {
        this.emitFile({
          type: "asset",
          fileName: "manifest.xml",
          source: renderManifest(options.manifestTemplate, releaseValues(options.release)),
        });
      }
    },
    // Vite also calls this when a dev server closes; only a build is checked.
    closeBundle() {
      if (!building) {
        return;
      }
      const problems = distProblems(outDir, options.site, options.release);
      if (problems.length > 0) {
        throw new Error(`The build in ${outDir} can't ship:\n- ${problems.join("\n- ")}`);
      }
    },
  };
}
