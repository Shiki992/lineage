// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// A document host for tests that behaves like Word where it matters: the body
// and the custom properties are kept apart, editing changes the body and keeps
// the properties, and replacing the contents brings the new file's properties in.

import type { Engine } from "../engine.js";
import type { DocumentHost, SlideHost } from "../host.js";
import type { Ids, SlideOp } from "../schemas.js";
import type { FileLocation } from "../storage.js";

export class FakeHost implements DocumentHost {
  protected content: Uint8Array;
  protected ids: Ids | null;
  saves = 0;
  replaced = 0;
  readonly opened: FileLocation[] = [];
  readonly copies: Uint8Array[] = [];
  /** What Word does to a file on the way in; by default, nothing. */
  mangle: ((bytes: Uint8Array) => Uint8Array) | undefined;

  constructor(
    private readonly engine: Engine,
    bytes: Uint8Array,
    private readonly info: { name: string; url: string | null },
  ) {
    this.content = bytes;
    // Word keeps the ids as document properties, so a file it can't read as
    // .docx here simply has none.
    try {
      this.ids = engine.readIds(bytes);
    } catch {
      this.ids = null;
    }
  }

  /** The person edits the document: its body becomes `bytes`' body. */
  edit(bytes: Uint8Array): void {
    this.content = bytes;
  }

  readBytes(): Promise<Uint8Array> {
    return Promise.resolve(this.ids ? this.engine.writeIds(this.content, this.ids) : this.content);
  }

  readIds(): Promise<Ids | null> {
    return Promise.resolve(this.ids);
  }

  writeIds(ids: Ids | null): Promise<void> {
    this.ids = ids;
    return Promise.resolve();
  }

  replaceContents(bytes: Uint8Array): Promise<void> {
    const landed = this.mangle ? this.mangle(bytes) : bytes;
    this.replaced += 1;
    this.content = landed;
    this.ids = this.engine.readIds(landed) ?? this.ids;
    return Promise.resolve();
  }

  save(): Promise<void> {
    this.saves += 1;
    return Promise.resolve();
  }

  describe(): Promise<{ name: string; url: string | null }> {
    return Promise.resolve(this.info);
  }

  openFile(location: FileLocation): Promise<void> {
    this.opened.push(location);
    return Promise.resolve();
  }

  openCopy(bytes: Uint8Array): Promise<void> {
    this.copies.push(bytes);
    return Promise.resolve();
  }
}

/**
 * PowerPoint for the open-deck path (decision 0019). It can't look inside a
 * deck, so carrying out the steps makes it hold the source deck, as real
 * PowerPoint does when every step lands. Tests script the rest: what it holds
 * instead (`lands`), a step that fails (`failAt`, holding `halfway`), or a
 * failure while putting the deck back (`failPutBack`).
 */
export class FakePowerPoint extends FakeHost implements SlideHost {
  /** Every call's steps, in order. */
  readonly applied: SlideOp[][] = [];
  /** Changes what PowerPoint holds after the steps; by default, the source. */
  lands: ((source: Uint8Array) => Uint8Array) | undefined;
  /** Throws at this step (0-based) of the next call, holding `halfway`. */
  failAt: number | undefined;
  halfway: Uint8Array | undefined;
  /** The second call (putting the deck back) throws. */
  failPutBack = false;
  supported = true;

  canChangeSlides(): boolean {
    return this.supported;
  }

  applySlideOps(ops: readonly SlideOp[], source: Uint8Array): Promise<void> {
    this.applied.push([...ops]);
    if (this.failPutBack && this.applied.length > 1) {
      return Promise.reject(new Error("SlideNotFound while putting back"));
    }
    if (this.failAt !== undefined && this.failAt < ops.length) {
      this.failAt = undefined;
      if (this.halfway) {
        this.content = this.halfway;
      }
      return Promise.reject(new Error("SlideNotFound"));
    }
    const lands = this.lands;
    this.lands = undefined;
    this.content = lands ? lands(source) : source;
    return Promise.resolve();
  }
}
