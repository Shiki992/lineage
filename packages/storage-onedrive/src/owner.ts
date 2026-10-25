// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Whose OneDrive a document is in, from the address Office reports for it
// (`Office.context.document.url`), so the panel can tell a file someone shared
// with you from one whose history is simply missing (decision 0032).
//
// - Personal OneDrive opens in the desktop apps as
//   https://d.docs.live.net/<cid>/<path>, and on the web with the owner's cid
//   in the address (`cid=` or `resid=<CID>!…`). A personal drive's id is that
//   cid.
// - Work and school OneDrive is https://<tenant>-my.sharepoint.com/personal/<user>/…;
//   the person's own app folder's webUrl starts the same way.
// - Anything else (a team site, a local file) can't be placed.
//
// Office's addresses for shared files aren't documented for every app and
// platform; the maintainer's OneDrive run checks them. When in doubt, the
// answer is "can't tell", never a guess.

export type Owner =
  | { readonly kind: "personal"; readonly cid: string }
  | { readonly kind: "business"; readonly root: string };

const CID = /^[0-9a-f]{1,16}$/i;

/** Whose drive `address` is in, or undefined when it can't be told. */
export function ownerOf(address: string): Owner | undefined {
  let url: URL;
  try {
    url = new URL(address);
  } catch {
    return undefined;
  }
  if (url.protocol !== "https:") {
    return undefined;
  }
  const host = url.hostname.toLowerCase();
  if (host === "d.docs.live.net") {
    const cid = url.pathname.split("/")[1] ?? "";
    return CID.test(cid) ? { kind: "personal", cid: cid.toLowerCase() } : undefined;
  }
  if (host === "onedrive.live.com" || host.endsWith(".onedrive.live.com")) {
    const params = url.searchParams;
    const cid = params.get("cid") ?? params.get("resid")?.split("!")[0] ?? "";
    return CID.test(cid) ? { kind: "personal", cid: cid.toLowerCase() } : undefined;
  }
  if (host.endsWith("-my.sharepoint.com")) {
    const [, personal, user] = url.pathname.split("/");
    if (personal?.toLowerCase() === "personal" && user) {
      return { kind: "business", root: `https://${host}/personal/${user.toLowerCase()}` };
    }
  }
  return undefined;
}

/** What the person's own drive is: its id, and its app folder's address. */
export interface OwnDrive {
  readonly driveId?: string | undefined;
  readonly webUrl?: string | undefined;
}

/**
 * Whether `owner` is the person whose drive is `own`: true, false, or
 * undefined when the two can't be compared.
 */
export function isOwn(owner: Owner, own: OwnDrive): boolean | undefined {
  if (owner.kind === "personal") {
    return own.driveId === undefined ? undefined : own.driveId.toLowerCase() === owner.cid;
  }
  if (own.webUrl === undefined) {
    return undefined;
  }
  return own.webUrl.toLowerCase().startsWith(`${owner.root}/`);
}
