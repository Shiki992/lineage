// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// Development and tests only: a dev server keeps Lineage's history in a folder
// on this computer, so the Word panel and "My documents" can be tried before
// OneDrive sign-in is set up, and the Rust harness can read what they wrote.
// Never part of a production build (`apply: "serve"`), and it answers only the
// origins it is given. The layout is the harness's LocalFolderStorage
// (crates/harness/src/storage.rs), with the task pane's two file folders:
//
//   <root>/Lineage/<familyId>/ledger.jsonl
//   <root>/Lineage/<familyId>/blobs/<sha256>
//   <root>/Lineage/index.json              the library index (decision 0015)
//   <root>/Tailored versions/<name>.docx or .pptx
//   <root>/Sent copies/<name>.docx or .pptx
//
// It also takes timings from the panel (`/log`) for the budgets, never document
// text, and serves Word smoke-test files for the write spike (`/spike`).

import { createHash } from "node:crypto";
import fs from "node:fs";
import type { IncomingMessage, ServerResponse } from "node:http";
import path from "node:path";
import type { Plugin } from "vite";

const ID = /^[0-9a-f]{32}$/;
const HASH = /^[0-9a-f]{64}$/;
const FOLDERS: Record<string, string> = { version: "Tailored versions", sent: "Sent copies" };

/** Where the dev server answers. */
export const ROUTE = "/__lineage-dev";

function fileName(name: string): string | undefined {
  const n = name.trim();
  const bad =
    n.length === 0 ||
    n.length > 200 ||
    /["*:<>?/\\|]/.test(n) ||
    n.split("").some((c) => c.charCodeAt(0) < 32) ||
    n.endsWith(".") ||
    n === "..";
  return bad ? undefined : n;
}

async function body(req: IncomingMessage): Promise<Buffer> {
  const chunks: Buffer[] = [];
  for await (const chunk of req) {
    chunks.push(chunk as Buffer);
  }
  return Buffer.concat(chunks);
}

function send(res: ServerResponse, status: number, data?: Buffer | string | object): void {
  res.statusCode = status;
  if (data === undefined) {
    res.end();
  } else if (Buffer.isBuffer(data)) {
    res.setHeader("Content-Type", "application/octet-stream");
    res.end(data);
  } else if (typeof data === "string") {
    res.setHeader("Content-Type", "text/plain; charset=utf-8");
    res.end(data);
  } else {
    res.setHeader("Content-Type", "application/json");
    res.end(JSON.stringify(data));
  }
}

/** A path inside `root`, or undefined if `rel` would leave it. */
function inside(root: string, rel: string): string | undefined {
  const full = path.resolve(root, rel);
  return full.startsWith(path.resolve(root) + path.sep) ? full : undefined;
}

/** A ref written by this storage: "<folder>/<name>" or a plain name in the root. */
function refPath(root: string, ref: string): string | undefined {
  const parts = ref.split("/");
  if (
    parts.length === 2 &&
    Object.values(FOLDERS).includes(parts[0] ?? "") &&
    fileName(parts[1] ?? "")
  ) {
    return inside(root, ref);
  }
  if (parts.length === 1 && fileName(ref)) {
    return inside(root, ref);
  }
  return undefined;
}

/** A stored file's version: the start of its hash. */
function fileVersion(bytes: Buffer): string {
  return createHash("sha256").update(bytes).digest("hex").slice(0, 16);
}

/** The index's version: the start of its hash, as the harness computes it. */
function indexVersion(bytes: Buffer): string {
  return createHash("sha256").update(bytes).digest("hex").slice(0, 16);
}

function ledgerLength(history: string, family: string): number {
  const file = path.join(history, family, "ledger.jsonl");
  return fs.existsSync(file) ? fs.statSync(file).size : 0;
}

/** Answers one request to the dev storage, relative to `ROUTE`. */
export async function handle(
  root: string,
  req: IncomingMessage,
  res: ServerResponse,
): Promise<void> {
  const url = new URL(req.url ?? "/", "https://localhost");
  const [route = "", a = "", b = ""] = url.pathname
    .split("/")
    .filter(Boolean)
    .map(decodeURIComponent);
  const method = req.method ?? "GET";
  const history = path.join(root, "Lineage");

  if (route === "ledger" && ID.test(a)) {
    const file = path.join(history, a, "ledger.jsonl");
    const current = fs.existsSync(file) ? fs.readFileSync(file) : Buffer.alloc(0);
    if (method === "GET") {
      res.setHeader("X-Ledger-Version", String(current.length));
      send(res, 200, current);
      return;
    }
    if (method === "POST") {
      const lines = await body(req);
      if (url.searchParams.get("expected") !== String(current.length)) {
        send(res, 409, { error: "conflict" });
        return;
      }
      fs.mkdirSync(path.dirname(file), { recursive: true });
      const fd = fs.openSync(file, "a");
      fs.writeSync(fd, lines);
      fs.fsyncSync(fd);
      fs.closeSync(fd);
      send(res, 200, { version: String(current.length + lines.length) });
      return;
    }
  }
  if (route === "ledger-versions" && method === "POST") {
    const families: unknown = JSON.parse((await body(req)).toString("utf8"));
    if (!Array.isArray(families) || !families.every((f) => typeof f === "string" && ID.test(f))) {
      send(res, 400, { error: "badFamilies" });
      return;
    }
    send(
      res,
      200,
      Object.fromEntries(families.map((f: string) => [f, String(ledgerLength(history, f))])),
    );
    return;
  }
  if (route === "index") {
    const file = path.join(history, "index.json");
    const current = fs.existsSync(file) ? fs.readFileSync(file) : undefined;
    if (method === "GET") {
      if (!current) {
        send(res, 404);
        return;
      }
      res.setHeader("X-Index-Version", indexVersion(current));
      send(res, 200, current);
      return;
    }
    if (method === "PUT") {
      const expected = url.searchParams.get("expected");
      if ((current ? indexVersion(current) : null) !== expected) {
        send(res, 409, { error: "conflict" });
        return;
      }
      const bytes = await body(req);
      fs.mkdirSync(history, { recursive: true });
      fs.writeFileSync(`${file}.tmp`, bytes);
      fs.renameSync(`${file}.tmp`, file);
      send(res, 200, { version: indexVersion(bytes) });
      return;
    }
  }
  if (route === "blob" && ID.test(a) && HASH.test(b)) {
    const file = path.join(history, a, "blobs", b);
    if (method === "GET") {
      if (fs.existsSync(file)) {
        send(res, 200, fs.readFileSync(file));
      } else {
        send(res, 404);
      }
      return;
    }
    if (method === "PUT") {
      const bytes = await body(req);
      if (createHash("sha256").update(bytes).digest("hex") !== b) {
        send(res, 400, { error: "badBlob" });
        return;
      }
      if (!fs.existsSync(file)) {
        fs.mkdirSync(path.dirname(file), { recursive: true });
        fs.writeFileSync(`${file}.tmp`, bytes);
        fs.renameSync(`${file}.tmp`, file);
      }
      send(res, 204);
      return;
    }
  }
  if (route === "families" && method === "GET") {
    const names = fs.existsSync(history)
      ? fs
          .readdirSync(history)
          .filter((n) => ID.test(n))
          .sort()
      : [];
    send(res, 200, names);
    return;
  }
  const folder = FOLDERS[a];
  if (route === "files" && folder !== undefined) {
    const dir = path.join(root, folder);
    if (method === "GET" && b === "") {
      const names = fs.existsSync(dir)
        ? fs
            .readdirSync(dir)
            .filter((n) => /\.(docx|pptx)$/i.test(n))
            .sort()
        : [];
      send(res, 200, names);
      return;
    }
    const name = fileName(b);
    if (method === "PUT" && name) {
      const file = path.join(dir, name);
      if (fs.existsSync(file) && url.searchParams.get("replace") !== "1") {
        send(res, 409, { error: "exists" });
        return;
      }
      const bytes = await body(req);
      fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(`${file}.lineage-tmp`, bytes);
      fs.renameSync(`${file}.lineage-tmp`, file);
      send(res, 200, { ref: `${folder}/${name}`, name });
      return;
    }
  }
  if ((route === "file" || route === "location") && method === "GET") {
    const file = refPath(root, url.searchParams.get("ref") ?? "");
    if (!file || !fs.existsSync(file)) {
      send(res, 404);
      return;
    }
    if (route === "file") {
      const data = fs.readFileSync(file);
      res.setHeader("X-File-Version", fileVersion(data));
      send(res, 200, data);
    } else {
      send(res, 200, { path: file });
    }
    return;
  }
  if (route === "file" && method === "PUT") {
    // Replacing a stored file only if it is still the version that was read.
    const file = refPath(root, url.searchParams.get("ref") ?? "");
    if (!file || !fs.existsSync(file)) {
      send(res, 404);
      return;
    }
    if (fileVersion(fs.readFileSync(file)) !== url.searchParams.get("expected")) {
      send(res, 409, { error: "changed" });
      return;
    }
    const bytes = await body(req);
    fs.writeFileSync(`${file}.lineage-tmp`, bytes);
    fs.renameSync(`${file}.lineage-tmp`, file);
    send(res, 200, { version: fileVersion(bytes) });
    return;
  }
  if (route === "log" && method === "POST") {
    // Timings and error kinds only: anything else is dropped.
    const entry = JSON.parse((await body(req)).toString("utf8")) as Record<string, unknown>;
    const kept = {
      at: new Date().toISOString(),
      name: typeof entry["name"] === "string" ? entry["name"].slice(0, 60) : "",
      ms: typeof entry["ms"] === "number" ? Math.round(entry["ms"]) : undefined,
      kind: typeof entry["kind"] === "string" ? entry["kind"].slice(0, 40) : undefined,
    };
    fs.mkdirSync(root, { recursive: true });
    fs.appendFileSync(path.join(root, "dev-log.jsonl"), `${JSON.stringify(kept)}\n`);
    send(res, 204);
    return;
  }
  if (route === "spike-actual" && method === "POST" && fileName(a)) {
    // What Word held after a failed write check, for the engine's tests.
    const dir = path.join(root, "spike-actual");
    const file = inside(dir, a);
    if (file) {
      fs.mkdirSync(dir, { recursive: true });
      fs.writeFileSync(file, await body(req));
      send(res, 204);
      return;
    }
  }
  if (route === "spike" && method === "GET") {
    const dir = path.join(root, "spike");
    if (a === "") {
      const names = fs.existsSync(dir)
        ? fs
            .readdirSync(dir)
            .filter((n) => /\.(docx|pptx)$/i.test(n))
            .sort()
        : [];
      send(res, 200, names);
      return;
    }
    const file = fileName(a) ? inside(dir, a) : undefined;
    if (file && fs.existsSync(file)) {
      send(res, 200, fs.readFileSync(file));
    } else {
      send(res, 404);
    }
    return;
  }
  send(res, 404, { error: "unknown route" });
}

/**
 * The Vite plugin. `origins` are the pages allowed to call it (the task pane,
 * My documents); a request from any other site is refused.
 */
export function localFolderStorage(root: string, origins: readonly string[]): Plugin {
  return {
    name: "lineage-dev-storage",
    apply: "serve",
    configureServer(server) {
      server.middlewares.use(ROUTE, (req, res) => {
        const origin = req.headers.origin;
        if (origin !== undefined && !origins.includes(origin)) {
          send(res, 403);
          return;
        }
        handle(root, req, res).catch((error: unknown) => {
          send(res, 500, String(error));
        });
      });
    },
  };
}
