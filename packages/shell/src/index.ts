// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

export { EngineBoundaryError, EngineError, loadEngine, loadEngineAsync } from "./engine.js";
export type {
  CommandResult,
  Engine,
  FamilyLedger,
  Observed,
  OpenDeckResult,
  UpdateResult,
} from "./engine.js";
export type { CellHost, CellNavigator, DocumentHost, SlideHost } from "./host.js";
export { PlanStopped, changesCells, changesSlides, goesToCells, reportsEdits } from "./host.js";
export { History } from "./history.js";
export type { AsyncEngine, EnginePort } from "./worker.js";
export { asyncEngine, connectEngine, serveEngine } from "./worker.js";
export { Library } from "./library.js";
export type { LibraryOptions, MissingFile, OpenedDocument } from "./library.js";
export { Lineage, LineageError, userMessage } from "./lineage.js";
export type { LineageOptions, Opened, Returning } from "./lineage.js";
export * as schemas from "./schemas.js";
export type {
  Candidate,
  CardDecision,
  Category,
  CellDetail,
  CellSide,
  CellStep,
  CellWrite,
  ChangeCard,
  CheckpointCard,
  CodeLine,
  Choice,
  CopyOf,
  DeckPanel,
  Decision,
  DocumentPage,
  DocumentView,
  Family,
  FieldLists,
  FamilyGraph,
  FileHistory,
  GraphEdge,
  GraphNode,
  Hint,
  Ids,
  KeepFormatting,
  Lane,
  LibraryEntry,
  MasterPanel,
  MatchedBy,
  Node,
  OpenFile,
  OverlapCard,
  PivotFields,
  PutBack,
  Recognition,
  ReturnReview,
  ReturnedRow,
  Segment,
  SendArgs,
  SentList,
  SentRow,
  SideBySide,
  SlideOp,
  SlideOps,
  TimelineEntry,
  UpdateRun,
  Updates,
  Verdict,
  VersionChoice,
  VersionPanel,
  VersionRow,
  VersionRun,
  WhatChanged,
  WorkbookCard,
  WorkbookPlan,
  WorkbookView,
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
