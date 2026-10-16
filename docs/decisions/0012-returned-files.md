# 0012: Files that come back

Date: 2026-10-15. Status: accepted.

## Context

BLUEPRINT §3, "Asking for feedback": a person sends a clean copy to a mentor or friend, it comes back edited, and they want to see only the other person's changes, one at a time, and keep the ones worth keeping. Phase 1 left placeholders (`FileReturned`, `ReturnReviewed`, a sent-list chip, fingerprints). Phase 3 had to settle six things:

1. how a returned file reaches the panel in Word;
2. what a sent copy carries;
3. which changes are "theirs";
4. where they are written;
5. how decisions are recorded;
6. what LibreOffice is used for.

The maintainer chose (2026-10-15) the sent-copy id, and real LibreOffice re-saves for fixtures.

## Decision

### How a returned file reaches the panel

- **"A file came back"** on the master and version panels and in What I've sent:
  - a drop zone and a **Choose file** button (an `<input type="file">`);
  - the bytes go straight to the engine, and nothing is stored until the person confirms which copy it is;
  - both work in Word for Windows, which hosts the pane in WebView2 (checked in this phase).
  - Microsoft documents the standard HTML drag-and-drop API in WebView2 task panes for classic Outlook ([Drag and drop items into the task pane](https://learn.microsoft.com/office/dev/add-ins/outlook/drag-drop-items)); Word has no add-in drag events of its own.
- **The returned file opened in Word itself:**
  - if its ids name a sent copy, or a checkpoint that isn't its file's latest, the pane shows **This file came back** instead of the file's panel. So a returned copy is never shown, or saved, as the master;
  - an untracked document is checked in the background, and if it looks like a copy that came back, the Start tracking screen says so first;
  - after the person confirms, the panel offers **Open "<file it came from>"**, because kept changes must be written into that file while it is open.
- **Protected View:** Word opens email attachments in Protected View, where add-ins don't run. The person chooses **Enable Editing** first, or saves the attachment and uses **A file came back**.

### What a sent copy carries

A sent copy's `LineageCheckpointId` holds **the sent copy's own id**, not the checkpoint's.

- It is still exactly two custom properties of the same shape, and no more revealing: a random 128-bit id that the ledger maps to its checkpoint (`CopySent`).
- So a returned file names exactly whose copy it is, even when several copies left from one checkpoint.
- `check_file` refuses a file that carries a sent-copy id, so a returned copy can't be checkpointed as its node.
- Copies made before this change carry a checkpoint id. They are still recognised: as that checkpoint, with a choice between the copies recorded from it.

### Only their changes

The review is `merge::plan(base, ours, theirs)`:

- **base** is what left: the sent copy's exact bytes, or the checkpoint's snapshot when nothing was recorded as sent;
- **ours** is the open file as it is now;
- **theirs** is the returned file, read in the accepted view, so a mentor's own tracked changes count as their proposals.

So:

- only the outside person's changes are offered, even if the master moved on;
- a paragraph both sides changed is an overlap, shown with both sides (**Keep mine** / **Use theirs**), never merged silently;
- change ids come from content, so the same file gives the same cards in a later sitting.

### Where they are written

Into the open file:

- as tracked changes by **Lineage** (decision 0003), written, read back and checked exactly as `decide` does (decision 0008);
- then a checkpoint, "Kept 3 changes from Mentor", with the file's previous checkpoint as its parent. On the master, those changes become an ordinary master checkpoint that can be offered to tailored versions.

Normally the target is the file the copy was made from. Reviewing into another file of the family is allowed, with a note saying so, and the panel offers **Open "<file it came from>" instead**.

### Recorded

- **`FileReturned`** (reshaped in Phase 3; nothing had written it):
  - the returned file is kept exactly, as a blob;
  - with its copy and checkpoint;
  - `matched_by` (`ids`, `content` or `person`) and the content match;
  - counts of content and formatting changes.

  Dropping the same file again finds the same record.

- **`ReturnReviewed`**: one per sitting, with kept and skipped ids and whether anything was left.
- **What I've sent** shows _Came back with N changes_, _Came back with formatting changes only_, _Came back unchanged_, _Partly reviewed_ or _Reviewed_.

### LibreOffice

A development tool only, never shipped. It makes the real re-save fixtures (`scripts/libreoffice-resave.ps1`, `fixtures/docx/returned/libreoffice/`), and runs the headless open check CLAUDE.md asks for. Installed with `winget install TheDocumentFoundation.LibreOffice` (MPL-2.0; nothing in the repository links to it).

## Alternatives

- **Keep the checkpoint id in sent copies, and tell a returned copy from the master by its location.** Word reports different URLs for one file (a local path, `d.docs.live.net`, the web), and a master that moves would be asked about on every open. Rejected for the sent-copy id.
- **A third property for the sent copy.** BLUEPRINT and CLAUDE.md allow two.
- **Base = the checkpoint snapshot, even when a copy was sent.** Cleaning removes comments and hidden text, so the outside person would appear to have deleted them.
- **Write each kept change as soon as it is clicked.** That is a Word write and a checkpoint per click (about a second each). Decisions are made one at a time and applied together.
- **Author the tracked changes as the recipient.** Clearer in Word's margin, but decision 0003 and the write check (`check::written`) rely on every Lineage change being by "Lineage". Possible later.

## Consequences

- **Not shown yet:** comments a mentor leaves are not part of "what changed". **Open what came back** opens the file exactly as it came, to read them.
- **Formatting of kept changes:** a paragraph's formatting came with a kept change, so a Google Docs download brought its fonts and spacing with the words. Since decision 0014, for a file another app saved (LibreOffice by name, an export without `docProps/app.xml`, or formatting changed almost everywhere), Keep brings only their words, in the master's formatting:
  - the review says why;
  - "Bring in their formatting too" brings theirs;
  - links they added, inserted rows and tables, and new paragraphs with nothing alike in theirs still keep their own formatting.
- **Known issue:** a file attached by hand (the master itself, not a sent copy) that comes back while the master is still at that checkpoint opens as the master. It carries the same checkpoint id, and no location check is made.
- **The ledger changes shape:**
  - `FileReturned`, `ReturnReviewed` and `MatchedBy` are new in use;
  - older engines refuse ledgers that contain them, as they refuse any unknown record. Nothing has been released.
