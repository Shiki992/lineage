# 0016: Dependencies added in Phase 3B

Date: 2026-10-16. Status: accepted.

## Context

CLAUDE.md allows React for the family app and asks for a decision note for every other dependency. Every version below is pinned exactly in the committed lockfile, and was released at least a week before it was added (checked on npm, 2026-10-16).

## Decision

### Shipped in My documents (`apps/family-app`)

The same packages as the task pane (decision 0011), at the same versions:

- `react`, `react-dom` 19.3.0;
- `@azure/msal-browser` 5.25.0, for the standard client and its redirect bridge;
- `valibot` 1.5.0, through `@lineage/shell`;
- the IBM Plex fonts 5.3.0.

Nothing new ships.

### Development and tests only

| Package                     | Version | Licence    | Why                                                                                                                                                                                                                                                                                                                   |
| --------------------------- | ------- | ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `@playwright/test`          | 1.64.0  | Apache-2.0 | The end-to-end runs the Phase 3B prompt asks for. It drives a browser that is already installed, so no browser is downloaded: Microsoft Edge here (`channel: "msedge"`), Google Chrome on the CI runner (`channel: "chrome"`). Brings `playwright` and `playwright-core` 1.64.0 (Apache-2.0). 1.64.1 is two days old. |
| `valibot` (family-app, dev) | 1.5.0   | MIT        | Test fixtures parse the golden replies with the shell's schemas.                                                                                                                                                                                                                                                      |

The testing libraries (`@testing-library/react` and `@testing-library/dom`, `jsdom`, `@types/react`) are the versions decision 0011 added.

### Rust

- `proptest` becomes a dev-dependency of `crates/lineage`, for the index property test (decision 0006 covers the crate).
- `lineage-harness` now depends on `lineage-testkit` as a normal dependency, for the accounts it builds. `xtask` depends on `lineage-harness`, for `cargo xtask accounts`. All three are workspace crates that are never shipped.

### New workspace packages (ours)

- `@lineage/storage-local`: the dev-folder storage, moved out of the add-in so both apps share it.
- `@lineage/design`: the design tokens and their contrast test.

### Considered and not added

| Package                                                  | Why not                                                                                                                   |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| `@azure/msal-react`                                      | One provider behind `AccountProvider` needs no React wrapper.                                                             |
| A router (`react-router`, `wouter`)                      | Two pages; hash routes are 40 lines (`src/app/route.ts`) and work on any static host.                                     |
| A virtualised list (`react-window`, `@tanstack/virtual`) | 300 checkpoints and 350 tree points draw in under half a second in Edge (STATUS); revisit if families grow much larger.   |
| A graph layout library (`dagre`, `elkjs`)                | Lanes in time order are a column per event and a row per lane; the engine already gives the edges.                        |
| `axe-core`, `@axe-core/playwright`                       | MPL-2.0 (decision 0011). Keyboard-only Playwright runs, role-and-name queries and the token contrast test cover Phase 3B. |
| Playwright's own browsers (`playwright install`)         | A download of several hundred MB; the installed Edge and the runner's Chrome are enough.                                  |

## Consequences

- The family app's production bundle has the same eight packages as the task pane.
- The Playwright job needs Google Chrome on the runner (present on `ubuntu-latest`), Rust (for the accounts and the wasm) and the pinned wasm-bindgen.
