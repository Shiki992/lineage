// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The privacy, terms and help pages: plain HTML in the design system, with
// this script only for the fonts, the styles and the version number.

import "@fontsource/ibm-plex-sans/latin-400.css";
import "@fontsource/ibm-plex-sans/latin-600.css";
import "@lineage/design/tokens.css";
import "./styles/app.css";
import "./styles/pages.css";
import { VERSION } from "./app/stopped";

for (const element of document.querySelectorAll("[data-version]")) {
  element.textContent = VERSION;
}
