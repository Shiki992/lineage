# Office add-in

The Lineage task pane for Word: React, designed at 360 px, running the engine as WebAssembly through `@lineage/shell`. PowerPoint (Phase 4) and Excel (Phase 6) come later.

- `manifest.xml`: the development manifest (https://localhost:3000), with a Lineage button on the Home tab. Decision 0008.
- `src/app/`: the personal screens.
  - the master panel and a tailored version's panel;
  - updates from the master, with bring in or decline, both sides of overlaps and by-hand reasons;
  - save checkpoint, make a tailored version, send a copy, and what I've sent;
  - files that come back: choose or drop the file, "Which copy is this?", and the review one change at a time with Keep / Skip (decision 0012).

  Labels come from the engine, in the glossary's words.

- `src/office/word-host.ts`: the open document through Office.js. Whole-file reads and writes, ids in custom properties, and every write read back and checked.
- `src/auth/sign-in.ts`: sign-in through Word (nested app authentication), with the narrowest Graph permission the layout needs. Decisions 0008 and 0009.
- `src/storage/`: OneDrive, or in development a local folder (`@lineage/storage-local`, also used by My documents).
- `src/dev/Diagnostics.tsx` (development only): the ids check, the write check over the engine's Word smoke-test files, and OneDrive's conditional-write check.

## Run it in Word for Windows

Once: trust the development certificate, which you run yourself because it changes your trusted roots:

```bash
powershell -ExecutionPolicy Bypass -File scripts/dev-cert.ps1
```

Then sideload it (Word closed) and start the server:

```bash
powershell -ExecutionPolicy Bypass -File scripts/sideload-word.ps1
```

```bash
pnpm --filter @lineage/office-addin dev
```

Open a document and choose **Home › Lineage**. History goes to `.dev-storage/` (git-ignored) unless `.env.local` sets `VITE_LINEAGE_STORAGE=onedrive` and a client ID ([setup](../../docs/setup/microsoft-app-registration.md)). The full manual check is [docs/testing/word-personal.md](../../docs/testing/word-personal.md).

## Tests and budgets

```bash
pnpm --filter @lineage/office-addin test
```

```bash
pnpm budgets
```

- `src/app/screens.test.tsx`: every screen drawn from the engine's golden replies, found by role and accessible name.
- `src/app/returned.test.tsx`: the screens for files that come back.
- `src/app/App.test.tsx`: the whole flow in jsdom with the real wasm engine, memory storage and a fake Word, including a mentor's copy coming back.
- `src/styles/tokens.test.ts`: every text colour pair reaches 4.5:1.
- `pnpm budgets` builds and fails over 1.5 MB raw or 420 KB gzipped wasm, or 250 KB gzipped JavaScript (decision 0010).
