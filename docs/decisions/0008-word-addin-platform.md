# 0008: The Word add-in: sign-in, manifest, writing into the open document, and development set-up

Date: 2026-10-15. Status: accepted.

## Context

Phase 2 runs the Phase 1 engine in a Word task pane. Four things had to be settled against Microsoft's current documentation (checked 2026-10-15), and one of them, whether Word keeps the engine's tracked changes when the add-in writes a file into the open document, was undocumented and had to be tested in Word:

1. how a personal user signs in and how the pane gets a Graph token;
2. which manifest;
3. how the engine's output reaches the open document, without corrupting it (CLAUDE.md, first non-negotiable);
4. how a developer runs and checks the pane in Word on Windows.

The maintainer also chose (2026-10-15) that "Send a copy" makes Word copies only in v0.

## Decision

### Sign-in: nested app authentication (NAA)

The pane uses MSAL.js's nestable client (`createNestablePublicClientApplication`), so Word signs the person in and brokers tokens.

- NAA is generally available in Word on the web, Windows and Mac, and supports personal Microsoft accounts ([NAA requirement set](https://learn.microsoft.com/javascript/api/requirement-sets/common/nested-app-auth-requirement-sets)).
- The pane asks only for the Graph permission its OneDrive layout needs (decision 0009). It tries silently at start-up, and shows a sign-in button otherwise.
- On the web it passes the login hint from `Office.auth.getAuthContext()`. NAA works there only for documents opened from OneDrive or SharePoint.
- The registration needs SPA redirects `brk-multihub://localhost:3000` and the task-pane URL ([Enable NAA](https://learn.microsoft.com/office/dev/add-ins/develop/enable-nested-app-authentication-in-your-add-in)).
- The client ID lives in a git-ignored `.env.local`; `docs/setup/microsoft-app-registration.md` walks through the registration.
- Registering from a personal account outside a directory is no longer possible, so the maintainer registers in a free tenant.
- Office that lacks `NestedAppAuth 1.1` shows "Update Office". A dialog-API fallback is left for later.

### Manifest: add-in only (XML)

`apps/office-addin/manifest.xml` is the add-in-only manifest, which is valid for production Word add-ins ([Manifests](https://learn.microsoft.com/office/dev/add-ins/develop/add-in-manifests)). It adds a Lineage button on the Home tab and requires WordApi 1.5. Office.js loads from Microsoft's CDN (`officeapis.public.onecdn.static.microsoft`), as add-ins must.

### Writing into the open document

Reading and writing are whole-file, and TypeScript never touches XML:

- **Reading:** `getFileAsync(Compressed)` in slices of at most 4 MB, closing the file afterwards ([Get the whole document](https://learn.microsoft.com/office/dev/add-ins/develop/get-the-whole-document-from-an-add-in-for-powerpoint-or-word)).
- **Ids:** written as custom document properties through Office.js (WordApi 1.3). The engine reads the same properties from the bytes; the diagnostics screen checked that in Word.
- **Writing a brought-in change:** `Document.insertFileFromBase64(file, "Replace", …)` (WordApi 1.5; custom properties and custom XML parts with 1.6). Track Changes is switched off for the insert and restored afterwards; otherwise Word would record the whole file as the person's own insertion. The limit is 4 MB, and document settings are not imported, so the open document keeps its own ([Document.insertFileFromBase64](https://learn.microsoft.com/javascript/api/word/word.document)).
- **Every write is read back and checked** (`lineage_core::check::written`):
  - every story must read the same with every revision accepted and with every revision rejected;
  - every Lineage revision must still be Lineage's.

  If not, the original is put back the same way (and checked), nothing is appended to the history, and the panel says so. Only then is the file saved and the ledger appended.

- **Opening files:**
  - "Open" on a version uses `Application.openDocument` (WordApi 1.6; cloud URLs only on the web). Word desktop asks the person to confirm before an add-in opens a file.
  - "Open what they received" uses `createDocument(bytes).open()`, which opens the exact bytes as a new, unsaved document.

**The spike** (Word 16.0.20430, Current Channel, Windows 11) wrote each of the engine's 57 Word smoke-test outputs into an open document this way and read it back:

- **First run: 11 of 57.** Once a file with notes had gone in, Word read every later file back with footnote and endnote parts holding only its separators. The first version of the check counted those empty stories as a difference.
- **The fix:** empty stories are now left out of the comparison (with a regression test).
- **Second run: 57 of 57 landed exactly.** Same text in both views, every Lineage revision still by "Lineage", nothing added by any other author.
- **Speed:** a write plus read-back took 0.9 to 6.5 s (median about 1.3 s; the slowest were the largest table and move files).

Word also re-saves files in its own way (revision ids, dates). So "has this file changed?" now asks the engine whether any story changed, in both views, instead of comparing bytes. Otherwise every send after a write made an empty checkpoint.

### Development set-up

- **HTTPS:** add-ins must be served over HTTPS; a self-signed certificate is fine once trusted. `scripts/dev-cert.ps1` makes a `localhost` certificate in the user's profile and trusts it. The maintainer runs it once, because it changes the trusted-root store.
- **Sideloading:** `scripts/sideload-word.ps1` adds one value under `HKCU\Software\Microsoft\Office\16.0\Wef\Developer` naming the manifest (what Microsoft's `office-addin-dev-settings` writes for an XML manifest), or removes it.
- **The dev server, development builds only:**
  - keeps history in a local folder in the harness's layout (`apps/office-addin/.dev-storage`), so the panel can be tried in Word without OneDrive, and `crates/harness/tests/dev_folder.rs` can audit what it wrote with the Rust engine alone;
  - takes timings and error kinds (never text) for the budgets;
  - serves the spike files to a diagnostics screen.
- **Driving the pane:** WebView2 task panes don't show in some screen captures. For automated checks, Word can be started with `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222` for that one launch, and the pane driven through the DevTools protocol. Close that Word afterwards.

### PDF: skipped in v0 (BLUEPRINT open decision 2)

Word exports PDF only from the open document (`getFileAsync(Pdf)`). That document still has its comments, tracked changes, hidden text and properties, so a clean PDF would need the clean copy opened in Word first. "Send a copy" makes clean Word copies and says "PDF is coming later".

## Alternatives

- **Legacy Office SSO (`getAccessToken`)**: needs a server for the on-behalf-of flow, and Lineage has no servers.
- **Office dialog API with MSAL popups**: works on older Office, but signs in separately from Word. Kept as a later fallback.
- **Unified manifest**: not needed for a Word-only add-in, and not accepted by network-share sideloading.
- **Editing the open document range by range with Office.js**: Word would attribute every change to the person, not to "Lineage" (decision 0003), and TypeScript would have to map engine changes onto ranges.
- **Overwriting the file in OneDrive while it is open**: conflicts with Word's own saving and co-authoring.
- **Microsoft's `office-addin-dev-certs` and `office-addin-debugging`**: they bring usage-data reporting, `mkcert`, `keytar` and the Agents Toolkit CLI; two short scripts do what Lineage needs.

## Consequences

- Files over 4 MB can't be changed while open; the panel says so.
- Bringing changes in costs one whole-document write and read in Word (about a second for a resume).
- The 4 MB limit and Word's re-saving shape the engine: change detection is by content, never by bytes.
- Word on the web and OneDrive sign-in are checked by the maintainer (`docs/testing/word-personal.md`) once the app registration exists.
