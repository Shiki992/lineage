# @lineage/storage-local

Development and tests only. Lineage's history in a folder on this computer, in the harness's layout (`crates/harness/src/storage.rs`), so the Word task pane and "My documents" can be tried without OneDrive, and the Rust harness can audit what they wrote.

- `src/plugin.ts`: a Vite plugin for the dev server (`apply: "serve"`, so never in a production build). It answers only the origins it is given.
- `src/client.ts`: `LocalFolderStorage`, the shell's `Storage` interface over that server.

The conformance suite in `@lineage/shell/testing` runs against a real server on a temporary folder (`src/plugin.test.ts`).
