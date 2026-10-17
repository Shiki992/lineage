// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

export { EngineBoundaryError, EngineError, loadEngine, loadEngineAsync } from "./engine.js";
export type { CommandResult, Engine, FamilyLedger, Observed, UpdateResult } from "./engine.js";
export type { DocumentHost } from "./host.js";
export { History } from "./history.js";
export { Library } from "./library.js";
export type { LibraryOptions, MissingFile, OpenedDocument } from "./library.js";
export { Lineage, LineageError, userMessage } from "./lineage.js";
export type { LineageOptions, Opened, Returning } from "./lineage.js";
export * as schemas from "./schemas.js";
export type {
  Candidate,
  CardDecision,
  ChangeCard,
  CheckpointCard,
  Choice,
  CopyOf,
  DeckPanel,
  Decision,
  DocumentPage,
  DocumentView,
  Family,
  FamilyGraph,
  FileHistory,
  GraphEdge,
  GraphNode,
  Ids,
  KeepFormatting,
  Lane,
  LibraryEntry,
  MasterPanel,
  MatchedBy,
  Node,
  OpenFile,
  OverlapCard,
  Recognition,
  ReturnReview,
  ReturnedRow,
  Segment,
  SendArgs,
  SentList,
  SentRow,
  TimelineEntry,
  UpdateRun,
  Updates,
  Verdict,
  VersionChoice,
  VersionPanel,
  VersionRow,
  WhatChanged,
  Written,
} from "./schemas.js";
export {
  BadBlob,
  FileChanged,
  FileLocked,
  FileExists,
  MemoryStorage,
  OFFICE_FILE,
  NotFound,
  StorageConflict,
  StorageError,
  checkFamily,
  checkFileName,
  checkHash,
  folderFor,
  sha256Hex,
} from "./storage.js";
export type { FileKind, FileLocation, LedgerVersion, Storage, StoredFile } from "./storage.js";
