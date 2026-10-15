// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// A document host for tests that behaves like Word where it matters: the body
// and the custom properties are kept apart, editing changes the body and keeps
// the properties, and replacing the contents brings the new file's properties in.

import type { Engine } from "../engine.js";
import type { DocumentHost } from "../host.js";
import type { Ids } from "../schemas.js";
import type { FileLocation } from "../storage.js";

export class FakeHost implements DocumentHost {
  private content: Uint8Array;
  private ids: Ids | null;
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
    this.ids = engine.readIds(bytes);
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
