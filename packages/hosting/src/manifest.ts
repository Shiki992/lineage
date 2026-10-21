// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The add-in manifests, from one template (apps/office-addin/manifest.template.xml):
// the committed development manifest, and staging and production ones written
// into a release build (decision 0031). Each has its own add-in id, so all
// three can be installed side by side.
// https://learn.microsoft.com/office/dev/add-ins/develop/add-in-manifests

import type { Channel, Release } from "./release.ts";

export interface ManifestValues {
  readonly id: string;
  /** Four numbers. Office takes a manifest as an update only when this rises. */
  readonly version: string;
  readonly name: string;
  /** Where the task pane is served. */
  readonly origin: string;
  readonly supportUrl: string;
  readonly learnMoreUrl: string;
}

const REPOSITORY = "https://github.com/Shiki992/lineage";

/** `pnpm dev` at https://localhost:3000. */
export const DEVELOPMENT: ManifestValues = {
  id: "44547a0d-b910-4208-a721-87f4b346428f",
  version: "0.0.1.0",
  name: "Lineage (development)",
  origin: "https://localhost:3000",
  supportUrl: REPOSITORY,
  learnMoreUrl: REPOSITORY,
};

const IDS: Record<Channel, string> = {
  staging: "d6553d8e-fd5f-4955-89d2-ee987653d292",
  production: "98fed49a-79e0-4bda-b5af-d1eefbbda353",
};

const NAMES: Record<Channel, string> = {
  staging: "Lineage (staging)",
  production: "Lineage",
};

export function releaseValues(release: Release): ManifestValues {
  return {
    id: IDS[release.channel],
    version: `${release.version}.${String(release.build)}`,
    name: NAMES[release.channel],
    origin: release.addinOrigin,
    supportUrl: `${release.appOrigin}/support.html`,
    learnMoreUrl: `${release.appOrigin}/support.html`,
  };
}

function escape(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

/**
 * Fills `{{id}}`, `{{version}}`, `{{name}}`, `{{origin}}`, `{{supportUrl}}`
 * and `{{learnMoreUrl}}`.
 *
 * @throws Error when the template names anything else.
 */
export function renderManifest(template: string, values: ManifestValues): string {
  const filled = template.replace(/\{\{(\w+)\}\}/g, (whole, key: string) => {
    if (!Object.hasOwn(values, key)) {
      throw new Error(`the manifest template names {{${key}}}, which isn't a value`);
    }
    return escape(values[key as keyof ManifestValues]);
  });
  return filled;
}

/** Every `DefaultValue` and `AppDomain` address in a manifest. */
function addresses(xml: string): string[] {
  const found = [...xml.matchAll(/DefaultValue="(https?:[^"]*)"/g)].map((m) => m[1] ?? "");
  found.push(...[...xml.matchAll(/<AppDomain>([^<]*)<\/AppDomain>/g)].map((m) => m[1] ?? ""));
  return found;
}

/**
 * What is wrong with a release manifest, if anything: it must name the
 * release's origins only, over HTTPS (Microsoft sign-in is the one other
 * domain), give all three hosts, a four-number version and a name that isn't
 * a development one, and point at icons the build has.
 */
export function manifestProblems(
  xml: string,
  release: Release,
  hasFile: (path: string) => boolean,
): string[] {
  const problems: string[] = [];
  if (/localhost/i.test(xml)) {
    problems.push("it names localhost");
  }
  if (/\{\{\w+\}\}/.test(xml)) {
    problems.push("a value wasn't filled in");
  }
  const allowed = new Set([
    release.addinOrigin,
    release.appOrigin,
    "https://login.microsoftonline.com",
  ]);
  for (const address of addresses(xml)) {
    let url: URL | undefined;
    try {
      url = new URL(address);
    } catch {
      problems.push(`${address} isn't an address`);
      continue;
    }
    if (url.protocol !== "https:") {
      problems.push(`${address} isn't HTTPS`);
    } else if (!allowed.has(url.origin)) {
      problems.push(`${address} is on a host this release doesn't serve`);
    } else if (url.origin === release.addinOrigin && !hasFile(url.pathname.slice(1))) {
      problems.push(`${url.pathname} isn't in the build`);
    }
  }
  const version = /<Version>([^<]*)<\/Version>/.exec(xml)?.[1] ?? "";
  if (!/^\d+\.\d+\.\d+\.\d+$/.test(version)) {
    problems.push(`its version "${version}" isn't four numbers`);
  }
  const name = /<DisplayName DefaultValue="([^"]*)"/.exec(xml)?.[1] ?? "";
  if (/development/i.test(name) || name.trim() === "") {
    problems.push(`its name "${name}" is a development one`);
  }
  for (const host of ["Document", "Presentation", "Workbook"]) {
    if (!xml.includes(`<Host Name="${host}"/>`) || !xml.includes(`<Host xsi:type="${host}">`)) {
      problems.push(`it doesn't give the ${host} host`);
    }
  }
  return problems;
}
