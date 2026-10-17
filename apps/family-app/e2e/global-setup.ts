// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Fresh test accounts for every run: the tests restore, mark and relink, and
// each run starts from the same invented history.

import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));

export default function globalSetup(): void {
  execFileSync(
    "cargo",
    ["run", "-q", "-p", "xtask", "--", "accounts", path.join(here, ".accounts")],
    {
      cwd: path.join(here, "..", "..", ".."),
      stdio: "inherit",
    },
  );
}
