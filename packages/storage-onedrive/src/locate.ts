// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// A document Office has open, as a Graph item, from the address Office
// reports for it (`Office.context.document.url`), and that item as last
// saved. Used for the "next to the master" layout (decision 0009) and for
// workbooks open in Excel on the web, which can't give an add-in their bytes
// (decision 0033).
//
// - Personal OneDrive opens as https://d.docs.live.net/<cid>/<path>: the
//   owner's drive by path.
// - On the web it can be https://onedrive.live.com/…?resid=<CID>!<n>: the item
//   by id.
// - Anything else on https goes through the shares API, as an encoded URL.
//   https://learn.microsoft.com/graph/api/shares-get
//
// Office's addresses aren't documented for every app and platform; the
// maintainer's OneDrive run checks these.

import { NotFound, StorageError } from "@lineage/shell";
import type { Graph } from "./graph.js";
import { encodePath, segment } from "./graph.js";

/** Base64url without padding, as the shares API wants. */
export function base64url(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  for (const b of bytes) {
    binary += String.fromCharCode(b);
  }
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

/** The Graph path of the item at `address`, or undefined when it isn't in OneDrive. */
export function itemPath(address: string): string | undefined {
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
    const [, cid, ...rest] = url.pathname.split("/");
    if (!cid || rest.length === 0) {
      return undefined;
    }
    const path = rest.map((p) => decodeURIComponent(p)).join("/");
    return `/drives/${segment(cid)}/root:/${encodePath(path)}`;
  }
  const resid = url.searchParams.get("resid");
  if ((host === "onedrive.live.com" || host.endsWith(".onedrive.live.com")) && resid) {
    const cid = resid.split("!")[0];
    return cid ? `/drives/${segment(cid)}/items/${segment(resid)}` : undefined;
  }
  return `/shares/u!${base64url(address)}/driveItem`;
}

/** The open document as last saved, and its content tag (`cTag`). */
export interface SavedFile {
  read(): Promise<{ bytes: Uint8Array; tag: string }>;
  /** The content tag alone: it changes when the file's content does. */
  tag(): Promise<string>;
}

interface Item {
  cTag?: string;
  eTag?: string;
  "@microsoft.graph.downloadUrl"?: string;
}

function notInOneDrive(): StorageError {
  return new StorageError(
    "Lineage can read this workbook on the web only once it's saved in OneDrive.",
    "the open document isn't a OneDrive address",
  );
}

/**
 * The document at `address`, as last saved in OneDrive. The person needs to
 * be able to read it: `Files.Read` for their own files (decision 0033).
 * https://learn.microsoft.com/graph/api/driveitem-get
 */
export function savedFile(graph: Graph, address: string): SavedFile {
  const path = itemPath(address);
  const meta = async (select: string): Promise<Item> => {
    if (!path) {
      throw notInOneDrive();
    }
    const response = await graph.request("GET", `${path}?$select=${select}`, { allow: [404] });
    if (response.status === 404) {
      throw new NotFound("the open workbook");
    }
    return (await response.json()) as Item;
  };
  const tagOf = (item: Item): string => {
    const tag = item.cTag ?? item.eTag;
    if (!tag) {
      throw new StorageError("OneDrive didn't say which version this is.", "no cTag or eTag");
    }
    return tag;
  };
  return {
    tag: async () => tagOf(await meta("id,cTag,eTag")),
    read: async () => {
      const item = await meta("id,cTag,eTag,@microsoft.graph.downloadUrl");
      const url = item["@microsoft.graph.downloadUrl"];
      if (!url) {
        throw new NotFound("the open workbook");
      }
      const bytes = new Uint8Array(await (await graph.plain("GET", url)).arrayBuffer());
      return { bytes, tag: tagOf(item) };
    },
  };
}
