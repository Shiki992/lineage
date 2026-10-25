// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The person's OneDrive, behind the shell's Storage interface (which mirrors the
// harness's): Microsoft Graph calls only.
//
// - Ledger appends are optimistic. The ledger is rewritten through an upload
//   session with If-Match set to the eTag it was read at, so a ledger changed
//   since gives 412, which becomes StorageConflict and a re-run.
//   https://learn.microsoft.com/graph/api/driveitem-createuploadsession
// - Downloads use @microsoft.graph.downloadUrl, because the /content redirect
//   can't be followed under CORS.
//   https://learn.microsoft.com/graph/api/driveitem-get-content
// - Blobs are content-addressed, so one listing of a family's blobs folder says
//   which already exist and gives their download URLs.
// - The library index (index.json beside the family folders) is written like the
//   ledger, through an upload session with If-Match. Every ledger's eTag is
//   asked for in $batch requests of up to 20, to check the index against.
//   https://learn.microsoft.com/graph/json-batching

import type {
  FileKind,
  FileLocation,
  LedgerVersion,
  MadeLink,
  Sharing,
  Storage,
  StoredFile,
} from "@lineage/shell";
import {
  BadBlob,
  checkFamily,
  checkFileName,
  checkHash,
  FileChanged,
  FileExists,
  NotFound,
  OFFICE_FILE,
  sha256Hex,
  StorageConflict,
  StorageError,
} from "@lineage/shell";
import type { Graph } from "./graph.js";
import { GRAPH, encodePath, segment } from "./graph.js";
import type { Layout } from "./layout.js";
import type { OwnDrive } from "./owner.js";
import { isOwn, ownerOf } from "./owner.js";

interface DriveItem {
  id: string;
  name: string;
  eTag?: string;
  webUrl?: string;
  file?: unknown;
  folder?: unknown;
  "@microsoft.graph.downloadUrl"?: string;
}

interface Page {
  value: DriveItem[];
  "@odata.nextLink"?: string;
}

const ID = /^[0-9a-f]{32}$/;
const EMPTY = new Uint8Array();
/** Graph takes at most 20 requests in one $batch. */
const BATCH = 20;

interface BatchResponse {
  responses?: { id?: string; status?: number; body?: { eTag?: string } }[];
}

function join(...parts: string[]): string {
  return parts.filter((p) => p !== "").join("/");
}

function concat(a: Uint8Array, b: Uint8Array): Uint8Array {
  const out = new Uint8Array(a.length + b.length);
  out.set(a, 0);
  out.set(b, a.length);
  return out;
}

export interface OneDriveOptions {
  /**
   * A Graph client with the wider Files.ReadWrite permission, for sharing a
   * copy as a link when OneDrive refuses with the app-folder one alone
   * (decision 0032). Asked for only then, so most people never see it.
   */
  readonly wider?: () => Promise<Graph>;
}

interface CreatedLink {
  id?: string;
  link?: { webUrl?: string; scope?: string };
}

export class OneDriveStorage implements Storage {
  readonly label = "OneDrive";
  private rootPath: Promise<string> | undefined;
  /** The ledger last read or written, per family, so an append needs no re-read. */
  private readonly ledgers = new Map<string, { eTag: string; bytes: Uint8Array }>();
  /** Per family: blob hash → download URL (undefined once uploaded here). */
  private readonly blobs = new Map<string, Promise<Map<string, string | undefined>>>();
  /** The person's own drive, asked for once. */
  private own: Promise<OwnDrive | undefined> | undefined;

  constructor(
    private readonly graph: Graph,
    readonly layout: Layout,
    private readonly options: OneDriveOptions = {},
  ) {}

  /**
   * Sharing links on files in Lineage's folder (decision 0032). Graph lists
   * Files.ReadWrite for these, not the app-folder permission, so each is tried
   * with the narrower one first, and with `wider` only when OneDrive says 403.
   * https://learn.microsoft.com/graph/api/driveitem-createlink
   * https://learn.microsoft.com/graph/api/permission-delete
   */
  readonly sharing: Sharing = {
    createLink: (ref, access) => this.createLink(ref, access),
    stopLink: (ref, link) => this.stopLink(ref, link),
  };

  private async narrowThenWide(call: (graph: Graph) => Promise<Response>): Promise<Response> {
    const first = await call(this.graph);
    if (first.status !== 403 || !this.options.wider) {
      return first;
    }
    return call(await this.options.wider());
  }

  private async createLink(ref: string, access: "view" | "edit"): Promise<MadeLink> {
    // Personal OneDrive makes "anyone with the link" links; a work account
    // gets its organisation's default kind, whatever that is.
    const work = (await this.ownDrive())?.webUrl?.includes(".sharepoint.com") ?? false;
    const path = await this.sub(ref, "createLink");
    const body = JSON.stringify({ type: access, ...(work ? {} : { scope: "anonymous" }) });
    const response = await this.narrowThenWide((graph) =>
      graph.request("POST", path, {
        headers: { "Content-Type": "application/json" },
        body,
        allow: [403],
      }),
    );
    if (response.status === 403) {
      throw new StorageError(
        "OneDrive wouldn't make a link for this copy. Your account or organisation may not allow sharing links.",
        "createLink: 403",
      );
    }
    const made = (await response.json()) as CreatedLink;
    const url = made.link?.webUrl;
    if (!made.id || !url?.startsWith("https://")) {
      throw new StorageError("OneDrive didn't make a link. Try again.", "createLink: no link");
    }
    return { id: made.id, url, scope: made.link?.scope ?? (work ? "organization" : "anonymous") };
  }

  private async stopLink(ref: string, link: string): Promise<void> {
    const path = await this.sub(ref, `permissions/${segment(link)}`);
    const response = await this.narrowThenWide((graph) =>
      graph.request("DELETE", path, { allow: [403, 404] }),
    );
    // 404: already gone, which is what was wanted.
    if (response.status === 403) {
      throw new StorageError(
        "OneDrive wouldn't remove that link. Remove it in OneDrive under Share › Manage access.",
        "permission delete: 403",
      );
    }
  }

  private root(): Promise<string> {
    return (this.rootPath ??= this.layout.root(this.graph));
  }

  /** The Graph path of an item, by its path relative to the root. */
  private async item(rel: string): Promise<string> {
    const root = await this.root();
    return rel === "" ? root : `${root}:/${encodePath(rel)}`;
  }

  /** The Graph path of an item's sub-resource, such as `content` or `children`. */
  private async sub(rel: string, resource: string): Promise<string> {
    const root = await this.root();
    return rel === "" ? `${root}/${resource}` : `${root}:/${encodePath(rel)}:/${resource}`;
  }

  private history(family: string, ...rest: string[]): string {
    return join(this.layout.history, checkFamily(family), ...rest);
  }

  /** An item's metadata, or undefined when it doesn't exist. */
  private async meta(rel: string, select: string): Promise<DriveItem | undefined> {
    const response = await this.graph.request("GET", `${await this.item(rel)}?$select=${select}`, {
      allow: [404],
    });
    return response.status === 404 ? undefined : ((await response.json()) as DriveItem);
  }

  private async download(rel: string, item: DriveItem | undefined): Promise<Uint8Array> {
    const url =
      item?.["@microsoft.graph.downloadUrl"] ??
      (await this.meta(rel, "id,@microsoft.graph.downloadUrl"))?.["@microsoft.graph.downloadUrl"];
    if (!url) {
      throw new NotFound(rel);
    }
    try {
      return new Uint8Array(await (await this.graph.plain("GET", url)).arrayBuffer());
    } catch (error) {
      if (!item) {
        throw error;
      }
      // Download URLs expire within minutes: ask for a fresh one, once.
      return this.download(rel, undefined);
    }
  }

  private async children(rel: string): Promise<DriveItem[]> {
    const first = await this.graph.request(
      "GET",
      `${await this.sub(rel, "children")}?$select=id,name,file,folder,@microsoft.graph.downloadUrl&$top=200`,
      { allow: [404] },
    );
    if (first.status === 404) {
      return [];
    }
    const items: DriveItem[] = [];
    let page = (await first.json()) as Page;
    for (;;) {
      items.push(...page.value);
      const next = page["@odata.nextLink"];
      if (!next?.startsWith(GRAPH)) {
        return items;
      }
      page = await this.graph.json<Page>("GET", next.slice(GRAPH.length));
    }
  }

  /** Uploads bytes through an upload session: the documented way to send If-Match. */
  private async upload(
    rel: string,
    bytes: Uint8Array,
    condition: { ifMatch: string } | "create",
  ): Promise<DriveItem> {
    return this.send(await this.session(rel, condition), bytes);
  }

  /** Opens an upload session; a stale If-Match is refused here (412). */
  private async session(rel: string, condition: { ifMatch: string } | "create"): Promise<string> {
    const session = await this.graph.json<{ uploadUrl?: string }>(
      "POST",
      await this.sub(rel, "createUploadSession"),
      {
        headers: {
          "Content-Type": "application/json",
          ...(condition === "create" ? {} : { "If-Match": condition.ifMatch }),
        },
        body: JSON.stringify({
          item: {
            "@microsoft.graph.conflictBehavior": condition === "create" ? "fail" : "replace",
          },
        }),
      },
    );
    if (!session.uploadUrl) {
      throw new StorageError(
        "OneDrive couldn't save Lineage's history just now. Try again.",
        "no upload URL",
      );
    }
    return session.uploadUrl;
  }

  /** Sends the bytes to an upload session, in one range. */
  private async send(uploadUrl: string, bytes: Uint8Array): Promise<DriveItem> {
    // One range is enough: the ledger stays far below the 60 MiB per-request limit.
    const response = await this.graph.plain("PUT", uploadUrl, {
      headers: { "Content-Range": `bytes 0-${String(bytes.length - 1)}/${String(bytes.length)}` },
      body: bytes as Uint8Array<ArrayBuffer>,
    });
    return (await response.json()) as DriveItem;
  }

  /**
   * Development check (decision 0009): does OneDrive refuse a write based on an
   * old eTag, both when the upload session is opened and when its bytes arrive
   * after the file changed? Uses a scratch file under `diagnostics/`.
   */
  async checkConditionalWrites(): Promise<{
    refusedWhenOpened: boolean;
    refusedWhenSent: boolean;
  }> {
    const rel = join(this.layout.history, "diagnostics", "conditional-write-check.txt");
    const text = (s: string) => new TextEncoder().encode(s);
    const first = await this.put(rel, text("first"));
    const second = await this.put(rel, text("second"));
    const refused = async (attempt: () => Promise<unknown>) => {
      try {
        await attempt();
        return false;
      } catch (error) {
        if (error instanceof StorageConflict) {
          return true;
        }
        throw error;
      }
    };
    const refusedWhenOpened = await refused(() =>
      this.upload(rel, text("stale"), { ifMatch: first.eTag ?? "none" }),
    );
    const url = await this.session(rel, { ifMatch: second.eTag ?? "none" });
    await this.put(rel, text("changed elsewhere"));
    const refusedWhenSent = await refused(() => this.send(url, text("late")));
    return { refusedWhenOpened, refusedWhenSent };
  }

  private async put(rel: string, bytes: Uint8Array): Promise<DriveItem> {
    return this.graph.json<DriveItem>("PUT", await this.sub(rel, "content"), {
      headers: { "Content-Type": "application/octet-stream" },
      body: bytes as Uint8Array<ArrayBuffer>,
    });
  }

  refFor(kind: FileKind, name: string): string {
    return join(this.layout.folder(kind), checkFileName(name));
  }

  async readFile(ref: string): Promise<Uint8Array> {
    const item = await this.meta(ref, "id,@microsoft.graph.downloadUrl");
    if (!item) {
      throw new NotFound(ref);
    }
    return this.download(ref, item);
  }

  async readFileVersioned(ref: string): Promise<{ bytes: Uint8Array; version: string }> {
    const item = await this.meta(ref, "id,eTag,@microsoft.graph.downloadUrl");
    if (!item?.eTag) {
      throw new NotFound(ref);
    }
    // The eTag first, then the bytes, as for ledgers: newer bytes than the
    // eTag can only make the replace refused, never lose a change.
    return { bytes: await this.download(ref, item), version: item.eTag };
  }

  async replaceFile(ref: string, bytes: Uint8Array, expected: string): Promise<string> {
    let written: DriveItem;
    try {
      written = await this.upload(ref, bytes, { ifMatch: expected });
    } catch (error) {
      if (error instanceof StorageConflict) {
        throw new FileChanged(ref);
      }
      throw error;
    }
    const eTag = written.eTag ?? (await this.meta(ref, "id,eTag"))?.eTag;
    if (!eTag) {
      throw new StorageError("OneDrive couldn't save that version just now.", "no eTag");
    }
    return eTag;
  }

  async writeFile(
    kind: FileKind,
    name: string,
    bytes: Uint8Array,
    replace = false,
  ): Promise<StoredFile> {
    const ref = this.refFor(kind, name);
    if (!replace && (await this.meta(ref, "id"))) {
      throw new FileExists(checkFileName(name));
    }
    await this.put(ref, bytes);
    return { ref, name: checkFileName(name) };
  }

  async fileNames(kind: FileKind): Promise<string[]> {
    return (await this.children(this.layout.folder(kind)))
      .filter((i) => i.file !== undefined && OFFICE_FILE.test(i.name))
      .map((i) => i.name)
      .sort();
  }

  async fileLocation(ref: string): Promise<FileLocation> {
    const item = await this.meta(ref, "id,webUrl");
    if (!item?.webUrl) {
      throw new NotFound(ref);
    }
    return { webUrl: item.webUrl };
  }

  /**
   * Whether a document's address is in this person's OneDrive, judged by its
   * owner (owner.ts) against the app folder's drive and address, which the
   * app-folder permission may read. Undefined when either can't be told.
   */
  async holds(url: string): Promise<boolean | undefined> {
    const owner = ownerOf(url);
    if (!owner) {
      return undefined;
    }
    const own = await this.ownDrive();
    return own ? isOwn(owner, own) : undefined;
  }

  /** The person's own drive, from the app folder, asked for once. */
  private ownDrive(): Promise<OwnDrive | undefined> {
    return (this.own ??= this.graph
      .json<{ webUrl?: string; parentReference?: { driveId?: string } }>(
        "GET",
        "/me/drive/special/approot?$select=id,webUrl,parentReference",
      )
      .then(
        (item) => ({ driveId: item.parentReference?.driveId, webUrl: item.webUrl }),
        () => undefined,
      ));
  }

  async readLedger(family: string): Promise<{ bytes: Uint8Array; version: LedgerVersion }> {
    const rel = this.history(family, "ledger.jsonl");
    const item = await this.meta(rel, "id,eTag,@microsoft.graph.downloadUrl");
    if (!item?.eTag) {
      this.ledgers.delete(family);
      return { bytes: EMPTY, version: "" };
    }
    // Read the eTag first, then the bytes: if the ledger changes in between, the
    // bytes are newer than the eTag and the next append is refused, never lost.
    const bytes = await this.download(rel, item);
    this.ledgers.set(family, { eTag: item.eTag, bytes });
    return { bytes, version: item.eTag };
  }

  async appendLedger(
    family: string,
    expected: LedgerVersion,
    lines: Uint8Array,
  ): Promise<LedgerVersion> {
    const rel = this.history(family, "ledger.jsonl");
    let current: Uint8Array = EMPTY;
    if (expected !== "") {
      const cached = this.ledgers.get(family);
      if (cached?.eTag === expected) {
        current = cached.bytes;
      } else {
        const read = await this.readLedger(family);
        if (read.version !== expected) {
          throw new StorageConflict(`the ledger is at ${read.version}, not ${expected}`);
        }
        current = read.bytes;
      }
    }
    const next = concat(current, lines);
    const written = await this.upload(
      rel,
      next,
      expected === "" ? "create" : { ifMatch: expected },
    );
    const eTag = written.eTag ?? (await this.meta(rel, "id,eTag"))?.eTag;
    if (!eTag) {
      throw new StorageError(
        "OneDrive couldn't save Lineage's history just now. Try again.",
        "no eTag",
      );
    }
    this.ledgers.set(family, { eTag, bytes: next });
    return eTag;
  }

  /** Every blob a family has, listed once per session. */
  private blobIndex(family: string, refresh = false): Promise<Map<string, string | undefined>> {
    const existing = refresh ? undefined : this.blobs.get(family);
    if (existing) {
      return existing;
    }
    const index = this.children(this.history(family, "blobs")).then(
      (items) =>
        new Map(
          items
            .filter((i) => i.file !== undefined)
            .map((i) => [i.name, i["@microsoft.graph.downloadUrl"]]),
        ),
    );
    this.blobs.set(family, index);
    index.catch(() => this.blobs.delete(family));
    return index;
  }

  async putBlob(family: string, hash: string, bytes: Uint8Array): Promise<void> {
    if ((await sha256Hex(bytes)) !== checkHash(hash)) {
      throw new BadBlob(hash);
    }
    const index = await this.blobIndex(family);
    if (index.has(hash)) {
      return;
    }
    await this.put(this.history(family, "blobs", hash), bytes);
    index.set(hash, undefined);
  }

  async getBlob(family: string, hash: string): Promise<Uint8Array | undefined> {
    checkHash(hash);
    let index = await this.blobIndex(family);
    if (!index.has(hash)) {
      // Another device may have added it since the listing.
      index = await this.blobIndex(family, true);
      if (!index.has(hash)) {
        return undefined;
      }
    }
    const url = index.get(hash);
    return this.download(
      this.history(family, "blobs", hash),
      url ? { id: "", name: hash, "@microsoft.graph.downloadUrl": url } : undefined,
    );
  }

  async families(): Promise<string[]> {
    return (await this.children(this.layout.history))
      .filter((i) => i.folder !== undefined && ID.test(i.name))
      .map((i) => i.name)
      .sort();
  }

  async readIndex(): Promise<{ bytes: Uint8Array; version: LedgerVersion } | undefined> {
    const rel = join(this.layout.history, "index.json");
    const item = await this.meta(rel, "id,eTag,@microsoft.graph.downloadUrl");
    if (!item?.eTag) {
      return undefined;
    }
    return { bytes: await this.download(rel, item), version: item.eTag };
  }

  async writeIndex(bytes: Uint8Array, expected: LedgerVersion | undefined): Promise<LedgerVersion> {
    const rel = join(this.layout.history, "index.json");
    const written = await this.upload(
      rel,
      bytes,
      expected === undefined ? "create" : { ifMatch: expected },
    );
    const eTag = written.eTag ?? (await this.meta(rel, "id,eTag"))?.eTag;
    if (!eTag) {
      throw new StorageError("OneDrive couldn't save Lineage's list just now.", "no eTag");
    }
    return eTag;
  }

  async ledgerVersions(families: readonly string[]): Promise<Map<string, LedgerVersion>> {
    const rels = families.map((f) => this.history(f, "ledger.jsonl"));
    const versions = new Map<string, LedgerVersion>();
    for (let start = 0; start < families.length; start += BATCH) {
      const chunk = families.slice(start, start + BATCH);
      const requests = await Promise.all(
        chunk.map(async (_, i) => ({
          id: String(i),
          method: "GET",
          url: `${await this.item(rels[start + i] ?? "")}?$select=id,eTag`,
        })),
      );
      const reply = await this.graph.json<BatchResponse>("POST", "/$batch", {
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ requests }),
      });
      const byId = new Map((reply.responses ?? []).map((r) => [r.id, r]));
      for (const [i, family] of chunk.entries()) {
        const answer = byId.get(String(i));
        if (answer?.status === 200 && answer.body?.eTag) {
          versions.set(family, answer.body.eTag);
        } else if (answer?.status === 404) {
          versions.set(family, "");
        } else {
          // Throttled or failed inside the batch: ask on its own, with retries.
          const item = await this.meta(rels[start + i] ?? "", "id,eTag");
          versions.set(family, item?.eTag ?? "");
        }
      }
    }
    return versions;
  }
}
