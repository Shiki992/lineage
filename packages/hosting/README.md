# @lineage/hosting

Build time only, never in a page. What makes the two static sites ready to host (decision 0031):

- `src/policy.ts`: each site's Content Security Policy, in one place. The task pane reaches only Office.js, Microsoft sign-in, Graph and OneDrive. My documents also refuses to be framed.
- `src/site.ts`: the Azure Static Web Apps configuration (`staticwebapp.config.json`): the policy as a header, caching (pages `no-cache`, hashed files immutable, ribbon icons cacheable), MSAL's bridge `no-store`, `.wasm` as `application/wasm`, and the 404 page.
- `src/manifest.ts`: the add-in manifests from `apps/office-addin/manifest.template.xml`: development (the committed `manifest.xml`), staging and production, each with its own id. Also the checks a release manifest must pass.
- `src/release.ts`: what a release build is told (`LINEAGE_RELEASE=1` and the rest, from the deploy workflow's GitHub Environment), and the check that it was told all of it.
- `src/dist.ts`: the checks on a finished build. No development code ships; a release has its hosting file and, for the task pane, a manifest that passes.
- `src/plugin.ts`: the Vite plugin both apps use. It adds the meta tag, writes the files above into a build, and fails the build when a check does.

After changing the manifest template, run `pnpm --filter @lineage/hosting manifest` (Node runs `src/cli.ts` directly), or the drift test fails. The sources import each other with `.ts` extensions for that reason.

[docs/setup/hosting.md](../../docs/setup/hosting.md) says how to set up the sites and deploy them.
