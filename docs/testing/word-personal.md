# Manual test: Lineage in Word for personal users

The Phase 2 "done when" check, run by hand: track a resume, make two tailored versions, offer a change from the master, send a copy and reopen it, on Word for Windows and on Word on the web, with history in your own OneDrive. About 20 minutes. Use a copy of a resume you don't mind Lineage writing to.

## Once, before the first run

1. **Trust the development certificate.** Run this yourself: it adds a certificate for `localhost` to your trusted roots, and Windows asks you to confirm.
   ```bash
   powershell -ExecutionPolicy Bypass -File scripts/dev-cert.ps1
   ```
2. **Register the app** with Microsoft Entra ID and put its client ID in `apps/office-addin/.env.local`, with `VITE_LINEAGE_STORAGE=onedrive`. See [docs/setup/microsoft-app-registration.md](../setup/microsoft-app-registration.md).
3. **Sideload in Word for Windows** (Word closed). This adds one value under your user's Word developer settings:
   ```bash
   powershell -ExecutionPolicy Bypass -File scripts/sideload-word.ps1
   ```

Start the task pane's server and leave it running:

```bash
pnpm --filter @lineage/office-addin dev
```

Without step 2, or with `VITE_LINEAGE_STORAGE=dev-folder`, history goes to `apps/office-addin/.dev-storage` instead of OneDrive. That is how the run below was first done (STATUS, Phase 2). Afterwards you can audit that folder with the Rust engine:

```bash
cargo test -p lineage-harness --test dev_folder -- --ignored --nocapture
```

## Word for Windows

| #   | Do                                                                                                                                                | Expect                                                                                                                                                                                                                    |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Save your resume to OneDrive and open it in Word. **Home › Lineage**. If the button is missing, **Home › Add-ins › Developer Add-ins › Lineage**. | The pane opens. If Word isn't signed in to the same account, it shows **Sign in with Microsoft**. The consent screen asks only for "the application's folder".                                                            |
| 2   | **Start tracking**                                                                                                                                | "Lineage is tracking this file. This is v1." Master panel: **Master** chip, "No tailored versions yet". In OneDrive, **Apps › Lineage** now has a folder with a long id holding `ledger.jsonl` and `blobs`.               |
| 3   | **Make a tailored version**, name it "… – data roles". Repeat for "… – platform roles".                                                           | "Made …" each time, with **Open it**. The panel says "2 tailored versions follow this file", and each row says "Has v1". **Apps › Lineage › Tailored versions** has both files.                                           |
| 4   | In Word, add a job to the master. **Save checkpoint**.                                                                                            | "What changed" lists the added paragraphs, marked **+** and underlined. Save with a note: "Checkpoint saved.", latest checkpoint v2.                                                                                      |
| 5   | **Offer them to both versions**                                                                                                                   | "Offered." The call-out disappears.                                                                                                                                                                                       |
| 6   | **Open** on the data-roles version.                                                                                                               | Word asks to confirm that the add-in may open the file; say **OK**. The version opens in a new window.                                                                                                                    |
| 7   | In that window: **Lineage › Review them**, **Bring in** on each card, then **Apply**.                                                             | "Done. The changes are in your file as tracked changes by Lineage…". In Word, **Review › Track Changes** shows the new job as an insertion by **Lineage**. Accept or reject it in Word as you like.                       |
| 8   | Open the platform-roles version the same way, **Decline** each card, **Apply**.                                                                   | "Done. Nothing was added to your file." The file is unchanged, and the panel shows no updates.                                                                                                                            |
| 9   | In the data-roles window: **Send a copy**, "Who it's for" = a company, leave the Lineage ID on, **Save copy and record it**.                      | "Saved '… (Company).docx' … in Sent copies." **Apps › Lineage › Sent copies** has it. With no edits since step 7, no new checkpoint is made.                                                                              |
| 10  | **What I've sent** › **Open** on that row.                                                                                                        | A new, unsaved document opens with exactly what was sent: the new job as plain text (no tracked changes), no comments. (Word may show you as its author, because it is a new document; the file in Sent copies has none.) |
| 11  | (Optional) Development link at the bottom of the pane: **Diagnostics › Check OneDrive's conditional writes**.                                     | "refused a stale write when the upload was opened: yes". Note the second answer ("when its bytes arrived after a change") for decision 0009.                                                                              |

## Files that come back (Phase 3)

Continue in the master's window from the run above, or any tracked file.

| #   | Do                                                                                                                                             | Expect                                                                                                                                                                                                                                                                                      |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 12  | **Send a copy** of the master to "Mentor" with the Lineage ID on, and another to "Northgate Analytics" with it off.                            | Two files in **Sent copies**.                                                                                                                                                                                                                                                               |
| 13  | Change the master in Word and **Save checkpoint**.                                                                                             | v3 (or the next number).                                                                                                                                                                                                                                                                    |
| 14  | Copy the Mentor file somewhere else under a new name, open it in Word (**Enable Editing** if Word shows Protected View), and open **Lineage**. | "This came back from Mentor", recognised by its Lineage ID, instead of the master's panel.                                                                                                                                                                                                  |
| 15  | Edit that copy (Track Changes on or off), save it, then **Review their changes**.                                                              | "Lineage kept the file" and **Open "<master>"**.                                                                                                                                                                                                                                            |
| 16  | In the master's window: the call-out "Mentor sent back your copy from …" › **Review**.                                                         | Only the mentor's changes, one at a time, with **Keep** / **Skip**; where both of you changed a paragraph, both sides with **Keep mine** / **Use theirs**.                                                                                                                                  |
| 17  | Decide each one, then **Apply**.                                                                                                               | Kept changes are tracked changes by **Lineage** (Review › Track Changes), skipped ones aren't written, and the latest checkpoint is "Kept N changes from Mentor". **What I've sent** says "Reviewed".                                                                                       |
| 18  | Edit the Northgate copy (no ids) and save it. In the master: **A file came back** › **Choose file**, or drop the file on the pane.             | Without ids, Lineage finds it by its content: "This came back from Northgate Analytics", or "Which copy is this?" if more than one copy fits. Nothing is kept until you pick.                                                                                                               |
| 19  | (Optional) Open the Northgate copy itself in Word with Lineage.                                                                                | The Start tracking screen first says "This looks like a copy that came back".                                                                                                                                                                                                               |
| 20  | Edit another sent copy in LibreOffice (or Google Docs, then **Download › .docx**), bring it back with **A file came back** and **Review**.     | The review says "This file was saved in LibreOffice…" (or "…by another app, such as Google Docs…"). With **Bring in their formatting too** unticked, kept words arrive in the master's own fonts and spacing: Review › Track Changes shows insertions and deletions, no formatting changes. |

Afterwards, `cargo test -p lineage-harness --test dev_folder -- --ignored --nocapture` (dev folder only) also checks every returned file and review.

## Word on the web

1. Open your OneDrive in a browser, then the tracked master in Word on the web.
2. **Home › Add-ins › More Add-ins › My Add-ins › Upload My Add-in**, and choose `apps/office-addin/manifest.xml`. The pane loads from `https://localhost:3000`, so the dev server must be running on this computer.
3. **Lineage** should show the master panel from step 2 above, without asking you to start tracking again.
4. Repeat steps 4 to 10 on the web, using a new change in step 4. In step 6 the version opens in a new tab. Note anything that differs from Windows: `insertFileFromBase64` options behave differently on the web.
5. Repeat steps 16 to 18. On the web, **Choose file** opens the browser's file picker; check that dropping a file on the pane works too.

## Afterwards

```bash
powershell -ExecutionPolicy Bypass -File scripts/sideload-word.ps1 -Remove
```

```bash
powershell -ExecutionPolicy Bypass -File scripts/dev-cert.ps1 -Remove
```

Tell the session what happened at each step, including the conditional-write answer and anything on the web that differed. Report counts and messages, never your resume's text.
