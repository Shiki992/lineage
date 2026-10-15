# 0011: Dependencies added in Phase 2

Date: 2026-10-15. Status: accepted.

## Context

CLAUDE.md allows React for the task panes and asks for a decision note for every other dependency. Phase 2 adds no Rust crates: `crates/wasm` now uses `lineage-core`, `serde` and `serde_json`, which were already in the workspace (decision 0006). Every TypeScript version below is pinned exactly in the committed lockfile, and was released at least a week before it was added (checked on npm, 2026-10-15).

## Decision

### Shipped in the task pane

| Package                                                  | Version | Licence | Why                                                                                                                                                                                                                                                    |
| -------------------------------------------------------- | ------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `react`, `react-dom`                                     | 19.3.0  | MIT     | Allowed by CLAUDE.md for task panes. Brings `scheduler` 0.28.0 (MIT).                                                                                                                                                                                  |
| `valibot` (in `@lineage/shell`)                          | 1.5.0   | MIT     | Checks every value crossing the wasm boundary (decision 0010). No dependencies, tree-shaken, and no `eval`, so it works under the pane's Content Security Policy.                                                                                      |
| `@azure/msal-browser`                                    | 5.25.0  | MIT     | Sign-in through nested app authentication (decision 0008), the library Microsoft documents for it. Brings `@azure/msal-common` 16.14.2 (MIT). About 80 KB of the 154 KB gzipped JavaScript.                                                            |
| `@fontsource/ibm-plex-sans`, `@fontsource/ibm-plex-mono` | 5.3.0   | OFL-1.1 | IBM Plex, the design system's typefaces, served from the add-in itself rather than a font CDN, so opening the pane sends nothing to a third party. Only the Latin subsets in weights 400 and 600 are bundled. OFL allows bundling fonts with software. |

### Development and tests only

| Package                                          | Version        | Licence | Why                                                                                           |
| ------------------------------------------------ | -------------- | ------- | --------------------------------------------------------------------------------------------- |
| `@types/office-js`                               | 1.0.611        | MIT     | Types for Office.js, which itself must load from Microsoft's CDN.                             |
| `@types/react`, `@types/react-dom`               | 19.3.0         | MIT     | Types.                                                                                        |
| `@testing-library/react`, `@testing-library/dom` | 16.3.3, 10.4.2 | MIT     | Component tests that find elements by role and accessible name, the way a screen reader does. |
| `jsdom`                                          | 30.1.2         | MIT     | The DOM for those tests in Vitest.                                                            |
| `eslint-plugin-react-hooks`                      | 7.1.1          | MIT     | The rules of hooks for the task pane (it found a state update inside an effect).              |

The development tree's transitive licences are MIT, MIT-0, ISC, BSD-2-Clause, BSD-3-Clause, Apache-2.0, BlueOak-1.0.0 and CC0-1.0, all permissive.

### Considered and not added

| Package                                            | Why not                                                                                                                                                                                                                                                                     |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `office-addin-dev-certs`, `office-addin-debugging` | Microsoft's tools for local certificates and sideloading. They bring usage-data reporting, `mkcert`, `keytar` (a native credential-store module) and the Microsoft 365 Agents Toolkit CLI. `scripts/dev-cert.ps1` and `scripts/sideload-word.ps1` do the two things needed. |
| `binaryen` (`wasm-opt`)                            | Measured: 9.4% smaller raw, but 7% larger gzipped (decision 0010).                                                                                                                                                                                                          |
| `ajv` at runtime                                   | Compiles validators with `new Function`, blocked by the Content Security Policy. It stays a test dependency for the change-list schema.                                                                                                                                     |
| `@vitejs/plugin-react`                             | Vite compiles JSX on its own; the plugin's fast refresh isn't needed.                                                                                                                                                                                                       |
| `zod`                                              | Larger than valibot for the same checks.                                                                                                                                                                                                                                    |
| `axe-core`                                         | MPL-2.0, and the role-and-name queries plus the contrast test on the design tokens cover what Phase 2 needs.                                                                                                                                                                |

## Consequences

- The production bundle has eight packages (react, react-dom, scheduler, valibot, the two MSAL packages and the two fonts), all MIT except the OFL fonts.
- New React or MSAL versions are adopted deliberately, at least a week after release, with the lockfile.
