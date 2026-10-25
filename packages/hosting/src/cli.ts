// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Writes the development manifest from the template, after the template
// changes: `pnpm --filter @lineage/hosting manifest` (Node runs this file's
// TypeScript directly). Release builds write their own (plugin.ts).

import fs from "node:fs";
import { fileURLToPath } from "node:url";
import { DEVELOPMENT, renderManifest } from "./manifest.ts";

const addin = new URL("../../../apps/office-addin/", import.meta.url);
const template = fs.readFileSync(new URL("manifest.template.xml", addin), "utf8");
const out = fileURLToPath(new URL("manifest.xml", addin));
fs.writeFileSync(out, renderManifest(template, DEVELOPMENT));
console.log(`wrote ${out}`);
