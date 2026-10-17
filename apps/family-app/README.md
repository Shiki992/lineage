# My documents (family app)

A static web app: a signed-in person sees every document family they track with Lineage, opens one, and sees all its versions, every copy they sent, and the family tree (Phase 3B, decision 0015). It reads only from the person's own storage, in their browser. There are no Lineage accounts and no Lineage server.

- `src/account/`: who is signed in. `AccountProvider` is all the screens know, so Google (Phase 5) is one more provider. `microsoft.ts` uses MSAL's standard public client with the redirect flow and the app folder permission; `dev-folder.ts` is for development builds.
- `src/app/`: the screens. `library.tsx` (My documents), `document.tsx` (Versions, Sent copies, Family tree), `tree.tsx` (layout and keyboard), over the shell's `Library`.
- `redirect.html`: MSAL v5's redirect bridge.
- `e2e/`: Playwright runs against accounts made by `cargo xtask accounts`.

```
pnpm --filter @lineage/family-app dev     # http://localhost:3001, the Word pane's development folder
pnpm --filter @lineage/family-app test    # component tests
pnpm --filter @lineage/family-app e2e     # Playwright, on the installed Edge
```

The Word task pane's footer has a **My documents** link to it (`VITE_LINEAGE_APP_URL`). The manual script is `docs/testing/my-documents.md`.
