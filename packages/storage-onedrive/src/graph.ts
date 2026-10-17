// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// A small Microsoft Graph client: bearer tokens, retries when Graph is busy, and
// errors in the panel's words. Nothing else in this package calls fetch for
// Graph directly.
//
// Throttling: https://learn.microsoft.com/graph/throttling (429 and 503 carry
// Retry-After, in seconds).

import { FileLocked, NotFound, StorageConflict, StorageError } from "@lineage/shell";

export const GRAPH = "https://graph.microsoft.com/v1.0";

/** Returns an access token for Graph; `forceRefresh` after a 401. */
export type TokenProvider = (options: { forceRefresh: boolean }) => Promise<string>;

export interface GraphOptions {
  readonly token: TokenProvider;
  readonly fetch?: typeof fetch;
  /** For tests: how to wait between retries. */
  readonly sleep?: (ms: number) => Promise<void>;
  /** Attempts for a busy or failing Graph, the first included. */
  readonly attempts?: number;
}

export interface RequestOptions {
  readonly body?: BodyInit;
  readonly headers?: Record<string, string>;
  /** Statuses the caller handles itself instead of them becoming errors. */
  readonly allow?: readonly number[];
}

const BUSY = new Set([429, 503, 504]);

/** Graph's error code from a failed response body, if any. */
async function errorCode(response: Response): Promise<string> {
  try {
    const body = (await response.clone().json()) as { error?: { code?: unknown } };
    return typeof body.error?.code === "string" ? body.error.code : "";
  } catch {
    return "";
  }
}

export class Graph {
  private readonly token: TokenProvider;
  private readonly fetcher: typeof fetch;
  private readonly sleep: (ms: number) => Promise<void>;
  private readonly attempts: number;

  constructor(options: GraphOptions) {
    this.token = options.token;
    this.fetcher = options.fetch ?? ((input, init) => globalThis.fetch(input, init));
    this.sleep =
      options.sleep ??
      ((ms) =>
        new Promise((resolve) => {
          setTimeout(resolve, ms);
        }));
    this.attempts = options.attempts ?? 4;
  }

  /** A Graph request (path from `/v1.0`), with auth, retries and plain errors. */
  async request(method: string, path: string, options: RequestOptions = {}): Promise<Response> {
    let forceRefresh = false;
    for (let attempt = 1; ; attempt++) {
      const token = await this.token({ forceRefresh });
      const response = await this.send(`${GRAPH}${path}`, {
        method,
        headers: { ...options.headers, Authorization: `Bearer ${token}` },
        ...(options.body === undefined ? {} : { body: options.body }),
      });
      if (response.ok || options.allow?.includes(response.status)) {
        return response;
      }
      if (response.status === 401 && !forceRefresh) {
        forceRefresh = true;
        continue;
      }
      if ((BUSY.has(response.status) || response.status >= 500) && attempt < this.attempts) {
        await this.sleep(retryAfter(response, attempt));
        continue;
      }
      throw await graphError(response, `${method} ${path}`);
    }
  }

  async json<T>(method: string, path: string, options: RequestOptions = {}): Promise<T> {
    const response = await this.request(method, path, options);
    return (await response.json()) as T;
  }

  /**
   * A pre-authenticated URL (download or upload session): no Authorization
   * header, as the docs require, with the same retries.
   */
  async plain(method: string, url: string, init: RequestInit = {}): Promise<Response> {
    for (let attempt = 1; ; attempt++) {
      const response = await this.send(url, { ...init, method });
      if (response.ok) {
        return response;
      }
      if ((BUSY.has(response.status) || response.status >= 500) && attempt < this.attempts) {
        await this.sleep(retryAfter(response, attempt));
        continue;
      }
      throw await graphError(response, `${method} a pre-authenticated URL`);
    }
  }

  private async send(url: string, init: RequestInit): Promise<Response> {
    try {
      return await this.fetcher(url, init);
    } catch (error) {
      throw new StorageError(
        "Lineage can't reach OneDrive. Check your connection and try again.",
        `network: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }
}

function retryAfter(response: Response, attempt: number): number {
  const header = Number(response.headers.get("Retry-After"));
  if (Number.isFinite(header) && header > 0) {
    return Math.min(header, 60) * 1000;
  }
  return Math.min(2 ** attempt * 500, 8000);
}

/** A failed Graph response as a storage error the panel can show. */
export async function graphError(response: Response, what: string): Promise<StorageError> {
  const code = await errorCode(response);
  const detail = `${what}: ${String(response.status)} ${code}`.trim();
  switch (response.status) {
    case 401:
      return new StorageError("Sign in again to use your OneDrive.", detail);
    case 403:
      return new StorageError(
        "Lineage doesn't have permission to use that OneDrive folder. Sign in again and allow access.",
        detail,
      );
    case 404:
      return new NotFound(what);
    case 409:
    case 412:
      return new StorageConflict(detail);
    case 423:
      // "The resource that is being accessed is locked", e.g. a file open in
      // an app that doesn't co-author it (https://learn.microsoft.com/graph/errors).
      return new FileLocked(what);
    case 507:
      return new StorageError("Your OneDrive is full. Free up space, then try again.", detail);
    default:
      if (BUSY.has(response.status)) {
        return new StorageError("OneDrive is busy right now. Try again in a minute.", detail);
      }
      return new StorageError("OneDrive couldn't do that just now. Try again.", detail);
  }
}

/** One path segment for Graph's `:/path:` syntax. */
export function segment(name: string): string {
  return encodeURIComponent(name);
}

/** A path relative to a folder, each segment encoded. */
export function encodePath(path: string): string {
  return path.split("/").map(segment).join("/");
}
