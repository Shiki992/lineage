# 0009: Where tailored versions and history live in OneDrive

Date: 2026-10-15. Status: accepted, pending the maintainer's run against a real OneDrive.

## Context

BLUEPRINT open decision 1: should a personal user's tailored versions sit next to the master (more visible, broader permission), or inside the app's own folder (least privilege, less visible)? CLAUDE.md asks for the narrowest storage permission that works, and prefers the OneDrive app folder where it is enough. Phase 2 prototyped both in `packages/storage-onedrive/src/layout.ts`. Docs checked 2026-10-15:

- [App folder](https://learn.microsoft.com/onedrive/developer/rest-api/concepts/special-folders-appfolder)
- [Special folders](https://learn.microsoft.com/graph/api/drive-get-specialfolder)
- [createUploadSession](https://learn.microsoft.com/graph/api/driveitem-createuploadsession)
- [Download content](https://learn.microsoft.com/graph/api/driveitem-get-content)

## The two layouts

|                         | App folder                                                                                                          | Next to the master                                                                                                  |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------- |
| Permission              | `Files.ReadWrite.AppFolder` (valid for personal accounts): only `Apps/Lineage`                                      | `Files.ReadWrite`: every file in the person's OneDrive                                                              |
| History                 | `Apps/Lineage/<familyId>/ledger.jsonl`, `…/blobs/<sha256>` (BLUEPRINT §4's layout, with the app folder as the root) | `<master's folder>/Lineage/<familyId>/…`                                                                            |
| Tailored versions       | `Apps/Lineage/Tailored versions/<name>.docx`                                                                        | `<master's folder>/<name>.docx`                                                                                     |
| Sent copies             | `Apps/Lineage/Sent copies/<name>.docx`                                                                              | `<master's folder>/Sent copies/<name>.docx`                                                                         |
| Where the master can be | Anywhere, even on this computer: Lineage reads it through Word, never through OneDrive                              | Only in OneDrive. Lineage finds its folder from the URL Word reports (`d.docs.live.net/<cid>/…`, or the shares API) |
| What the person sees    | One folder for everything Lineage made; versions are a step away from the master                                    | Versions beside the master, as people already keep them                                                             |
| Consent screen          | "Have full access to the application's folder"                                                                      | "Have full access to all files you can access"                                                                      |

Both pass the shell's storage conformance suite against a fake Graph that answers the way the docs describe: path addressing, eTags, `If-Match` → 412, `conflictBehavior: fail` → 409, 429 with `Retry-After`, expiring download URLs, and no token on upload URLs. The "next to the master" prototype also lists the master and any other `.docx` in its folder as names taken, which is correct for avoiding clashes.

## Decision

**The app folder is the default**, and the only layout a production build uses. "Next to the master" stays as a development option (`VITE_LINEAGE_LAYOUT=nextToMaster`) for the maintainer's comparison run, and is not offered to users.

- It is the narrowest permission that works, as CLAUDE.md asks; the other needs access to every file.
- It works wherever the master is: a local file, OneDrive, or a folder Lineage could never reach.
- To soften "less visible", the panel's **Open** buttons open versions directly, and the "Send a copy" result says where the copy went.

How the adapter uses Graph, the same for both layouts:

- **Ledger:** read the metadata first (eTag and download URL), then the bytes. An append rewrites the ledger through an upload session, with `If-Match` set to the eTag it was read at. A ledger changed since gives 412, which becomes a conflict and a re-run. A new ledger is created with `conflictBehavior: fail`.
- **Downloads:** always through `@microsoft.graph.downloadUrl`, because the `/content` redirect can't be followed under CORS. An expired URL is replaced once.
- **Blobs:** one listing per family per session says which already exist, and new ones are uploaded with a simple PUT.

## Alternatives

- **Ask for `Files.ReadWrite` only when someone wants versions beside the master** (incremental consent). Possible later, if people ask; the prototype shows it works.
- **Keep history in the app folder and versions beside the master.** It still needs `Files.ReadWrite`, so it has the same cost as "next to the master".

## Consequences

- A version's storage ref is its path inside the layout's folder. Renaming or moving it in OneDrive loses the link until returned-file matching (Phase 3) can find it by its ids.
- Whether OneDrive honours `If-Match` at the end of an upload session, as well as when it is created, is not documented. The maintainer's run (`docs/testing/word-personal.md`, step 9) checks a real conflict.
- The app folder's name is localised (`Apps` in English); the adapter addresses it through the `approot` alias, never by name.
