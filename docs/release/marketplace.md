# Release to Microsoft Marketplace

The checklist for submitting Lineage, after a production deploy ([hosting.md](../setup/hosting.md)). Microsoft's rules: [publish to Microsoft Marketplace](https://learn.microsoft.com/office/dev/add-ins/publish/publish-office-add-ins-to-appsource), the [commercial marketplace certification policies](https://learn.microsoft.com/legal/marketplace/certification-policies) (section 1140 for Office add-ins), and [effective listings](https://learn.microsoft.com/partner-center/marketplace-offers/create-effective-office-store-listings). Checked 2026-10-21; re-read them before each submission.

## Before the first submission

- [ ] A Partner Center account enrolled in the Microsoft 365 and Copilot program, owned by the project.
- [ ] Publisher verification on the app registration ([hosting.md](../setup/hosting.md), step 2).
- [ ] The maintainer has read `privacy.html`, `terms.html` and `support.html` on the production site, and is happy to stand behind them. They are plain-language drafts, not legal advice.

## Each release

1. **Tag and deploy** `vX.Y.Z`; approve the production run. Download `manifest.xml` from the GitHub release.
2. **Check the manifest** with Microsoft's validator, by hand. It is a Microsoft package that sends usage data unless told not to, so turn that off first:

   ```bash
   pnpm dlx office-addin-usage-data@3.0.1 off
   ```

   ```bash
   pnpm dlx office-addin-manifest@3.0.1 validate manifest.xml
   ```

   The build's own checks (`@lineage/hosting`) already refuse localhost, stray hosts, missing icons and development names. This adds Microsoft's schema check.

3. **Try it on every platform it runs on.** Microsoft's validators do, and an add-in must work everywhere its hosts run ([1140.3](https://learn.microsoft.com/legal/marketplace/certification-policies#11403-functionality)). With the production manifest added by **Upload My Add-in**, run the founder, job-seeker and pilot journeys (`docs/testing/`) in:

   | App        | Windows | Mac | On the web | iPad |
   | ---------- | ------- | --- | ---------- | ---- |
   | Word       | ☐       | ☐   | ☐          | ☐    |
   | PowerPoint | ☐       | ☐   | ☐          | ☐    |
   | Excel      | ☐       | ☐   | ☐          | ☐    |

   On iPad, check at least that the pane opens and says plainly what it can do, or don't offer iPad in Partner Center.

4. **Submit** in Partner Center: **Marketplace offers → New offer → Office add-in**, then upload `manifest.xml`.

## The listing

- **Name:** Lineage
- **Summary** (100 characters): Keep a master and its tailored versions in step, and record every copy you send.
- **Description:**

  > Lineage keeps one master of your resume, pitch deck or workbook, and the versions you tailor from it, in step. Change the master once and choose which versions get the change. Every copy you send is recorded, so you can open exactly what each person saw, and when a copy comes back with their edits, you see only their changes and keep the ones you want.
  >
  > Your history stays in your own OneDrive. Lineage has no servers and no accounts of its own, and it's free and open source.

- **Categories:** Productivity; Document review.
- **Logo:** `docs/release/listing-logo-300.png` (made by `node scripts/make-icons.mjs`).
- **Screenshots** (1366 × 768): the master resume panel, Update N versions on a deck, What changed in a workbook, What I've sent with a shared link, and a file that came back. Use invented documents only.
- **Links:** privacy `https://<My documents>/privacy.html`, terms `…/terms.html`, support `…/support.html`.

## Notes for Microsoft's validators

Paste this into **Notes for certification**:

> Lineage has no accounts of its own. Sign in with any Microsoft account (personal, work or school) when the pane asks. It needs only access to its own OneDrive folder (Files.ReadWrite.AppFolder). In Excel on the web it also asks to read the open workbook (Files.Read), and the first time you share a copy as a link it may ask for Files.ReadWrite.
>
> To try it: open any Word document saved to OneDrive, choose Lineage on the Home tab, sign in, and choose Start tracking. Then Make a tailored version, change the master, and choose Check for changes on the version. Send a copy records what left; Share a link makes a OneDrive link to it. PowerPoint and Excel work the same way. Lineage never runs macros.
