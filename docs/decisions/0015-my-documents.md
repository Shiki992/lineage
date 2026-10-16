# 0015: My documents: accounts, the library index, and what the page may change

Date: 2026-10-16. Status: accepted.

## Context

Phase 3B (`docs/PROMPTS.md`, BLUEPRINT §6.5) adds "My documents": a static web app where a signed-in person sees every family they track, opens one, and sees its versions, its sent copies and its family tree. CLAUDE.md applies:

- no servers run by the project;
- least privilege;
- plain language;
- never corrupt a file.

The maintainer chose (2026-10-16):

- decks are shown, though none can be made before PowerPoint (Phase 4);
- signing out clears Lineage's cache only;
- milestones can be marked from the page.

Microsoft's documentation was checked on 2026-10-16:

- [What is an App Folder](https://learn.microsoft.com/onedrive/developer/rest-api/concepts/special-folders-appfolder): the folder is created for "the calling app id".
- [Configure a single-page app](https://learn.microsoft.com/entra/identity-platform/scenario-spa-app-configuration).
- msal-browser docs:
  - [initialization](https://github.com/AzureAD/microsoft-authentication-library-for-js/blob/dev/lib/msal-browser/docs/initialization.md);
  - [v4 to v5](https://github.com/AzureAD/microsoft-authentication-library-for-js/blob/dev/lib/msal-browser/docs/v4-migration.md);
  - [redirect bridge](https://github.com/AzureAD/microsoft-authentication-library-for-js/blob/dev/lib/msal-browser/docs/redirect-bridge.md);
  - [logout](https://github.com/AzureAD/microsoft-authentication-library-for-js/blob/dev/lib/msal-browser/docs/logout.md).
- [JSON batching](https://learn.microsoft.com/graph/json-batching): at most 20 requests.
- [Office.UI.openBrowserWindow](https://learn.microsoft.com/javascript/api/office/office.ui): OpenBrowserWindowApi 1.1.

## Decision

### Accounts

- **There are no Lineage accounts.** "Signing in" is the person's own Microsoft account. The screens know only `AccountProvider`:
  - status, sign in, sign out;
  - the person's `Storage`.

  Google (Phase 5) is one more provider.

- **Microsoft:**
  - MSAL's standard public client (`createStandardPublicClientApplication`, msal-browser 5.25.0, as in the add-in);
  - authority `common`, authorization code with PKCE, redirect flow;
  - v5's redirect bridge page (`redirect.html`, `broadcastResponseToMainFrame`), registered as the SPA redirect URI.
- **Same client ID as the Word add-in.** OneDrive ties the app folder to the app id, so both see `Apps/Lineage`.
- **One permission:** `Files.ReadWrite.AppFolder`.
- **Session storage for MSAL's cache:** gone when the tab closes.
- **Sign out is Lineage only:** `clearCache()` for the account, plus everything the page kept under `lineage.` in session storage. The Microsoft session stays, so Outlook and the rest stay signed in. `logoutRedirect` would end that session too.
- **Development builds:** an account for the dev folder (`@lineage/storage-local`), so the app works, and is tested, before a registration exists.

### Only the person's storage, in their browser

- No analytics; nothing sent anywhere but Microsoft sign-in, Graph and OneDrive's download hosts.
- The Content Security Policy allows only those.
- The engine (the same wasm as the task pane) does everything computational.

### The library index

`index.json` sits beside the family folders: `Apps/Lineage/index.json` on OneDrive, `Lineage/index.json` in the dev folder. It is the BLUEPRINT's `Lineage/index.json` at the storage root.

- **Format:** `{v: 1, families: [{family_id, ledger_version, facts}]}`, with `facts` the library entry's data (no wording). `facts: null` is a family folder whose ledger is empty.
- **Kept up to date by every write.** The task pane updates it after each append, without holding up the panel; so do My documents and the harness. It's best effort: a failed update never fails the command.
- **Checked on every visit.**
  - The list is drawn from the index at once.
  - Then every ledger's version is asked for: OneDrive eTags in `$batch` calls of up to 20, one by one if throttled inside a batch.
  - Ledgers that are missing from the index, or at another version, are read. So is every ledger, when the index is missing, unreadable or of another version.
  - **The ledgers win**, and the index is rewritten (with If-Match; a conflict is left for the next visit).

### Reading damaged histories

- `read::read_lenient` folds records until the first line that can't be read or breaks a rule.
- The page shows that much, with "Lineage could read this history up to 14 Oct. 3 later entries are damaged, so nothing will be added to it." Every action is disabled.
- Commands keep the strict reader, so nothing is appended to a damaged ledger.

### What the page may change, and how

Nothing that exists is ever changed.

- **Restore as a new version.** A new tailored version holding exactly what the chosen checkpoint held, saved as a new file, with "Restored from Resume v1".
  - Its `made_from` is the newest master checkpoint the chosen one descends from, so later master changes are offered to it.
  - Those it already has are found by content (decision 0002).
- **Milestones:** `MilestoneMarked {checkpoint_id, label}`. The label is at most 60 characters, trimmed; a null label takes the mark off.
- **A moved file:** `NodeRelinked {node_id, storage_ref}`.
  - **Tailored version.** On the page, "File not found" lists the files in Lineage's folder that no file of the family points at. Only a file whose own Lineage ids name that version is accepted. Otherwise the person can restore its last checkpoint.
  - **Master.** It usually lives outside the app folder, where the app-folder permission can't see. The page says where Word last saw it. When Word opens it from a new address of the same kind (moved or renamed), the task pane records the move. Word reports one file differently on the web and the desktop, so a change of kind is not a move.
- **Decks and workbooks:** the library, filter, icons and tree show them. Compare and open say "Comes with PowerPoint support".

### Sent copies

**Open as sent** downloads the exact bytes of the copy, checked against their hash by the engine, as `<file> v3 (<recipient>).docx`.

### The family tree

- The engine gives the tree's parts (`graph::family_graph`):
  - lanes: master, tailored versions, copies sent, files that came back;
  - nodes;
  - edges: next, made from, brought in (with the count), found by content, restored, sent, came back, kept into.
- The screen lays it out: one column per event, in time order, one row per lane.
- **One tab stop.** Arrow keys move along and between lanes, and Home and End go to a lane's ends.
- Archived lanes are behind a toggle.

### Where it runs

- `apps/family-app` is a static site: `index.html` and `redirect.html`, with hash routes, so any static host serves it.
- In development it runs at `http://localhost:3001`.
- The task pane's footer links to it through `VITE_LINEAGE_APP_URL`, with `openBrowserWindow` where OpenBrowserWindowApi 1.1 is available.
- Production hosting, like the add-in's, is not decided here.

## Alternatives

- **No index, read every ledger on each visit.** Simple, and fine for three families. But it's a download per family before anything draws, and the prompt asks for the index.
- **The index as the truth, rebuilt by hand.** Rejected: two copies of the truth.
- **Folder eTags instead of ledger eTags.** Whether a OneDrive folder's eTag changes when a child changes isn't documented.
- **`logoutRedirect` on sign out.** Ends the Microsoft session for every site in the browser; the maintainer chose Lineage only.
- **msal-react.** Not needed for one provider behind an interface.
- **The OneDrive file picker to relink a master.** It needs broader permissions than the app folder.
- **Relinking a master from the web app by uploading it.** That would copy a file the person keeps elsewhere.
- **Restoring over the old file.** Rejected: CLAUDE.md, never overwrite.

## Consequences

- **The ledger changes shape.** `MilestoneMarked` and `NodeRelinked` are new. Older engines refuse ledgers that contain them, as they refuse any unknown record. Nothing has been released.
- **Every storage adapter gains three methods:** `readIndex`, `writeIndex` and `ledgerVersions`. So does the harness's `Storage` trait; the conformance suite checks them.
- **The index costs about two Graph calls** per command in the task pane, run after the panel has updated.
- **The family app's JavaScript has its own 250 KB gzip budget** in CI.
- **The wasm grew** to 1,159,791 bytes raw and 388,922 gzipped, about 31 KB under the 420 KB budget.
- **Not yet checked:**
  - personal OneDrive's download host names (`*.1drv.com`, and possibly `*.microsoftpersonalcontent.com`; the CSP allows both);
  - whether batched eTags match single requests.

  The maintainer's run checks both.
