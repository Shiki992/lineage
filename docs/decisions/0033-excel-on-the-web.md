# 0033: Excel on the web reads the workbook as last saved

Date: 2026-10-25. Status: accepted. The real behaviour of OneDrive and Excel on the web is the maintainer's run to check.

## Context

Decision 0020 left Excel on the web unsupported. `getFileAsync` gives an add-in only a PDF there, and reading the saved file needs a permission wider than the app folder. The maintainer chose (2026-10-21) to list Word, PowerPoint and Excel on Microsoft Marketplace in the first release. Marketplace validation needs an add-in to work on every platform its hosts run on ([1140.3](https://learn.microsoft.com/legal/marketplace/certification-policies#11403-functionality)), so Excel on the web has to work.

Checked on 2026-10-21 and 25:

- [getFileAsync](<https://learn.microsoft.com/javascript/api/office/office.document#office-office-document-getfileasync-member(1)>): Excel on the web gives PDF only;
- [driveItem](https://learn.microsoft.com/graph/api/resources/driveitem): `cTag` changes with the content, not with metadata alone;
- [shares](https://learn.microsoft.com/graph/api/shares-get): an encoded URL reaches an item. Forum reports say personal accounts can refuse a plain web address ("Invalid shares key"), so personal OneDrive goes by path or by id instead.

## Decision

- **Where the bytes come from.** In Excel on the web (`Office.PlatformType.OfficeOnline`), `ExcelHost.readBytes` reads the workbook as last saved in OneDrive (`savedFile` in `@lineage/storage-onedrive`). The item comes from `Office.context.document.url`:
  - `d.docs.live.net/<cid>/<path>`: by path;
  - `onedrive.live.com/…?resid=<CID>!<n>`: by id;
  - anything else on https: through the shares API.

  The "next to the master" layout now uses the same lookup.

- **Permission.** Reading the person's own workbook needs `Files.Read`, asked for through MSAL the first time it's read, and never at sign-in. In Excel on the web, the panel says beforehand that it reads the workbook as last saved, and that Microsoft will ask once; it reads only the workbook that's open.
- **"Last saved" made safe:**
  - **After Lineage's own writes** (a bring-in's cell steps, or new ids), the next read waits for the saved file's content tag to change from the one last read, so Excel's autosave has carried them. If it hasn't within 30 s, the read fails with "Excel hasn't saved the workbook yet". If that happens while bringing changes in, every step is taken back out (the shell undoes all of them, not none), and nothing is recorded.
  - **After edits in the last 3 s** (from change events), the read waits until the tag holds still, so a comparison doesn't miss what was just typed.
  - Otherwise it reads at once.
- **Unchanged:** cell plans already go through Office.js in chunks inside Excel on the web's 5 MB request limit (decision 0020). Change events, edit hints and co-author hints (decision 0032) work the same on the web.
- **Only for workbooks.** Word and PowerPoint on the web read the open file with `getFileAsync(Compressed)` as before.

## Alternatives

- **Read cells through Office.js.** The 5 MB and 5-million-cell limits, values as shown rather than stored, and no names, tables or styles as the file holds them (decision 0020).
- **Ask for `Files.Read` at sign-in.** Everyone would see the "all your files" screen, including people who never use Excel on the web.
- **Leave Excel out of the first listing**, or list it and fail validation. The maintainer chose all three apps.

## Consequences

- A workbook must be saved in OneDrive to be read on the web; an unsaved one says to save it first.
- A tracked workbook kept in a SharePoint team site needs more than `Files.Read`. It fails with OneDrive's own message until someone asks for that.
- Comparisons on the web can be a few seconds slower right after edits, while the saved file settles.
- **For the maintainer's run:**
  - the addresses Office reports in Excel on the web, for personal and work accounts;
  - how the shares API answers for them;
  - how long autosave takes;
  - whether `Workbook.save` after a bring-in does anything on the web.
