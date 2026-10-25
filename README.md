# Lineage

Version control for Word, PowerPoint and Excel files, for people who never want to see git.

Lineage is a free, open-source add-in for Word, PowerPoint and Excel. It keeps one master file in step with the versions you tailor from it, records every copy you send, and shows you exactly what changed when a copy comes back. Every version stays a real file you open as usual. History lives in your own OneDrive, and the project runs no servers.

- [What it is](#what-it-is)
- [What it does](#what-it-does)
- [How it works](#how-it-works)
- [Run it on your computer](#run-it-on-your-computer)
- [Self-host your own copy](#self-host-your-own-copy)
- [Find a problem](#find-a-problem)
- [Fix it: branch, push and pull request](#fix-it-branch-push-and-pull-request)
- [Project rules](#project-rules)

## What it is

People already version documents by hand: "Resume – data roles.docx", "Deck for Northgate v3 FINAL.pptx", "Model (Harbor Lane).xlsx". It works until the master changes and five copies need the same edit, or someone asks "which version did you send me?".

Lineage keeps that way of working and adds the part that's missing. It is built first for personal use:

- **Job seekers** with one master resume and a tailored version for each kind of role;
- **Founders** with one pitch deck and a version for each investor, and a financial model with a version for each round.

Enterprise use on Microsoft 365, and later Google Workspace, comes after.

Lineage never shows git words. The panel talks about a **master**, **tailored versions**, **checkpoints**, **what I've sent** and **files that come back**.

## What it does

**Keep versions in step with the master**

- **Start tracking** a file: it becomes the master, with its first checkpoint.
- **Make a tailored version**: a real copy you edit like any file.
- When the master changes, each tailored version is **offered** the change. You bring in what you want, one change at a time. Your own edits stay, and when both sides changed the same thing, you choose.
- **Update N versions** for decks: tick the versions that should get the new numbers.

**Know what you sent**

- **Send a copy** makes a clean copy: no history, comments, tracked changes or hidden data, only a small Lineage ID so the file can be recognised if it comes back. You can turn even that off.
- **What I've sent** lists every copy, kept exactly as it left, so you can open what someone actually received.
- **Share a link** instead of attaching: a view-only or editable OneDrive link to that copy. Stop sharing whenever you like.

**See only their changes when a file comes back**

- **A file came back**: Lineage recognises the copy (by its ID, or by its content when the ID was stripped) and shows only the other person's changes. Keep or skip each one.
- An edit link that was changed through the link shows up in What I've sent, ready to review the same way.

**Work with other people**

- A file shared with you through a link says **Shared with you**. You edit as usual, and the owner's Lineage sees your changes.
- In Excel, edits by someone editing the workbook with you are labelled as theirs, never yours.

**In each app**

- **Word:** brought-in changes land as real tracked changes, so you review them in Word.
- **PowerPoint:** changes are matched slide by slide and shape by shape.
- **Excel:** every change is sorted into one of five kinds: inputs, formulas, knock-on changes, refreshed data, and structure or formatting. Rows are matched by key or content, so a sorted or filtered sheet doesn't show every row as changed. Macros, pivot tables and Power Query are compared but never run.
- **Excel on the web:** works from the workbook as last saved in OneDrive, because Excel on the web doesn't let add-ins read the open file.

**My documents**

A web page listing every document you track, each with its versions, sent copies and family tree. Compare any two versions, open a copy as sent, or restore an old version as a new one.

**What Lineage never does**

- It never runs macros or queries in any file.
- It never merges silently when it isn't sure: it shows both sides and asks.
- It never sends your documents anywhere but your own storage, and has no analytics.

## How it works

A Rust engine, compiled to WebAssembly, does everything that touches document bytes: reading, comparing, merging, writing and summarising in plain words. Thin TypeScript shells draw the panel inside Word, PowerPoint and Excel, and call Office.js and Microsoft Graph. TypeScript never parses or edits document XML: it passes bytes to the engine and gets bytes back.

| Path                        | What it is                                                                                     |
| --------------------------- | ---------------------------------------------------------------------------------------------- |
| `spec/format/`              | The open change format (JSON Schema): the public, versioned description of a change            |
| `crates/core`               | The document engine for .docx, .pptx and .xlsx: read, compare, merge, write. Pure Rust, no I/O |
| `crates/lineage`            | Family logic: the ledger, checkpoints, versions, offers, sent copies, links. Pure Rust, no I/O |
| `crates/wasm`               | WebAssembly bindings                                                                           |
| `crates/harness`, `testkit` | Test-only: end-to-end journeys on a folder, and invented fixtures                              |
| `packages/shell`            | The typed TypeScript side of the engine, and the storage interface                             |
| `packages/storage-onedrive` | OneDrive through Microsoft Graph                                                               |
| `packages/storage-local`    | Development only: history in a folder on your computer                                         |
| `packages/hosting`          | Build time only: security headers, manifests and release checks                                |
| `packages/design`           | The design tokens                                                                              |
| `apps/office-addin`         | The task pane for Word, PowerPoint and Excel (React)                                           |
| `apps/family-app`           | My documents, plus the Help, Privacy and Terms pages                                           |
| `fixtures/`                 | Test documents, all invented                                                                   |
| `xtask/`                    | Repository tasks (`cargo xtask …`)                                                             |

**Where history lives.** In your OneDrive app folder, **Apps › Lineage**:

```text
Apps/Lineage/<family id>/ledger.jsonl     one append-only log of events per document family
Apps/Lineage/<family id>/blobs/<sha256>   content-addressed snapshots, each part stored once
Apps/Lineage/index.json                   a rebuildable list of families, for My documents
Apps/Lineage/Tailored versions/           your tailored versions
Apps/Lineage/Sent copies/                 the copies you sent
```

**Permissions.** Lineage asks for the narrowest one that works, and asks for more only when a feature needs it:

| Permission                  | When it's asked for                                                                                        |
| --------------------------- | ---------------------------------------------------------------------------------------------------------- |
| `Files.ReadWrite.AppFolder` | At sign-in. Only Lineage's own folder                                                                      |
| `Files.Read`                | The first time you use Lineage in Excel on the web, to read the open workbook as last saved                |
| `Files.ReadWrite`           | The first time you share a copy as a link, and only if OneDrive won't share from the app folder without it |

**Office versions.** The panel checks at start-up and says plainly when an update is needed: WordApi 1.5 in Word, PowerPointApi 1.7 in PowerPoint (1.8 to bring changes into an open deck), and ExcelApi 1.7 in Excel (1.16 to bring changes into an open workbook).

Every significant choice is written down in [`docs/decisions/`](docs/decisions/), and [`docs/STATUS.md`](docs/STATUS.md) says what is built, what is checked, and what isn't yet.

## Run it on your computer

### What you need

- Rust through [rustup](https://rustup.rs). The pinned toolchain in `rust-toolchain.toml` installs itself.
- Node 24 and pnpm 12 (`corepack enable`, or `npm install -g pnpm@12`).
- The wasm-bindgen CLI at the exact version in `Cargo.lock`:

```bash
cargo install --locked wasm-bindgen-cli --version 0.2.129
```

On Windows, keep the clone in a short path such as `C:\src\lineage`, or turn on long-path support. pnpm writes deep folders under `node_modules`, which can hit the 260-character limit.

### Build and test

```bash
pnpm install
```

```bash
cargo test --workspace
```

```bash
pnpm test
```

`pnpm test`, `pnpm lint` and `pnpm typecheck` build the wasm module first.

### Run the task pane

Office loads add-ins only over HTTPS. Once, make and trust a development certificate for `https://localhost:3000` (it changes your trusted certificates, so run it yourself):

```bash
powershell -ExecutionPolicy Bypass -File scripts/dev-cert.ps1
```

On macOS or Linux, use `bash scripts/dev-cert.sh`.

Then add the development add-in to Office. On Windows, close Word, PowerPoint and Excel first:

```bash
powershell -ExecutionPolicy Bypass -File scripts/sideload-word.ps1
```

On Mac or Office on the web, add `apps/office-addin/manifest.xml` yourself: on the web through **Home › Add-ins › More Add-ins › My Add-ins › Upload My Add-in**, and on Mac as Microsoft describes in [Sideload Office Add-ins on Mac](https://learn.microsoft.com/office/dev/add-ins/testing/sideload-an-office-add-in-on-mac).

Start the development server:

```bash
pnpm --filter @lineage/office-addin dev
```

Open a document and choose **Home › Lineage**. In development, history goes to `apps/office-addin/.dev-storage/` (git-ignored), so you need no Microsoft account. To use OneDrive instead, register an app ([`docs/setup/microsoft-app-registration.md`](docs/setup/microsoft-app-registration.md)) and put its client ID in `apps/office-addin/.env.local`, copied from `.env.example`.

### Run My documents

```bash
pnpm --filter @lineage/family-app dev
```

It opens at `http://localhost:3001` and reads the same development folder as the task pane.

### Manual test scripts

[`docs/testing/`](docs/testing/) has a step-by-step script for each journey: Word, My documents, the PowerPoint founder, the Excel pilots, and Excel macros. Use invented documents only.

## Self-host your own copy

Self-hosting means serving the task pane and My documents from your own addresses, with your own Microsoft app registration. People who add your manifest then use your copy. Their history still lives in their own OneDrive: your sites only serve Lineage's program files, and never see anyone's documents or sign-in.

You need:

- a fork of this repository on GitHub;
- a Microsoft Entra tenant to hold the app registration (a free Azure account gives you one);
- two static sites on HTTPS, one for the task pane and one for My documents. The repository is set up for Azure Static Web Apps on the Free plan.

### 1. Register the app

Follow [`docs/setup/microsoft-app-registration.md`](docs/setup/microsoft-app-registration.md). Name the registration `Lineage`: OneDrive names everyone's history folder after it. Add the delegated Graph permissions `Files.ReadWrite.AppFolder`, `Files.Read` and `Files.ReadWrite`. Listing the last two only lets Lineage ask for them when needed.

For each environment (staging and production), add these single-page application redirect addresses:

```text
brk-multihub://<task pane host>
https://<task pane host>/taskpane.html
https://<My documents host>/redirect.html
```

### 2. Create the two sites

In the Azure portal, create two Static Web Apps (Free plan, deployment source **Other**), for example `lineage-addin` and `lineage-app`. Each needs an origin of its own. Copy each site's deployment token. The details, including custom domains and staging, are in [`docs/setup/hosting.md`](docs/setup/hosting.md).

### 3. Deploy from GitHub

In your fork, under **Settings › Environments**, create `staging` and `production`, and in each set:

| Kind     | Name                    | Value                                                       |
| -------- | ----------------------- | ----------------------------------------------------------- |
| Variable | `LINEAGE_ADDIN_ORIGIN`  | the task pane's origin, such as `https://addin.example.org` |
| Variable | `LINEAGE_APP_ORIGIN`    | My documents' origin, such as `https://app.example.org`     |
| Variable | `LINEAGE_CLIENT_ID`     | your app registration's client ID                           |
| Secret   | `AZURE_SWA_TOKEN_ADDIN` | the task pane site's deployment token                       |
| Secret   | `AZURE_SWA_TOKEN_APP`   | My documents site's deployment token                        |

Give `production` a required reviewer. The **Deploy** workflow then runs the whole of CI and deploys:

- each push to `main` to **staging**;
- each tag like `v1.0.0` to **production**, attaching the production `manifest.xml` to the GitHub release.

Until these settings exist, Deploy stops at its first build step and lists what's missing. That is expected in a fork that doesn't host.

### 4. Or build and host it yourself

Any static host works if it serves HTTPS and sends the same headers. Build both sites with your settings:

```bash
export LINEAGE_RELEASE=1 LINEAGE_CHANNEL=production VITE_LINEAGE_VERSION=1.0.0 LINEAGE_BUILD=1 LINEAGE_ADDIN_ORIGIN=https://addin.example.org LINEAGE_APP_ORIGIN=https://app.example.org VITE_LINEAGE_CLIENT_ID=<your client id> VITE_LINEAGE_APP_URL=https://app.example.org/
```

```bash
pnpm build:wasm
```

```bash
pnpm --filter @lineage/office-addin --filter @lineage/family-app build
```

Upload `apps/office-addin/dist` to the task pane's host and `apps/family-app/dist` to My documents' host. A release build fails rather than producing something unsafe: a missing setting, a manifest naming another host, a missing icon, or development code in the build all stop it.

Each `dist` has a `staticwebapp.config.json` with the headers. On another host, send the same ones:

- the `Content-Security-Policy`, HSTS, `X-Content-Type-Options` and `Referrer-Policy` headers;
- `no-cache` on the HTML pages and immutable caching on `assets/`, but icons only cached, never `no-store`;
- `no-store` on My documents' `redirect.html`, and no `Cross-Origin-Opener-Policy` there;
- `.wasm` files as `application/wasm`.

### 5. Give people the add-in

Hand out the production `manifest.xml` from the build or the GitHub release:

- **One person:** Upload My Add-in, as in [Run the task pane](#run-the-task-pane).
- **An organisation:** in the Microsoft 365 admin center, under **Settings › Integrated apps**, upload the manifest and choose who gets it.

Microsoft Marketplace is for the project's own listing ([`docs/release/marketplace.md`](docs/release/marketplace.md)).

**Updating** is a new deploy: pages aren't cached and every script is hashed, so people get the new version the next time they open the panel. Always deploy both sites together, because a newer task pane can write history an older My documents can't read. **Rolling back** is running Deploy by hand on an earlier tag.

Review the Privacy, Terms and Help pages (`apps/family-app/*.html`) before you publish them under your name.

## Find a problem

Good places to start:

- **[`docs/STATUS.md`](docs/STATUS.md):** each phase's **Not done** and **Known issues**.
- **[GitHub issues](https://github.com/Shiki992/lineage/issues)** reported by people using it.
- **The panel itself:** when a screen breaks, Lineage shows "Lineage stopped working" with **Copy details** (version, Office app, error). The details are copied only when you ask, and sent nowhere.
- **The test suites and tools:**
  - `cargo test --workspace` and `pnpm test`, including the property tests;
  - the Rust harness's journeys (`crates/harness/tests/`), on a real folder;
  - Playwright against invented accounts (`pnpm --filter @lineage/family-app e2e`, on the installed Microsoft Edge);
  - the smoke scripts that open the engine's output in real Office or LibreOffice (`scripts/word-smoke.ps1`, `powerpoint-smoke.ps1`, `excel-smoke.ps1`, `libreoffice-smoke.sh`);
  - the fuzz targets in `fuzz/`, which CI runs briefly on every change. Returned files are untrusted input.

When you find something:

1. **Reproduce it with an invented document.** Never put a real resume, deck or anyone's personal data in an issue, a commit or a log.
2. **Security problems stay private.** Use the repository's **Security › Report a vulnerability**, as [SECURITY.md](SECURITY.md) describes: for example a crafted file that crashes or hangs the engine, or hidden data leaking into a clean copy.
3. **Open an issue** for anything else: what you did, what you expected, what happened, which app and platform, and the copied details if the panel showed them.
4. **Turn it into a failing test.** Every engine bug gets a fixture that reproduces it, and a test. Prefer a synthetic fixture built in `crates/testkit`, then run `cargo xtask fixtures`. A file only a real app produces goes in `fixtures/<format>/` with invented content and a line in that folder's README saying which app and version saved it.

## Fix it: branch, push and pull request

### 1. Start from an up-to-date `main`

```bash
git switch main
```

```bash
git pull
```

```bash
git switch -c fix/short-topic
```

Name the branch after the work: `fix/…`, `feat/…`, `docs/…`, or `phase-N-…` for a whole build phase. Contributors without write access fork the repository first and push to their fork.

### 2. Make the change the project's way

- **Plan larger work first:** say what you'll change, which files, and how you'll test it, before writing code. A whole phase starts in plan mode.
- **Tests first in `crates/core`:** the failing test from [Find a problem](#find-a-problem), then the fix.
- **Never corrupt a file:** every write path has a round-trip test (open, write, reopen, compare).
- **Check the platform docs** (Office.js, Microsoft Graph) before using an API, and link the page in a comment wherever behaviour is subtle.
- **Write down significant decisions** in `docs/decisions/NNNN-short-title.md`: context, decision, alternatives, consequences. A new dependency always needs one.
- **Plain language** in every screen, from the glossary in [CLAUDE.md](CLAUDE.md).
- **A licence header** at the top of every Rust, TypeScript and JavaScript file:

```text
// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0
```

[CLAUDE.md](CLAUDE.md) holds the full rules. It's written for people and coding agents alike, so an agent working in this repository follows the same ones.

### 3. Run the checks CI runs

```bash
cargo fmt --all --check
```

```bash
cargo clippy --workspace --all-targets -- -D warnings
```

```bash
cargo test --workspace
```

```bash
cargo deny check
```

```bash
cargo xtask check-headers
```

```bash
cargo xtask check-fixtures
```

```bash
pnpm lint
```

```bash
pnpm typecheck
```

```bash
pnpm test
```

```bash
pnpm format:check
```

```bash
pnpm budgets
```

`cargo deny` needs `cargo install --locked cargo-deny --version 0.20.2`. When a change alters what the engine returns, rewrite the golden replies on purpose with `LINEAGE_UPDATE_GOLDEN=1 cargo test -p lineage-wasm --test golden`, and check the diff.

### 4. Commit with a sign-off

Small commits, in [Conventional Commits](https://www.conventionalcommits.org/) form (`feat:`, `fix:`, `test:`, `docs:`, `ci:`, `refactor:`, `build:`, `chore:`, with an optional scope such as `fix(core):`), each signed off under the [Developer Certificate of Origin](CONTRIBUTING.md#sign-off-dco):

```bash
git commit -s -m "fix(core): keep bookmarks when a paragraph moves"
```

`-s` adds `Signed-off-by: Your Name <you@example.com>` from your git settings. CI refuses a pull request with an unsigned commit.

### 5. Push

```bash
git push -u origin fix/short-topic
```

Git prints a link to open a pull request for the branch.

### 6. Open the pull request

Open it from that link on GitHub, or with the [GitHub CLI](https://cli.github.com/):

```bash
gh pr create --base main --fill
```

One topic per pull request. Say what changed, why, how you tested it, and anything a reviewer should know: a new decision note, regenerated golden replies, a change to the ledger or the open change format, or something only a real Microsoft account can check.

### 7. Get it green, reviewed and merged

CI runs on every pull request:

- **Rust on Linux and Windows:** formatting, clippy, tests, licence headers, fixtures;
- **cargo-deny:** licences, advisories, duplicate crates;
- **wasm and TypeScript:** lint, typecheck, tests, Prettier, size budgets;
- **Playwright** for My documents;
- **short fuzz runs** of every reader;
- **DCO:** every commit signed off.

Fix what fails, push again, and the pull request updates. The maintainer reviews and merges. Merging to `main` deploys to staging; a release is a tag (`git tag -a v1.0.0 -m v1.0.0`, then `git push origin v1.0.0`), which deploys to production after approval.

## Project rules

- **Never corrupt a user's file.**
- **Never merge silently when unsure:** show both sides and let the person choose.
- **Copies that leave are clean,** with only a minimal Lineage ID that can be turned off.
- **Never run anything inside a file.** Macro and query code is read as text.
- **No servers run by the project.** History lives in the person's own storage.
- **Least privilege:** the narrowest permission that works.
- **Plain language and accessible by default:** real buttons and labels, 44 px targets, 4.5:1 contrast.

More in [CONTRIBUTING.md](CONTRIBUTING.md), [GOVERNANCE.md](GOVERNANCE.md) and the [Code of Conduct](CODE_OF_CONDUCT.md).

## Licence

[Apache-2.0](LICENSE). Contributions are accepted under the [Developer Certificate of Origin](CONTRIBUTING.md#sign-off-dco).
