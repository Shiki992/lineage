# Host Lineage (staging and production)

Lineage is two static sites: the **task pane** (the add-in Word, PowerPoint and Excel load) and **My documents**. Both are served by Azure Static Web Apps on its Free plan. They only serve files: documents, history and sign-in tokens never pass through them (decision 0031). The deploy workflow (`.github/workflows/deploy.yml`) builds and uploads both:

- **a push to `main`** goes to **staging**, a named environment with a stable address of its own;
- **a tag `v1.2.3`** goes to **production**, once the whole of CI has passed and a reviewer has approved.

Checked against Microsoft's docs on 2026-10-21: [configuration](https://learn.microsoft.com/azure/static-web-apps/configuration), [quotas](https://learn.microsoft.com/azure/static-web-apps/quotas), [named preview environments](https://learn.microsoft.com/azure/static-web-apps/named-environments), [custom domains](https://learn.microsoft.com/azure/static-web-apps/custom-domain), [nested app authentication](https://learn.microsoft.com/office/dev/add-ins/develop/enable-nested-app-authentication-in-your-add-in).

## 1. Create the two sites

In the [Azure portal](https://portal.azure.com/), **Create a resource → Static Web App**, twice:

| Setting       | Task pane                                                                        | My documents        |
| ------------- | -------------------------------------------------------------------------------- | ------------------- |
| Name          | `lineage-addin`                                                                  | `lineage-app`       |
| Plan          | **Free**                                                                         | **Free**            |
| Deployment    | **Other** (the workflow uploads a finished build; Azure never builds the source) | **Other**           |
| Custom domain | optional, for example `addin.<your domain>`                                      | optional, `app.<…>` |

The two need **origins of their own**: the release build refuses one origin for both. Without custom domains, each site's address is `https://<generated-name>.<region>.azurestaticapps.net`. Staging is the same site's `staging` environment, at `https://<generated-name>-staging.<region>.azurestaticapps.net` once it is first deployed. Custom domains apply to production only.

For each site, **Overview → Manage deployment token**, and copy the token.

Free-plan limits that matter here: 250 MB per app (a build is about 3 MB), 2 custom domains per app, 3 preview environments (staging uses one), 100 GB of bandwidth a month across the subscription.

## 2. Register sign-in for the hosted sites

Use one app registration for staging and production, owned by the project rather than a person, named `Lineage`: OneDrive names everyone's history folder after it (**Apps › Lineage**), and ties that folder to the registration. Follow [microsoft-app-registration.md](microsoft-app-registration.md), then under **Authentication → Single-page application** add, for each of staging and production:

```text
brk-multihub://<task pane origin's host>
<task pane origin>/taskpane.html
<My documents origin>/redirect.html
```

For example `brk-multihub://addin.example.org`, `https://addin.example.org/taskpane.html` and `https://app.example.org/redirect.html`. The `brk-multihub` address takes the host only, with no path.

For the Marketplace listing, verify the publisher (**Branding & properties → Publisher verification**, with the Partner Center account). Work and school accounts can't consent to an unverified multi-tenant app in many organisations.

## 3. Set up the GitHub Environments

In the repository, **Settings → Environments**, create `staging` and `production`. For `production`, tick **Required reviewers** and add yourself, and under **Deployment branches and tags** allow only tags matching `v*.*.*`.

In each environment:

| Kind     | Name                    | Value                                                |
| -------- | ----------------------- | ---------------------------------------------------- |
| Variable | `LINEAGE_ADDIN_ORIGIN`  | the task pane's origin for this environment, no path |
| Variable | `LINEAGE_APP_ORIGIN`    | My documents' origin for this environment, no path   |
| Variable | `LINEAGE_CLIENT_ID`     | the app registration's client ID                     |
| Secret   | `AZURE_SWA_TOKEN_ADDIN` | the task pane site's deployment token                |
| Secret   | `AZURE_SWA_TOKEN_APP`   | My documents' deployment token                       |

Staging and production use the same two tokens, since staging is an environment of the same sites. Nothing here is committed. The client ID isn't secret (it is in every page that signs in), but it stays out of the repository as CLAUDE.md asks.

## 4. Deploy

- **Staging:** merge to `main`, or run **Deploy** by hand on a branch. The add-in is called **Lineage (staging)** and has an id of its own, so it can be installed beside production.
- **Production:** push a tag, `git tag -s v0.6.0 && git push origin v0.6.0`, then approve the run. The release on GitHub gets the production `manifest.xml`.

The build stops before anything is uploaded if:

- a setting above is missing or malformed;
- the manifest names a host the release doesn't serve, an icon the build lacks, or a development name;
- development code (the dev servers' addresses, or the dev-folder storage) is in either build.

The version comes from the tag (`v0.6.0` → `0.6.0`). The manifest's fourth number is the run number, so every deploy is newer than the last.

## 5. Check a deploy

```bash
curl -sI https://<task pane origin>/taskpane.html
```

Expect `content-security-policy`, `strict-transport-security`, `x-content-type-options: nosniff`, `referrer-policy: no-referrer` and `cache-control: no-cache`. Then:

- `/assets/icon-32.png` must be cacheable (`max-age=86400`);
- the `.wasm` file must be `application/wasm`;
- on My documents, `/redirect.html` must be `no-store`, with no `cross-origin-opener-policy`.

`@lineage/hosting` writes all of these into `staticwebapp.config.json` in each build.

## 6. How updates reach people

- Pages are `no-cache` and every script is hashed, so a deploy reaches everyone the next time they open the pane.
- Office caches the manifest. A manifest change (a new permission, host or command) reaches Marketplace users when Partner Center publishes it; organisations that deployed Lineage centrally may need their admin to accept it.
- **Rolling back:** run **Deploy** by hand on the previous tag. Both sites go back together.

Deploy both sites together, always. A newer task pane can write ledger records an older My documents can't read.
