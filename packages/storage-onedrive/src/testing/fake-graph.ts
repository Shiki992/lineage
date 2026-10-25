// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// An in-memory OneDrive, answering the Graph requests OneDriveStorage makes, the
// way the docs describe them: path addressing, eTags that change on every
// write, If-Match → 412, conflictBehavior fail → 409, download URLs that expire,
// upload sessions that refuse an Authorization header, and 429 with Retry-After.

const GRAPH = "https://graph.microsoft.com/v1.0";
const DOWNLOAD = "https://download.fake/";
const UPLOAD = "https://upload.fake/";

interface Entry {
  id: string;
  drive: string;
  path: string;
  folder: boolean;
  content: Uint8Array;
  version: number;
}

interface Session {
  drive: string;
  path: string;
  behavior: string;
}

export interface Recorded {
  method: string;
  url: string;
  headers: Record<string, string>;
}

function json(status: number, body: unknown, headers: Record<string, string> = {}): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json", ...headers },
  });
}

function error(status: number, code: string): Response {
  return json(status, { error: { code, message: code } });
}

export class FakeGraph {
  private readonly entries = new Map<string, Entry>();
  private readonly sessions = new Map<string, Session>();
  private nextId = 1;
  readonly requests: Recorded[] = [];
  /** The next N Graph requests answer 429 with Retry-After: 1. */
  busy = 0;
  /** Download URLs handed out before this are expired. */
  private urlEpoch = 0;
  /** What /drives/{d}/root:/path and /shares resolve the master to. */
  readonly drive = "d1";
  /**
   * The person's own drive as Graph names it: a personal drive's id is its
   * cid, and a work drive's addresses start with its owner's site.
   */
  me = { driveId: "a1b2c3d4e5f60718", site: "https://onedrive.fake/me" };
  /** Sharing links by permission id (createLink). */
  readonly links = new Map<string, { drive: string; path: string; type: string; scope: string }>();
  /**
   * Whether the app-folder permission may share. Graph's docs list only
   * Files.ReadWrite for createLink, so by default it can't: only a request
   * with the token "wider" (Files.ReadWrite) may.
   */
  appFolderShares = false;

  constructor() {
    this.folder("me", "");
    this.folder("me", "Apps/Lineage");
    this.folder(this.drive, "");
  }

  private key(drive: string, path: string): string {
    return `${drive}|${path}`;
  }

  private folder(drive: string, path: string): Entry {
    const existing = this.entries.get(this.key(drive, path));
    if (existing) {
      return existing;
    }
    if (path !== "") {
      this.folder(drive, path.split("/").slice(0, -1).join("/"));
    }
    const entry: Entry = {
      id: `i${String(this.nextId++)}`,
      drive,
      path,
      folder: true,
      content: new Uint8Array(),
      version: 1,
    };
    this.entries.set(this.key(drive, path), entry);
    return entry;
  }

  /** Puts a file in the fake drive, as if the person had saved it. */
  file(drive: string, path: string, content: Uint8Array): Entry {
    this.folder(drive, path.split("/").slice(0, -1).join("/"));
    const existing = this.entries.get(this.key(drive, path));
    if (existing) {
      existing.content = content;
      existing.version += 1;
      return existing;
    }
    const entry: Entry = {
      id: `i${String(this.nextId++)}`,
      drive,
      path,
      folder: false,
      content,
      version: 1,
    };
    this.entries.set(this.key(drive, path), entry);
    return entry;
  }

  /** Reads a file's bytes, for assertions. */
  read(drive: string, path: string): Uint8Array | undefined {
    return this.entries.get(this.key(drive, path))?.content;
  }

  /** Every download URL handed out so far stops working. */
  expireDownloadUrls(): void {
    this.urlEpoch += 1;
  }

  private eTag(e: Entry): string {
    return `"{${e.id}},${String(e.version)}"`;
  }

  private view(e: Entry): Record<string, unknown> {
    const name = e.path.split("/").pop() ?? "";
    return {
      id: e.id,
      name,
      eTag: this.eTag(e),
      // The content tag: changes when the bytes do, as cTag does.
      cTag: `"c:{${e.id}},${String(e.version)}"`,
      webUrl:
        e.drive === "me"
          ? `${this.me.site}/${e.path.split("/").map(encodeURIComponent).join("/")}`
          : `https://onedrive.fake/${encodeURIComponent(e.drive)}/${e.path.split("/").map(encodeURIComponent).join("/")}`,
      ...(e.folder ? { folder: { childCount: 0 } } : { file: {} }),
      ...(e.folder
        ? {}
        : {
            "@microsoft.graph.downloadUrl": `${DOWNLOAD}${e.id}?v=${String(e.version)}&epoch=${String(this.urlEpoch)}`,
          }),
      parentReference: {
        driveId: e.drive === "me" ? this.me.driveId : e.drive,
        id: this.entries.get(this.key(e.drive, e.path.split("/").slice(0, -1).join("/")))?.id,
      },
    };
  }

  private byId(id: string): Entry | undefined {
    return [...this.entries.values()].find((e) => e.id === id);
  }

  /** Splits a Graph path into the item it addresses and the sub-resource. */
  private resolve(path: string): { drive: string; path: string; rest: string } | undefined {
    const bases: [RegExp, (m: RegExpExecArray) => { drive: string; path: string } | undefined][] = [
      [/^\/me\/drive\/special\/approot/, () => ({ drive: "me", path: "Apps/Lineage" })],
      [/^\/drives\/([^/:]+)\/root/, (m) => ({ drive: decodeURIComponent(m[1] ?? ""), path: "" })],
      [
        /^\/drives\/([^/:]+)\/items\/([^/:]+)/,
        (m) => {
          const e = this.byId(decodeURIComponent(m[2] ?? ""));
          return e ? { drive: e.drive, path: e.path } : undefined;
        },
      ],
      [
        /^\/shares\/u![^/]+\/driveItem/,
        () => ({ drive: this.drive, path: "Documents/Resume.docx" }),
      ],
    ];
    for (const [pattern, base] of bases) {
      const m = pattern.exec(path);
      if (!m) {
        continue;
      }
      const at = base(m);
      if (!at) {
        return undefined;
      }
      let rest = path.slice(m[0].length);
      let rel = "";
      if (rest.startsWith(":/")) {
        const end = rest.indexOf(":", 2);
        rel = end === -1 ? rest.slice(2) : rest.slice(2, end);
        rest = end === -1 ? "" : rest.slice(end + 1);
      }
      const decoded = rel
        .split("/")
        .filter((s) => s !== "")
        .map(decodeURIComponent)
        .join("/");
      return { drive: at.drive, path: [at.path, decoded].filter((p) => p !== "").join("/"), rest };
    }
    return undefined;
  }

  /** Requests inside $batch whose answer is a 429, by URL fragment (for tests). */
  busyInBatch: string | undefined;

  /** JSON batching: each GET answered as on its own, in a different order. */
  private async batch(body: Uint8Array, headers: Record<string, string>): Promise<Response> {
    const request = JSON.parse(new TextDecoder().decode(body)) as {
      requests: { id: string; method: string; url: string }[];
    };
    if (request.requests.length > 20) {
      return error(400, "tooManyRequestsInBatch");
    }
    const responses = [];
    for (const r of [...request.requests].reverse()) {
      if (this.busyInBatch !== undefined && r.url.includes(this.busyInBatch)) {
        responses.push({ id: r.id, status: 429, headers: { "Retry-After": "1" }, body: {} });
        continue;
      }
      const answer = await this.fetch(`${GRAPH}${r.url}`, { method: r.method, headers });
      this.requests.pop();
      responses.push({ id: r.id, status: answer.status, body: (await answer.json()) as unknown });
    }
    return json(200, { responses });
  }

  /** The fetch function to hand to Graph. */
  readonly fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    const url = typeof input === "string" ? input : input instanceof URL ? input.href : input.url;
    const method = (init?.method ?? "GET").toUpperCase();
    const headers = Object.fromEntries(new Headers(init?.headers).entries());
    this.requests.push({ method, url, headers });
    const body =
      init?.body === undefined || init.body === null
        ? new Uint8Array()
        : new Uint8Array(await new Response(init.body).arrayBuffer());

    if (url.startsWith(DOWNLOAD)) {
      const u = new URL(url);
      const e = this.byId(u.pathname.slice(1));
      if (!e || u.searchParams.get("epoch") !== String(this.urlEpoch)) {
        return new Response("gone", { status: 401 });
      }
      return new Response(e.content.slice(), { status: 200 });
    }
    if (url.startsWith(UPLOAD)) {
      if (headers["authorization"]) {
        return error(401, "unauthenticated");
      }
      const session = this.sessions.get(url.slice(UPLOAD.length));
      if (!session || method !== "PUT") {
        return error(404, "itemNotFound");
      }
      const range = /^bytes 0-(\d+)\/(\d+)$/.exec(headers["content-range"] ?? "");
      if (!range || Number(range[2]) !== body.length) {
        return error(400, "invalidRange");
      }
      this.sessions.delete(url.slice(UPLOAD.length));
      if (session.behavior === "fail" && this.entries.has(this.key(session.drive, session.path))) {
        return error(409, "nameAlreadyExists");
      }
      const e = this.file(session.drive, session.path, body);
      return json(201, this.view(e));
    }
    if (!url.startsWith(GRAPH)) {
      return error(400, "unknownHost");
    }
    if (!headers["authorization"]?.startsWith("Bearer ")) {
      return error(401, "unauthenticated");
    }
    if (this.busy > 0) {
      this.busy -= 1;
      return json(429, { error: { code: "tooManyRequests" } }, { "Retry-After": "1" });
    }
    const full = new URL(url);
    if (method === "POST" && full.pathname === "/v1.0/$batch") {
      return this.batch(body, headers);
    }
    const target = this.resolve(full.pathname.slice("/v1.0".length));
    if (!target) {
      return error(404, "itemNotFound");
    }
    const entry = this.entries.get(this.key(target.drive, target.path));
    const rest = target.rest;

    if (method === "GET" && rest === "") {
      return entry ? json(200, this.view(entry)) : error(404, "itemNotFound");
    }
    if (method === "GET" && rest === "/children") {
      if (!entry?.folder) {
        return error(404, "itemNotFound");
      }
      const prefix = target.path === "" ? "" : `${target.path}/`;
      const kids = [...this.entries.values()].filter(
        (e) =>
          e.drive === target.drive &&
          e.path.startsWith(prefix) &&
          e.path !== target.path &&
          !e.path.slice(prefix.length).includes("/"),
      );
      const top = Number(full.searchParams.get("$top") ?? "200");
      const skip = Number(full.searchParams.get("$skiptoken") ?? "0");
      const page = kids.slice(skip, skip + Math.min(top, 2));
      const next =
        skip + page.length < kids.length
          ? `${full.origin}${full.pathname}?$top=${String(top)}&$skiptoken=${String(skip + page.length)}`
          : undefined;
      return json(200, {
        value: page.map((e) => this.view(e)),
        ...(next ? { "@odata.nextLink": next } : {}),
      });
    }
    if (method === "PUT" && rest === "/content") {
      const e = this.file(target.drive, target.path, body);
      return json(e.version === 1 ? 201 : 200, this.view(e));
    }
    const mayShare = this.appFolderShares || headers["authorization"] === "Bearer wider";
    if (method === "POST" && rest === "/createLink") {
      if (!entry || entry.folder) {
        return error(404, "itemNotFound");
      }
      if (!mayShare) {
        return error(403, "accessDenied");
      }
      const request = JSON.parse(new TextDecoder().decode(body)) as {
        type?: string;
        scope?: string;
      };
      const type = request.type ?? "view";
      const scope =
        request.scope ?? (this.me.site.includes(".sharepoint.com") ? "organization" : "anonymous");
      const existing = [...this.links].find(
        ([, l]) => l.drive === entry.drive && l.path === entry.path && l.type === type,
      );
      const id = existing?.[0] ?? `perm${String(this.nextId++)}`;
      if (!existing) {
        this.links.set(id, { drive: entry.drive, path: entry.path, type, scope });
      }
      return json(existing ? 200 : 201, {
        id,
        roles: [type === "edit" ? "write" : "read"],
        link: { type, scope, webUrl: `https://1drv.ms/w/${id}` },
      });
    }
    if (method === "DELETE" && rest.startsWith("/permissions/")) {
      if (!mayShare) {
        return error(403, "accessDenied");
      }
      const id = decodeURIComponent(rest.slice("/permissions/".length));
      if (!this.links.delete(id)) {
        return error(404, "itemNotFound");
      }
      return new Response(null, { status: 204 });
    }
    if (method === "POST" && rest === "/createUploadSession") {
      const ifMatch = headers["if-match"];
      if (ifMatch !== undefined && (!entry || this.eTag(entry) !== ifMatch)) {
        return error(412, "preconditionFailed");
      }
      const request = JSON.parse(new TextDecoder().decode(body)) as {
        item?: Record<string, string>;
      };
      const behavior = request.item?.["@microsoft.graph.conflictBehavior"] ?? "fail";
      const id = `s${String(this.nextId++)}`;
      this.sessions.set(id, { drive: target.drive, path: target.path, behavior });
      return json(200, { uploadUrl: `${UPLOAD}${id}`, expirationDateTime: "2026-10-16T00:00:00Z" });
    }
    return error(400, "invalidRequest");
  };
}
