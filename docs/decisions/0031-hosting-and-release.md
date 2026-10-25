# 0031: Hosting the two sites, and what a release build must pass

Date: 2026-10-21. Status: accepted. The first staging deploy, and the live headers, are the maintainer's to check.

## Context

Phase 5C makes Lineage ready to host and to list on Microsoft Marketplace. Until now both apps ran only from their dev servers, `https://localhost:3000` (the task pane) and `http://localhost:3001` (My documents), with a development manifest. CLAUDE.md says the project runs no servers, keeps history in the person's own storage, and asks for least privilege. BLUEPRINT §7 already expects the family app to be a static web app and one public listing per store.

The maintainer chose (2026-10-21): Azure Static Web Apps, Marketplace preparation, and Word, PowerPoint and Excel in the first listing.

Checked on 2026-10-21: [SWA configuration](https://learn.microsoft.com/azure/static-web-apps/configuration) (20 KB file, routes matched in order, route headers override `globalHeaders`), [SWA quotas](https://learn.microsoft.com/azure/static-web-apps/quotas), [publishing add-ins](https://learn.microsoft.com/office/dev/add-ins/publish/publish) (images must be cacheable; support URL, privacy and terms for Marketplace), [HighResolutionIconUrl](https://learn.microsoft.com/javascript/api/manifest/highresolutioniconurl) (64 × 64 for task panes, required for Marketplace), and [NAA](https://learn.microsoft.com/office/dev/add-ins/develop/enable-nested-app-authentication-in-your-add-in) (`brk-multihub://<host>` with no path, plus the task pane page for Office on the web).

## Decision

### Two Static Web Apps, Free plan, served files only

- The task pane and My documents each get a site and an origin of their own. They have different policies (the pane must be framed by Office; My documents never is), their MSAL caches stay apart, and a release build refuses one origin for both.
- The sites serve the built files and nothing else: no Functions, no Azure sign-in, no logging turned on. Documents, history and tokens go from the person's browser straight to Microsoft. So "no servers run by the project" still holds: the project hosts program files, as an app store would.
- Staging is the sites' named environment `staging`, with a stable address, so it can have redirect URIs. Production is the sites themselves, under custom domains if the maintainer adds them.

### One policy, sent twice

`@lineage/hosting` holds each site's Content Security Policy. Vite puts it in a meta tag (dev server and build), and the build writes `staticwebapp.config.json` with the same policy as a header. A header is the only way `frame-ancestors` works, and it also covers the Excel engine's module worker, which gets its policy from its own response. That worker needs `'wasm-unsafe-eval'`, and the policy has it.

- **My documents** adds `frame-ancestors 'none'` and `X-Frame-Options: DENY`.
- **The task pane has no `frame-ancestors`.** Office on the web, Teams and the desktop apps frame the pane from origins Microsoft doesn't list, and an origin left out would blank the pane there. Lineage's irreversible actions ask for confirmation in the pane.
- Both send HSTS without `includeSubDomains` (the domain's other hosts aren't ours), `nosniff`, `Referrer-Policy: no-referrer` and a minimal `Permissions-Policy`.
- **Caching:** pages are `no-cache`, so a release reaches people at once; hashed files are immutable; ribbon icons are cached for a day, since Microsoft requires them to be cacheable; MSAL's bridge is `no-store`, with no COOP header.
- **The task pane's `connect-src` gains `*.microsoftpersonalcontent.com`.** My documents already had it: personal OneDrive can hand out download URLs on that host, and the pane fetches blobs the same way.

### Manifests from one template

`apps/office-addin/manifest.template.xml` makes three manifests, each with its own add-in id so all three can be installed side by side:

- development: the committed `manifest.xml`, with a drift test;
- staging: "Lineage (staging)";
- production: "Lineage".

A release manifest's version is the tag plus the run number (`1.4.0.27`), so each deploy is newer. Its support and learn-more links go to My documents' help page.

### What a release build must pass

The Vite plugin checks the finished build, and fails it:

- **Every build:** no development code. The dev servers' addresses, or the dev-folder storage's route or folder, in any text file fail it. This found the dev-folder client and Diagnostics in the production task pane, because the Diagnostics screen wasn't guarded by `import.meta.env.DEV`. It is now, and the pane's JavaScript is 1 KB smaller.
- **A release build** (`LINEAGE_RELEASE=1`):
  - every setting is present and well formed: channel, version, build number, two HTTPS origins with no path, the client ID, and My documents' link;
  - the manifest names only the release's origins (and Microsoft sign-in);
  - it gives all three hosts, a four-number version and a name that isn't a development one;
  - every icon it names is in the build.

### Microsoft's manifest validator stays a manual step

`office-addin-manifest` depends on `office-addin-usage-data`, which reports usage to Microsoft unless it is turned off. Lineage sends nothing anywhere, so CI doesn't run it. The release checklist (`docs/release/marketplace.md`) runs it by hand, pinned (3.0.1), after turning usage data off. Partner Center validates the manifest again on submission.

### Deploying

`deploy.yml` calls the whole of `ci.yml` first, then builds for release and uploads both sites with `Azure/static-web-apps-deploy` (pinned by commit; it runs Microsoft's own container image, which isn't pinned by digest). On a tag it attaches `manifest.xml` to the GitHub release. The settings come from GitHub Environments, and production needs a reviewer. Both sites always deploy together: a newer task pane can write ledger records an older My documents can't read.

### Failing safely

Both apps catch a screen that throws while it draws, and say "Lineage stopped working" with a reload button. The details (version, Office host, the error and a few stack lines) are shown first, and copied only when the person asks. Nothing is sent.

### Marketplace pages

`privacy.html`, `terms.html` and `support.html` on My documents' site, in plain language and the design system. The manifest's support URL and the listing point at them. They are drafts the maintainer must review; they aren't legal advice.

## Alternatives

- **GitHub Pages:** free, but no custom headers. The bridge couldn't be `no-store`, and the policy would be a meta tag only.
- **Cloudflare Pages:** custom headers through `_headers`, but a second vendor account beside the Azure tenant the app registration already needs.
- **One site for both apps:** one origin, so MSAL's caches would share storage, and the per-route headers would have to keep two policies apart.
- **The unified JSON manifest:** Word, PowerPoint and Excel are fine with the add-in-only manifest, which also keeps Centralized Deployment open (decision 0008).
- **Running the validator in CI:** see above.

## Consequences

- CI runs twice for a push to `main`: once on its own, and once inside the deploy.
- Rolling back means deploying an earlier tag; its manifest version is lower, so Office keeps the newer manifest, but the files go back.
- Adding a host, permission or command to the template changes the manifest. Centrally deployed copies may need an admin to accept it.
- The legal pages need the maintainer's review before the first submission, and again whenever a permission changes.
