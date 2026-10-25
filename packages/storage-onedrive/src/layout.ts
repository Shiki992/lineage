// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Where in OneDrive a family's history and files go: BLUEPRINT open decision 1,
// prototyped both ways (decision 0009).
//
// App folder (recommended): everything in Apps/Lineage, the folder OneDrive
// gives an app with the Files.ReadWrite.AppFolder permission. The master can be
// anywhere, even on this computer, because Lineage reads it through Word, not
// through OneDrive.
//   Apps/Lineage/<familyId>/ledger.jsonl and blobs/<sha256>
//   Apps/Lineage/Tailored versions/<name>.docx
//   Apps/Lineage/Sent copies/<name>.docx
// https://learn.microsoft.com/onedrive/developer/rest-api/concepts/special-folders-appfolder
//
// Next to the master: tailored versions sit beside the master in its own
// folder. That needs Files.ReadWrite (every file the person has), and works
// only for a master saved in OneDrive.
//   <master's folder>/<name>.docx
//   <master's folder>/Sent copies/<name>.docx
//   <master's folder>/Lineage/<familyId>/…

import type { FileKind } from "@lineage/shell";
import { StorageError, folderFor } from "@lineage/shell";
import type { Graph } from "./graph.js";
import { segment } from "./graph.js";
import { itemPath } from "./locate.js";

export interface Layout {
  readonly kind: "appFolder" | "nextToMaster";
  /** The narrowest delegated Graph permission this layout works with. */
  readonly scopes: readonly string[];
  /** The Graph path everything else is relative to. */
  root(graph: Graph): Promise<string>;
  /** The folder holding each family's history, relative to the root ("" is the root). */
  readonly history: string;
  /** The folder for each kind of file, relative to the root ("" is the root). */
  folder(kind: FileKind): string;
}

export const appFolder: Layout = {
  kind: "appFolder",
  scopes: ["Files.ReadWrite.AppFolder"],
  root: () => Promise.resolve("/me/drive/special/approot"),
  history: "",
  folder: folderFor,
};

interface Parent {
  parentReference?: { driveId?: string; id?: string };
}

/**
 * The master's folder, from the address Word reports for it (locate.ts).
 * https://learn.microsoft.com/graph/api/shares-get
 */
async function parentFolder(graph: Graph, masterUrl: string): Promise<string> {
  const path = itemPath(masterUrl);
  if (!path) {
    throw notInOneDrive(masterUrl);
  }
  const item = await graph.json<Parent>("GET", `${path}?$select=parentReference`);
  const drive = item.parentReference?.driveId;
  const id = item.parentReference?.id;
  if (!drive || !id) {
    throw notInOneDrive(masterUrl);
  }
  return `/drives/${segment(drive)}/items/${segment(id)}`;
}

function notInOneDrive(url: string): StorageError {
  return new StorageError(
    "To keep tailored versions next to it, the master has to be saved in your OneDrive.",
    `not a OneDrive URL: ${url.slice(0, 40)}`,
  );
}

export function nextToMaster(masterUrl: string): Layout {
  let resolved: Promise<string> | undefined;
  return {
    kind: "nextToMaster",
    scopes: ["Files.ReadWrite"],
    root: (graph) => (resolved ??= parentFolder(graph, masterUrl)),
    history: "Lineage",
    folder: (kind) => (kind === "version" ? "" : folderFor(kind)),
  };
}
