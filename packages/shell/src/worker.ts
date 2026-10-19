// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The engine behind a message port: in a Web Worker for the Excel pane, so
// comparing a large workbook never blocks Office (decision 0025), or in this
// thread, wrapped to the same promise-returning shape (`asyncEngine`).
//
// The protocol is one request, one reply: `{id, method, args}` in, `{id, ok,
// value}` or `{id, ok: false, error}` back. Bytes and maps travel by
// structured clone. Errors come back as the same classes, so `Lineage` still
// tells "fetch these blobs" from a real failure.

import type { Engine } from "./engine.js";
import { EngineBoundaryError, EngineError } from "./engine.js";
import type * as s from "./schemas.js";

/** Every method of `Engine`, returning a promise. */
export type AsyncEngine = {
  readonly [
    K in keyof Engine as Engine[K] extends (...args: never[]) => unknown ? K : never
  ]: Engine[K] extends (...args: infer A) => infer R ? (...args: A) => Promise<R> : never;
} & {
  /** Engine version, as loaded. */
  readonly version: string;
  /** Marks an engine that is already asynchronous. */
  readonly async: true;
};

/** The engine as `Lineage` uses it: asynchronous, whichever kind it was given. */
export function asyncEngine(engine: Engine | AsyncEngine): AsyncEngine {
  if ((engine as Partial<AsyncEngine>).async === true) {
    return engine as AsyncEngine;
  }
  const out: Record<string, unknown> = { version: engine.version, async: true };
  for (const [key, value] of Object.entries(engine)) {
    if (typeof value === "function") {
      out[key] = (...args: unknown[]): Promise<unknown> => {
        try {
          return Promise.resolve((value as (...a: unknown[]) => unknown).apply(engine, args));
        } catch (error) {
          return Promise.reject(error instanceof Error ? error : new Error(String(error)));
        }
      };
    }
  }
  return out as unknown as AsyncEngine;
}

/** A worker, or one end of a `MessageChannel`. */
export interface EnginePort {
  postMessage(message: unknown): void;
  onmessage: ((event: MessageEvent) => void) | null;
}

interface Request {
  readonly id: number;
  readonly method: string;
  readonly args: readonly unknown[];
}

type SentError =
  | {
      readonly name: "EngineError";
      readonly kind: s.ErrorKind | "crashed";
      readonly userMessage: string;
      readonly detail: string;
      readonly hashes: readonly string[];
      readonly family: string | undefined;
    }
  | { readonly name: string; readonly message: string };

type Response =
  | { readonly id: number; readonly ok: true; readonly value: unknown }
  | { readonly id: number; readonly ok: false; readonly error: SentError };

function send(error: unknown): SentError {
  if (error instanceof EngineError) {
    return {
      name: "EngineError",
      kind: error.kind,
      userMessage: error.userMessage,
      detail: error.detail,
      hashes: [...error.hashes],
      family: error.family,
    };
  }
  if (error instanceof Error) {
    return { name: error.name, message: error.message };
  }
  return { name: "Error", message: String(error) };
}

function receive(error: SentError): Error {
  if ("kind" in error) {
    return new EngineError(error.kind, error.userMessage, error.detail, error.hashes, error.family);
  }
  if (error.name === "EngineBoundaryError") {
    return new EngineBoundaryError(error.message);
  }
  const out = new Error(error.message);
  out.name = error.name;
  return out;
}

/** Answers engine calls arriving on `port` (run inside the worker). */
export function serveEngine(engine: Engine | Promise<Engine>, port: EnginePort): void {
  const ready = Promise.resolve(engine);
  port.onmessage = (event: MessageEvent) => {
    const { id, method, args } = event.data as Request;
    void ready
      .then((e) => {
        if (method === "version") {
          return e.version;
        }
        const fn = (e as unknown as Record<string, unknown>)[method];
        if (typeof fn !== "function") {
          throw new EngineBoundaryError(`no engine method ${method}`);
        }
        return (fn as (...a: unknown[]) => unknown).apply(e, [...args]);
      })
      .then(
        (value) => {
          port.postMessage({ id, ok: true, value } satisfies Response);
        },
        (error: unknown) => {
          port.postMessage({ id, ok: false, error: send(error) } satisfies Response);
        },
      );
  };
}

/**
 * The engine served on `port` (by `serveEngine` in a worker), once it has
 * loaded. Calls run one after another in the worker, in the order made.
 */
export async function connectEngine(port: EnginePort): Promise<AsyncEngine> {
  let next = 0;
  const waiting = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();
  port.onmessage = (event: MessageEvent) => {
    const reply = event.data as Response;
    const call = waiting.get(reply.id);
    if (!call) {
      return;
    }
    waiting.delete(reply.id);
    if (reply.ok) {
      call.resolve(reply.value);
    } else {
      call.reject(receive(reply.error));
    }
  };
  const call = (method: string, args: readonly unknown[]): Promise<unknown> =>
    new Promise((resolve, reject) => {
      next += 1;
      waiting.set(next, { resolve, reject });
      port.postMessage({ id: next, method, args } satisfies Request);
    });
  const version = await call("version", []);
  if (typeof version !== "string") {
    throw new EngineBoundaryError("the engine worker gave no version");
  }
  return new Proxy({} as AsyncEngine, {
    get(_, key) {
      if (key === "version") {
        return version;
      }
      if (key === "async") {
        return true;
      }
      // Not a thenable, and nothing else is a method.
      if (typeof key !== "string" || key === "then") {
        return undefined;
      }
      return (...args: unknown[]) => call(key, args);
    },
  });
}
