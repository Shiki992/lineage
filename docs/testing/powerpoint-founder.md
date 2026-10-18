# Manual test: the founder journey in PowerPoint

The Phase 4 "done when" check, run by hand: track a master pitch deck, make tailored versions for investors, send one a copy, change a slide in the master, update the versions, and open what an investor was sent. Run it in PowerPoint for Windows and in PowerPoint on the web, with history in your own OneDrive. About 20 minutes.

Use an invented deck, or a copy of a deck you don't mind Lineage writing to. `fixtures/pptx/synthetic/pitch-deck.pptx` is an invented eight-slide deck: slide 6 is Traction, with "$1.2M ARR" and a chart.

## Once, before the first run

The same as for Word ([word-personal.md](word-personal.md)):

1. **Trust the development certificate:**
   ```bash
   powershell -ExecutionPolicy Bypass -File scripts/dev-cert.ps1
   ```
2. **Register the app** with Microsoft Entra ID and put its client ID in `apps/office-addin/.env.local`, with `VITE_LINEAGE_STORAGE=onedrive`. See [docs/setup/microsoft-app-registration.md](../setup/microsoft-app-registration.md).
3. **Sideload** with PowerPoint and Word closed. The one manifest now lists both apps:
   ```bash
   powershell -ExecutionPolicy Bypass -File scripts/sideload-word.ps1
   ```

Start the task pane's server and leave it running:

```bash
pnpm --filter @lineage/office-addin dev
```

Without step 2, history goes to `apps/office-addin/.dev-storage`. To keep a test run apart from your own history, point it at another folder first with `$env:LINEAGE_DEV_STORAGE = "<folder>"`. That is how the run in STATUS (Phase 4) was done. Afterwards, audit the folder with the Rust engine:

```bash
cargo test -p lineage-harness --test dev_folder -- --ignored --nocapture
```

With the dev folder, **Open** on a version can't open the file: PowerPoint add-ins can't open a file on this computer. Open it yourself from the folder's `Tailored versions`.

## PowerPoint for Windows

| #   | Do                                                                                                                                                                                                              | Expect                                                                                                                                                                                                                                                                   |
| --- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | Save the deck to OneDrive and open it in PowerPoint. **Home › Lineage**. If the button is missing, **Home › Add-ins › Developer Add-ins › Lineage** (the button appeared after the first start in the dev run). | The pane opens. If PowerPoint isn't signed in to the same account, it shows **Sign in with Microsoft**.                                                                                                                                                                  |
| 2   | **Start tracking**                                                                                                                                                                                              | "Lineage is tracking this file. This is v1." **Master deck** chip, "No tailored versions yet".                                                                                                                                                                           |
| 3   | **Make a tailored version**: "Pitch deck – Seed fund", then "Pitch deck – Northgate", then "Pitch deck – Pellow Ventures".                                                                                      | "Made “… .pptx”." each time. "3 tailored versions follow this deck"; each row says "Has v1".                                                                                                                                                                             |
| 4   | Open Pellow's version (**Open**, or from OneDrive). In its pane: **Send a copy**, "Who it's for" = Pellow Ventures, leave the Lineage ID on, **Save copy and record it**.                                       | The format is "PowerPoint (.pptx)", and the note says "no comments, hidden slides or document properties. Speaker notes stay." "Saved “Pitch deck – Pellow Ventures (Pellow Ventures).pptx” …". Close the deck (**Don't Save** if PowerPoint asks: nothing was changed). |
| 5   | Open Northgate's version, change "$1.2M" on slide 6 to "$1.4M", save, and close it.                                                                                                                             | (Its own number, so the update meets an overlap.)                                                                                                                                                                                                                        |
| 6   | In the master, change "$1.2M" on slide 6 to "$2.0M". **Check for changes**.                                                                                                                                     | "You just changed Slide 6 · Traction", with "'$1.2M' changed to '$2.0M'" and the figures marked. Three ticked versions; Pellow's says "Last sent …, before this change". **Sent to Pellow Ventures on … · Open what they saw**.                                          |
| 7   | Untick Pellow. **Update 2 versions**.                                                                                                                                                                           | "Updated 1 of 2 versions": Seed fund "Brought in 1 change"; Northgate "You changed the same thing here: 1 change to choose", "Your own edits to this version were saved as a checkpoint first", and both numbers side by side.                                           |
| 8   | On Northgate: **Use the master's**, **Apply my choices**.                                                                                                                                                       | "Updated 1 version", "Brought in 1 change". Open the two versions: slide 6 says "$2.0M ARR" in both, and Pellow's still says "$1.2M ARR".                                                                                                                                |
| 9   | Back in the master: **Open what they saw**.                                                                                                                                                                     | A new, unsaved presentation with exactly what Pellow got: eight slides, "$1.2M ARR".                                                                                                                                                                                     |
| 10  | Open Seed fund's version and leave it open. Change something else in the master, then **Update** with Seed fund ticked.                                                                                         | Seed fund's line: "Not updated: it's open, so Lineage couldn't change it. Close it, then update it again." The other versions are updated. Close Seed fund and update again: "Brought in 1 change".                                                                      |
| 10a | Change the master again and save a checkpoint (**Save checkpoint**). Open Seed fund's version and leave it open. In its pane: **Review them**.                                                                  | "1 update from the master" (or more), the change marked under its slide, and **Bring in 1 change**. (Needs PowerPointApi 1.8; without it the pane says to close the deck and update it from the master.)                                                                 |
| 10b | **Bring in 1 change**.                                                                                                                                                                                          | The slide changes in the open deck within a second or two, and the pane says "Brought in 1 change. Lineage saved a checkpoint of this deck." Save the deck. In the master's pane, Seed fund's row says it has the latest.                                                |
| 10c | In Northgate's open version, change the same number yourself, then change it in the master and save a checkpoint there. In Northgate's pane: **Review them**, **Use the master's**, **Bring in 1 change**.      | "You both changed these" shows both numbers. Afterwards slide 6 has the master's number, and its history has "Your edits before the update" then "Brought in 1 change …". With **Keep mine** instead, nothing in the deck changes and the update isn't offered again.    |
| 11  | (Optional) **My documents** in the pane's footer, then the deck.                                                                                                                                                | Compare any two checkpoints, slide by slide. **Sent copies › Open as sent** downloads the .pptx that left.                                                                                                                                                               |

## PowerPoint on the web

1. Open your OneDrive in a browser, then the tracked master in PowerPoint on the web.
2. **Home › Add-ins › More Add-ins › My Add-ins › Upload My Add-in**, and choose `apps/office-addin/manifest.xml`. The pane loads from `https://localhost:3000`, so the dev server must be running on this computer.
3. **Lineage** should show the master panel from above, without asking you to start tracking again.
4. Repeat steps 3 and 6 to 9 with a new change. **Open** on a version opens it in a new tab. Note what **Open what they saw** does on the web.
5. **Step 10 on OneDrive.** With a version open in PowerPoint on the web (or in PowerPoint for Windows with AutoSave on), update it from the master. Note whether the update is refused ("Not updated: it's open…"), or whether it lands and the open window shows it. OneDrive may let co-authoring windows share the file instead of locking it. Decision 0018 needs the answer.

## Afterwards

```bash
powershell -ExecutionPolicy Bypass -File scripts/sideload-word.ps1 -Remove
```

```bash
powershell -ExecutionPolicy Bypass -File scripts/dev-cert.ps1 -Remove
```

Tell the session what happened at each step, including step 5 on the web and anything that differed from Windows. Report counts and messages, never your deck's text.
