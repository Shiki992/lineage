# @lineage/storage-onedrive

The person's OneDrive behind the storage interface in `@lineage/shell`, which mirrors the harness's `Storage` trait. Microsoft Graph calls only.

- `src/graph.ts`: bearer tokens (refreshed once after a 401), retries with `Retry-After` when Graph is busy, and errors in the panel's words.
- `src/onedrive-storage.ts`:
  - **Ledger appends:** through an upload session with `If-Match` set to the eTag the ledger was read at, so a ledger changed elsewhere is a conflict and the command runs again.
  - **Downloads:** through `@microsoft.graph.downloadUrl`, since the `/content` redirect can't be followed under CORS.
  - **Blobs:** one listing per family says which exist.
- `src/layout.ts`: where things go (decision 0009).
  - **App folder**, the default: `Files.ReadWrite.AppFolder`, everything in `Apps/Lineage`.
  - **Next to the master**, a development prototype: `Files.ReadWrite`.
- `src/testing/fake-graph.ts`: an in-memory OneDrive answering the same requests the documented way, including 412, 409, 429 and expiring download URLs.

```bash
pnpm --filter @lineage/storage-onedrive test
```

Both layouts pass the shell's storage conformance suite, plus tests for `If-Match`, conflicts, retries, token refresh, expired download URLs, path encoding and the network being down.
