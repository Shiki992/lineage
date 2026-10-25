// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

import { describe, expect, it } from "vitest";
import { sitePageUrl } from "./my-documents";

describe("Privacy and Terms", () => {
  it("are at the root of My documents' site, as its own footer links them", () => {
    expect(sitePageUrl("privacy", "https://app.example.org/")).toBe(
      "https://app.example.org/privacy.html",
    );
    expect(sitePageUrl("terms", "http://localhost:3001/")).toBe("http://localhost:3001/terms.html");
  });
});
