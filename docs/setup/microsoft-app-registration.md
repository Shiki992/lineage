# Set up sign-in for the Word add-in (development)

Lineage keeps history in your own OneDrive. To let the task pane ask Microsoft for access to it, you register an app with Microsoft Entra ID once, then put its client ID in a local file that is never committed. No client secret is used: the add-in is a single-page app that signs in through Word (nested app authentication).

Checked against Microsoft's docs on 2026-10-15:

- [Enable SSO in an Office add-in with nested app authentication](https://learn.microsoft.com/office/dev/add-ins/develop/enable-nested-app-authentication-in-your-add-in)
- [What is an App Folder](https://learn.microsoft.com/onedrive/developer/rest-api/concepts/special-folders-appfolder)
- For My documents (checked 2026-10-16): [Configure a single-page app](https://learn.microsoft.com/entra/identity-platform/scenario-spa-app-configuration), [MSAL.js initialization](https://github.com/AzureAD/microsoft-authentication-library-for-js/blob/dev/lib/msal-browser/docs/initialization.md)

## 1. Get a directory to register in

Registering apps from a personal Microsoft account outside a directory is no longer supported. If you don't have a Microsoft Entra tenant, create one: a free Azure account, or the Microsoft 365 Developer Program, gives you one. You will still sign in to Lineage with your personal Microsoft account; the tenant only holds the registration.

## 2. Register the app

In the [Azure portal](https://portal.azure.com/), go to **App registrations** → **New registration**:

| Setting                 | Value                                                                                             |
| ----------------------- | ------------------------------------------------------------------------------------------------- |
| Name                    | `Lineage`. OneDrive names the app folder after it, so your history appears in **Apps › Lineage**. |
| Supported account types | **Accounts in any organizational directory and personal Microsoft accounts**                      |
| Redirect URI            | Platform **Single-page application (SPA)**, `brk-multihub://localhost:3000`                       |

Then **Register**, and copy the **Application (client) ID**.

## 3. Add the second redirect

Word on the web signs in through the browser, so it needs the task pane's own page as well. Under **Manage → Authentication → Single-page application**, **Add URI**: `https://localhost:3000/taskpane.html`, then **Save**.

## 3b. My documents (the web app)

My documents (`apps/family-app`, Phase 3B) uses **the same registration**, so it sees the same **Apps › Lineage** folder: OneDrive ties the app folder to the app's id. It signs in with MSAL's standard flow in the browser and returns through a bridge page. Under **Single-page application**, **Add URI**: `http://localhost:3001/redirect.html`. (Signing out of My documents only clears this browser's cache, so there is no sign-out address.)

When My documents is hosted somewhere for real, add that address's `/redirect.html` too. The bridge page must be served with `Cache-Control: no-store` and without a `Cross-Origin-Opener-Policy` header ([redirect bridge](https://github.com/AzureAD/microsoft-authentication-library-for-js/blob/dev/lib/msal-browser/docs/redirect-bridge.md)).

## 4. Permissions

Under **API permissions**, add Microsoft Graph **delegated** permission **Files.ReadWrite.AppFolder**. This is the only permission Lineage uses: it reaches only its own folder in your OneDrive, not your other files.

Only if you want to try the "next to the master" layout (BLUEPRINT open decision 1, decision 0009), add **Files.ReadWrite** as well. That one reaches every file in your OneDrive, which is why it is not the default.

No admin consent is needed for these. You consent the first time you sign in.

## 5. Tell the add-in

Copy `apps/office-addin/.env.example` to `apps/office-addin/.env.local` (git-ignored) and set:

```text
VITE_LINEAGE_CLIENT_ID=<the Application (client) ID>
VITE_LINEAGE_STORAGE=onedrive
```

Restart `pnpm --filter @lineage/office-addin dev`. When the task pane opens, it signs in silently if Word is signed in to the same account, or shows **Sign in with Microsoft**.

For My documents, copy `apps/family-app/.env.example` to `apps/family-app/.env.local` with the same client ID and `VITE_LINEAGE_STORAGE=onedrive`, and run `pnpm --filter @lineage/family-app dev`. To have the task pane's **My documents** link open it, set `VITE_LINEAGE_APP_URL=http://localhost:3001/` in the add-in's `.env.local` (development builds use that address anyway).

Never commit `.env.local`, and never paste the client ID into issues or logs.
