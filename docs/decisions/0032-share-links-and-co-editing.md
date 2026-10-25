# 0032: Share links: co-editing a tracked file, and sending a copy as a link

Date: 2026-10-25. Status: accepted. OneDrive's real behaviour (below) is the maintainer's run to check.

## Context

People share Office files as OneDrive links, and several people edit the same file. The maintainer asked for Lineage to work with that (2026-10-21), in both cases: **others co-edit a tracked file through a link**, and **a sent copy goes out as a link** instead of an attachment.

Lineage keeps each person's history in their own OneDrive app folder (decision 0009), so a co-editor can't see the owner's history. Checked on 2026-10-21:

- [createLink](https://learn.microsoft.com/graph/api/driveitem-createlink) lists `Files.ReadWrite` as the least-privilege permission, not `Files.ReadWrite.AppFolder`. It makes `view` or `edit` links, `anonymous` on personal OneDrive and `organization` on business, and gives back an existing link of the same type (200).
- [Deleting a permission](https://learn.microsoft.com/graph/api/permission-delete) removes a link.
- A Graph PUT to a file open in Office gives 423 `resourceLocked`, and nothing says who has it open ([Q&A](https://learn.microsoft.com/answers/questions/5654593)).
- Excel's `onChanged` has `source`, Local or Remote, where Remote is a co-author ([WorksheetChangedEventArgs](https://learn.microsoft.com/javascript/api/excel/excel.worksheetchangedeventargs)). Word's paragraph events carry the same from WordApi 1.6 ([ParagraphChangedEventArgs](https://learn.microsoft.com/javascript/api/word/word.paragraphchangedeventargs)). PowerPoint has nothing like it.

## Decision

### Co-editing a tracked file

- **A co-editor sees "Shared with you"**, not "sign in with the account you used". `Storage.holds(url)` says whether the document's address is in the person's own drive. OneDrive judges it by the owner in the address, against the app folder's drive id and address, which the app-folder permission may read:
  - `d.docs.live.net/<cid>/…`, or `cid=` / `resid=<CID>!…` on the web, against the drive id;
  - `…-my.sharepoint.com/personal/<user>/…` against the folder's address.

  A team site, a local file or a failed lookup is "can't tell", and the screen names both reasons. Nothing on either screen writes to the file, and the engine already refuses to give a tracked file new ids.

- **Whose edit it was (Excel).** Each edit hint keeps its event's `source`. The engine's `Hint` gains `by`, and a changed cell gains `edited_by` when its provenance is a direct edit: you, a co-author, or both. A co-author's edit reads "Edited by someone you share it with, in this session", never the person's own. The open change format gains `editedBy` (`you`, `coAuthor`, `both`), allowed only with `directEdit`. It is additive within v0, and readers that don't know it can ignore it.
- **Word** counts Remote paragraph events since the panel opened or the last checkpoint, and the panel says "What changed includes their edits". Word can't say which edits were theirs, so it is a hint only, and nothing goes in the ledger.
- **A version someone has open.** "Not updated: it's open" now says it may be open by someone it was shared with. A co-author's autosave between Lineage's read and its write makes the write refused (412, `FileChanged`), never overwritten, and the next press keeps their edits as the version's own (a test covers it).

### Sending a copy as a link

- **What is shared is exactly what left.** The file in Sent copies is used when its bytes match the recorded copy; otherwise the exact bytes are written there again from history.
- **The ledger** gains `LinkShared { sent_copy_id, link_id, access, url, file_ref, scope }` and `LinkStopped { sent_copy_id, link_id }`. A sent copy has at most one live link. The same link again records nothing (OneDrive gives back the same link); another waits until the live one is stopped. The record version stays at 1. An older Lineage refuses a ledger with these events, so the two sites always deploy together (decision 0031).
- **Sent copies stay immutable.** The recorded copy never changes. With an edit link, the shared file is a live copy whose edits come back as a returned file, recognised by the copy's own Lineage ID, and reviewed like any other (Word and workbooks; decks still say "comes later"). For each live edit link, What I've sent and My documents carry the shared file and the SHA-256 of every version of it Lineage has seen. A shared file with any other hash was changed through the link.
- **A copy that comes back again** asks only about changes not already kept or skipped from earlier returns of the same copy, and its count leaves those out too. This holds for a second emailed file as well as for an edit link.
- **Least privilege.** OneDrive tries `createLink` and the permission delete with the app-folder token first. Only on 403 does it ask for `Files.ReadWrite`, through MSAL and only then, so if Microsoft allows sharing from the app folder, nobody is ever asked. The share screen says before Microsoft does: "Its screen says 'all your files'; Lineage only shares the copies you choose." Personal accounts ask for an anyone-link; work accounts get their organisation's default, and the pane shows what came back.
- **If the history can't record a link**, the link is removed again, so nothing is shared that the history doesn't show.
- **The dev folder** makes stand-in links (`https://dev-folder.invalid/…`) with the same rules, so the whole journey runs without a Microsoft account. Editing the shared file in the dev folder stands in for the recipient.

## Alternatives

- **Keep history next to the master, so co-editors share it.** That needs `Files.ReadWrite` for everyone, and it is enterprise ("Share family") work, which CLAUDE.md keeps out of personal phases.
- **Ask for `Files.ReadWrite` at sign-in.** That would show everyone the "all your files" screen for a feature most may never use.
- **Store link details in the sent copy itself.** Copies that leave carry only the minimal Lineage ID.
- **Keep the link's state only in OneDrive** (its permissions). The history would no longer say what was shared and when, and My documents would need the wider permission to show it.

## Consequences

- **Unknown until the maintainer's OneDrive run:**
  - whether items in the app folder can be shared with the app-folder permission;
  - the exact addresses Office reports for shared files on each platform;
  - what Word and Excel on the web do with a version Lineage replaces while someone has it open;
  - whether `getFileAsync` includes co-authors' changes that aren't merged yet.
- What I've sent downloads each live edit link's file when it opens, to compare hashes. That is fine at personal scale, but it grows with the number of live edit links.
- A workbook family's tree shows a returned file for each time an edit link's changes were brought back.
