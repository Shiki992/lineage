# Lineage blueprint

This is the text version of the Lineage blueprint design canvas. The canvas has three pages (Microsoft 365, Google Workspace + Slack, Personal users) with journey trees, family trees, screen mockups and architecture diagrams. Where the canvas and this file disagree, ask the maintainer.

---

## 1. The idea in one paragraph

People already version documents by hand: "Resume – data roles.docx", "Deck – Northgate v3.pptx", "Budget FINAL v27.xlsx". Lineage keeps that habit, where every version is a real file, and adds the missing part: the files remember where they came from. A master can offer its changes to every version that follows it. Every copy that leaves is recorded, so you can open exactly what someone received. A file that comes back edited is recognised, and you see only the other person's changes.

## 2. What is distinctive (protect these)

1. **Variants that follow a master.** Change the master once, and each tailored version is offered the change. People pick what to bring in, and their local edits stay.
2. **Sent copies and returned files.** Every outside copy is recorded and kept exactly as sent. When it comes back, Lineage matches it to the checkpoint it left from and shows only the outside changes.
3. **Works where people already are.** Inside Word, PowerPoint and Excel (later Google Docs, Slides and Sheets), with no new app to learn.
4. **Cross-platform history** (later). One family can hold a Google Doc and the .docx downloaded from it.

## 3. Users and journeys

### Personal users (build first)

- **Getting started.** Adds Lineage from the Word or Google add-on store, signs in with a personal Microsoft or Google account, opens a file and clicks "Start tracking". History is kept in their own OneDrive or Drive. Free and open source, with no Lineage server.
- **Job seeker.** Keeps one master resume and makes a tailored version for each kind of role. When they add a new job to the master once, every tailored version is offered the change. When they send a copy, Lineage records who got which version.
- **Founder.** Keeps one master pitch deck and makes a version for each investor or customer. When they update the traction numbers in the master, they pick which versions get the new numbers. When an investor replies, they open exactly what that investor saw.
- **Asking for feedback.** Sends a clean copy to a mentor or friend, who edits it and sends it back. They drop the returned file into Lineage, see only the other person's changes one at a time, and keep the ones worth keeping.

### Enterprise on Microsoft 365 (later)

- **IT admin:** deploys the add-in centrally, registers the app in the company's own Entra ID, chooses where history is kept, and sets rules such as protected masters and outside sharing.
- **Owner:** starts tracking a file, which becomes the master copy. Decides who may edit the master directly, saves checkpoints and marks milestones.
- **Contributor:** makes a working copy and edits it as usual. Clicks "Catch up" when the master has moved ahead, picks a side where both changed the same thing, and suggests the changes with a note.
- **Reviewer:** gets a request in Outlook or Teams and reads a plain-English summary. Compares side by side, accepts or rejects each change, and approves, which gives the master a new checkpoint and archives the working copy.
- **Variant owner:** makes a variant (for example EMEA), sees "3 updates from the master", picks which to bring in, and copies a local fix to other variants.
- **Outside round trip:** sends a clean copy, it comes back, and Lineage recognises it ("v5, sent to the auditor on 3 Oct") and turns the changes into a suggestion.

### Enterprise on Google Workspace + Slack (later)

The same paths. The add-on is installed from the Workspace Marketplace, sign-in is set up as an internal app, the master is locked in Drive, and requests and approvals arrive in Slack. A reviewer can approve from Slack when nothing overlaps. In Docs, changes arrive as real suggestions. A returned file can arrive in a shared Slack channel.

## 4. The family model

### Entities

- **Family** `{ id, name, format: docx|pptx|xlsx|gdoc|gslides|gsheet, masterNodeId, createdAt }`
- **FileNode**: a real file in the family. `{ id, familyId, role: master|variant|working_copy|sent_copy|returned, displayName, storageRef, madeFromCheckpointId, followsMaster: boolean, lastSyncedMasterCheckpointId, status: active|archived }`
- **Checkpoint**: a saved state of one FileNode. `{ id, familyId, nodeId, parentIds: [id] (two parents for a merge), snapshotRef, note, author, createdAt, milestone?: string }`
- **SentCopy**: `{ id, familyId, fromCheckpointId, recipient, purpose?, format: docx|pdf|pptx|xlsx, sentAt, snapshotRef, idEmbedded: boolean }`
- **Offer**: master changes offered to a variant. `{ id, fromCheckpointId, toNodeId, changeIds[], status: pending|applied|partly_applied|skipped, conflicts[] }`
- **ChangeState**: what one variant did with one master change. `{ familyId, nodeId, changeId, fromCheckpointId, state: merged|declined|pending, how: clicked|detected, decidedAt }`. See "Change states" below.
- **Suggestion** (enterprise, keep the shape now and build later): `{ id, fromNodeId, toNodeId, baseCheckpointId, note, reviewers[], decisions[], status }`

### Ledger and snapshots

- One append-only event log per family (JSON Lines). Examples: `FamilyStarted`, `CheckpointSaved`, `VariantMade`, `UpdateOffered`, `UpdateApplied`, `CopySent`, `FileReturned`. Current state is derived from the events.
- Appends to cloud storage use optimistic concurrency (ETag / revision check) and retry.
- Snapshots are content-addressed blobs (SHA-256) of package parts, so images and fonts are stored once across all versions.
- Storage layout, per family: `Lineage/<familyId>/ledger.jsonl`, `Lineage/<familyId>/blobs/<sha256>`.

### Identity

- docx/pptx/xlsx: custom document properties `LineageFamilyId` and `LineageCheckpointId`, nothing else. In a sent copy, `LineageCheckpointId` holds the sent copy's own id, which the ledger maps to its checkpoint, so a returned file names whose copy it is (decision 0012).
- Fallback when IDs are stripped: a content fingerprint (normalised text shingles plus structure), matched against stored checkpoints, then compared word by word with what actually left, with a confidence score shown to the user. Only a clear match is taken without asking (decision 0013).
- PDF sent copies: no ID inside by default. The sent record is enough to "open what they got".

### Change states (decided 2026-10-15, see docs/decisions/0002-change-states.md)

- Every master change offered to a variant is tagged for that variant: **merged** or **declined** when the person clicks, otherwise **pending**.
- A pending change is checked against the variant's content. If the variant already contains it (copied in by hand, or the file was made outside Lineage), it is tagged merged (`how: detected`) and the variant is linked as a descendant of that checkpoint, which fixes the tree. If not, the variant is treated as independent of that checkpoint: it stays in history, but is not offered and triggers no notification.
- Declined changes are not offered again unless the person asks.
- The merge base for a variant is the newest master checkpoint whose changes are all merged or declined for it. Later changes are judged one by one, so a variant can take v3 without v2.

### Rules

- **Catch up and offers** are three-way merges with base = the merge base defined under "Change states".
- **Conservative merge.** A change is auto-applied only if no other change touches the same unit (paragraph, table cell, slide shape, sheet cell). Otherwise it is a conflict that the person decides.
- **Formatting-only changes** are reported separately from content changes.
- **Sent copies are immutable.** Later edits never change them.
- **Archived nodes** stay in history and are hidden by default.

## 5. Format notes for the engine

- **Word (.docx).** A sequence model: blocks (paragraphs, tables, section breaks) → runs. Align blocks with a diff that is stable under moved paragraphs, then diff runs inside matched blocks. Show merges as real tracked changes (`w:ins` / `w:del`) so people review them in Word. Preserve unknown XML untouched.
- **PowerPoint (.pptx).** A slide model: slides → shapes → text frames. Match slides by ID and layout, shapes by ID, and fall back to position plus text similarity. Merge at slide level first, then shape text. Visual before/after rendering comes later.
- **Excel (.xlsx), later phase.** A grid model: sheets → cells, with formulas as the source of truth and cached values ignored for diffs. Align inserted or deleted rows and columns before comparing cells. Excel has no tracked changes, so the review happens in the panel or a review sheet.
- **Google Docs, later phase.** The Docs API structural JSON maps to the same canonical sequence model. Write updates with `documents.batchUpdate` with `writeMode: SUGGEST`, so they appear as real suggestions (some request types are not supported in suggest mode).
- **Google Slides and Sheets.** There are no suggestions, so the review happens in the sidebar.

## 6. Screens

### Personal (canvas page "Personal users")

1. **Master resume panel (Word).**
   - Shows file name with a "Master" chip and "2 tailored versions follow this file".
   - Latest checkpoint card.
   - A call-out when the master has changes the versions don't have, with an "Offer it to both versions" button.
   - A list of tailored versions with "Has v3 · last sent 2 Oct" and an Open button.
   - Actions: Make a tailored version, Save checkpoint, Send a copy.
   - Footer link: "What I've sent".
2. **Send a copy.**
   - Pick the version, who it's for, an optional role, and the format (PDF or Word).
   - A lock note: "Always a clean copy".
   - Primary button "Save copy and record it", with "Later you can open exactly what they received."
3. **What I've sent.**
   - Search, then rows grouped by month: recipient, version, format, date, and Open.
   - A returned copy shows a "Came back with 5 changes" chip and a Review button.
   - Note: "Each copy is kept exactly as sent."
4. **Pitch deck panel (PowerPoint).**
   - "You just changed Slide 6 · Traction" with before → after figures.
   - Checkboxes for which tailored versions to offer it to, with context such as "Sent 1 Oct with the old numbers".
   - "Update 3 versions" button.
   - A "Pellow replied · Open what they saw" card.

### Enterprise Microsoft (canvas page "Microsoft 365")

Panels: master copy, working copy (with a "master moved ahead → Catch up" banner), suggest changes, variant updates, and file came back. App pages: review a suggestion (per-change Accept/Reject, a "Both changed this" decision, side by side, "Approve and create v7" disabled until decided), and the family tree view (lanes for variants, master, working copies and outside copies).

### Enterprise Google + Slack (canvas page "Google Workspace + Slack")

Sheets sidebars for the master and working copy, and a Docs sidebar for variant updates that arrive as suggestions. Slack: a review request with Approve available only when nothing overlaps, a home tab, and a returned file recognised in a shared channel.

## 7. Architecture

- **Personal:** the add-in plus core run on the user's machine, with storage in the user's OneDrive or Drive. No project servers. The project publishes one public add-in listing per store, which needs an owner.
- **No command line for users** (decided 2026-10-15, see docs/decisions/0001-no-command-line.md). Every action is a button. A test-only Rust harness drives end-to-end scenarios against a local folder. The canvas's "Command line and git drivers" box is out of scope.
- **Enterprise Microsoft:** the same, plus each company registers the app in its own Entra ID. The family app is a static web app. An optional self-hosted helper handles background jobs.
- **Enterprise Google + Slack:** the same engine, with a Drive adapter. Sign-in is an internal app per company, which is exempt from Google verification. Slack interactivity needs a running endpoint, so a small Lineage helper runs in the company's own Google Cloud. It also receives Drive change alerts.

### Platform facts already checked (re-verify before building)

- Google `drive.file` is non-sensitive. Full `drive` and `drive.readonly` are restricted.
- Google internal-only apps are exempt from OAuth verification.
- The Google Docs API can write changes as suggestions (`writeMode: SUGGEST`).
- Drive content restrictions (`contentRestrictions.readOnly`, with an owner-restricted option) can lock the master. A lock does not limit access.
- Slack button clicks need a request URL or Socket Mode, so there is always a running service.

## 8. Open decisions (decide in the relevant phase, record in docs/decisions)

1. ~~**Personal OneDrive layout.**~~ Decided 2026-10-15 in docs/decisions/0009-onedrive-layout.md: the app folder (`Files.ReadWrite.AppFolder`), with "next to the master" kept as a development prototype. To be confirmed by the maintainer's OneDrive run.
2. ~~**PDF export.**~~ Decided 2026-10-15 in docs/decisions/0008-word-addin-platform.md: skipped in v0. Word exports PDF only from the open document, which still has comments, tracked changes and properties.
3. ~~**Fingerprint threshold.**~~ Decided 2026-10-15 in docs/decisions/0013-fingerprint-threshold.md: automatic only for a sent copy matching at least 80% with at least 5 fewer changed words than any other copy; otherwise "Which copy is this?" with the candidates and "None of these". Measured: 100% precision, 80% automatic recall where copies differ, 100% found.
4. ~~**Wasm budgets.**~~ Set 2026-10-15 in docs/decisions/0010-wasm-boundary-and-budgets.md: wasm at most 1.5 MB raw and 420 KB gzipped, JavaScript at most 250 KB gzipped (enforced in CI); start-up and command times measured and recorded.
5. **Start from files made by hand.** Many people already have a master and dozens of tailored copies made by hand (the maintainer's own resume folder is this shape). Lineage should match each file to the master by content and propose the family tree for the person to confirm, using the same detection as "Change states". Decide which phase.
6. ~~**How accepted changes land in the file.**~~ Decided 2026-10-15 in docs/decisions/0003-accepted-changes.md: once the owner approves a change in the panel, it is written into the file as a tracked change, and Lineage then saves a checkpoint in the version history.
