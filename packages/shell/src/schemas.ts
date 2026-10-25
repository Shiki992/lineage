// Copyright 2026 Hrishikesh Vedantam
// SPDX-License-Identifier: Apache-2.0

// The shapes that cross the wasm boundary, mirroring crates/wasm/src/dto.rs.
// Every value the engine returns is checked against these before the rest of the
// TypeScript code sees it, and the golden replies in test/golden (written by
// crates/wasm/tests/golden.rs) must all pass, so the two sides can't drift apart
// unnoticed (decision 0010). Objects are strict: an unknown field is an error.

import * as v from "valibot";

export const Id = v.pipe(v.string(), v.regex(/^[0-9a-f]{32}$/, "an id is 32 hex digits"));
export const BlobHash = v.pipe(v.string(), v.regex(/^[0-9a-f]{64}$/, "a hash is 64 hex digits"));
const Text = v.string();
const Count = v.pipe(v.number(), v.integer(), v.minValue(0));

export const Ids = v.strictObject({ family: Id, checkpoint: Id });
export type Ids = v.InferOutput<typeof Ids>;

export const Role = v.picklist(["master", "variant", "workingCopy"]);
export type Role = v.InferOutput<typeof Role>;

export const Decision = v.picklist([
  "bringIn",
  "decline",
  "keepMine",
  "useMasters",
  "keepBothMineFirst",
  "keepBothMastersFirst",
]);
export type Decision = v.InferOutput<typeof Decision>;

export const CheckpointCard = v.strictObject({ id: Id, label: Text, note: Text, when: Text });
export type CheckpointCard = v.InferOutput<typeof CheckpointCard>;

export const Outcome = v.strictObject({
  ledgerLines: Text,
  records: Count,
  blobs: v.array(BlobHash),
  ids: v.nullable(Ids),
  hasFile: v.boolean(),
  newFileName: v.nullable(Text),
  created: v.nullable(Id),
  nothingChanged: v.boolean(),
  familyId: v.nullable(Id),
});
export type Outcome = v.InferOutput<typeof Outcome>;

export const Node = v.strictObject({
  id: Id,
  role: Role,
  displayName: Text,
  storageRef: Text,
  followsMaster: v.boolean(),
  archived: v.boolean(),
  latest: v.nullable(CheckpointCard),
});
export type Node = v.InferOutput<typeof Node>;

export const Family = v.strictObject({
  familyId: Id,
  name: Text,
  format: Text,
  nodes: v.array(Node),
});
export type Family = v.InferOutput<typeof Family>;

export const CopyOf = v.strictObject({ sentCopyId: Id, recipient: Text, date: Text });
export type CopyOf = v.InferOutput<typeof CopyOf>;

export const OpenFile = v.strictObject({
  familyId: Id,
  nodeId: Id,
  role: Role,
  displayName: Text,
  storageRef: Text,
  format: Text,
  checkpointLabel: Text,
  isLatest: v.boolean(),
  /** Set when the open file is a copy that was sent and came back. */
  copyOf: v.nullable(CopyOf),
});
export type OpenFile = v.InferOutput<typeof OpenFile>;

export const Callout = v.strictObject({ text: Text, button: Text, versions: v.array(Id) });
export type Callout = v.InferOutput<typeof Callout>;

export const VersionRow = v.strictObject({
  id: Id,
  name: Text,
  status: Text,
  button: Text,
  storageRef: Text,
});
export type VersionRow = v.InferOutput<typeof VersionRow>;

/** A file that came back, with changes still to review. */
export const ReturnedRow = v.strictObject({ id: Id, text: Text, detail: Text, button: Text });
export type ReturnedRow = v.InferOutput<typeof ReturnedRow>;

export const MasterPanel = v.strictObject({
  nodeId: Id,
  fileName: Text,
  chip: Text,
  follows: Text,
  latest: CheckpointCard,
  callout: v.nullable(Callout),
  returned: v.array(ReturnedRow),
  versions: v.array(VersionRow),
  actions: v.array(Text),
  footer: Text,
});
export type MasterPanel = v.InferOutput<typeof MasterPanel>;

export const VersionPanel = v.strictObject({
  nodeId: Id,
  fileName: Text,
  chip: Text,
  status: Text,
  latest: CheckpointCard,
  updates: v.nullable(Text),
  returned: v.array(ReturnedRow),
  actions: v.array(Text),
  footer: Text,
});
export type VersionPanel = v.InferOutput<typeof VersionPanel>;

export const Segment = v.strictObject({
  kind: v.picklist(["same", "removed", "added", "reformatted"]),
  text: Text,
});
export type Segment = v.InferOutput<typeof Segment>;

export const Choice = v.strictObject({ decision: Decision, label: Text });
export type Choice = v.InferOutput<typeof Choice>;

export const ChangeCard = v.strictObject({
  id: v.pipe(Text, v.minLength(1)),
  summary: Text,
  place: v.nullable(Text),
  text: v.array(Segment),
  choices: v.array(Choice),
  byHand: v.nullable(Text),
});
export type ChangeCard = v.InferOutput<typeof ChangeCard>;

export const OverlapCard = v.strictObject({
  id: v.pipe(Text, v.minLength(1)),
  summary: Text,
  place: v.nullable(Text),
  before: v.nullable(Text),
  mine: v.array(Segment),
  masters: v.array(Segment),
  choices: v.array(Choice),
});
export type OverlapCard = v.InferOutput<typeof OverlapCard>;

// --- Workbooks (decisions 0022, 0023) ----------------------------------------

/** "B4", "AA10": one cell, no sheet. */
const CellAddress = v.pipe(Text, v.regex(/^[A-Z]{1,3}[1-9][0-9]{0,6}$/, "a cell address"));
const SheetName = v.pipe(Text, v.minLength(1), v.maxLength(255));

export const CellSide = v.strictObject({ shown: Text, formula: v.nullable(Text) });
export type CellSide = v.InferOutput<typeof CellSide>;

/** One line of a macro's or query's code view (Phase 5B). Line numbers from 1. */
export const CodeLine = v.strictObject({
  kind: v.picklist(["same", "removed", "added", "skipped"]),
  before: v.nullable(v.pipe(v.number(), v.integer(), v.minValue(1))),
  after: v.nullable(v.pipe(v.number(), v.integer(), v.minValue(1))),
  text: Text,
});
export type CodeLine = v.InferOutput<typeof CodeLine>;

/** A pivot table's fields, as the PivotTable Fields pane lists them. */
export const PivotFields = v.strictObject({
  rows: v.array(Text),
  columns: v.array(Text),
  values: v.array(Text),
  filters: v.array(Text),
});
export type PivotFields = v.InferOutput<typeof PivotFields>;

export const FieldLists = v.strictObject({
  before: v.nullable(PivotFields),
  after: v.nullable(PivotFields),
});
export type FieldLists = v.InferOutput<typeof FieldLists>;

export const WorkbookCard = v.strictObject({
  id: v.pipe(Text, v.minLength(1)),
  summary: Text,
  place: v.nullable(Text),
  sheet: Text,
  address: v.nullable(Text),
  before: v.nullable(CellSide),
  after: v.nullable(CellSide),
  cells: Count,
  why: v.nullable(Text),
  ids: v.array(v.pipe(Text, v.minLength(1))),
  choices: v.array(Choice),
  byHand: v.nullable(Text),
  mine: v.nullable(CellSide),
  conflict: v.nullable(Text),
  code: v.array(CodeLine),
  fields: v.nullable(FieldLists),
  details: v.array(Text),
  /** A macro's or query's whole code, for "Copy code" (Phase 5B). */
  copy: v.nullable(Text),
});
export type WorkbookCard = v.InferOutput<typeof WorkbookCard>;

export const Category = v.picklist(["input", "formula", "knockOn", "refreshed", "structure"]);
export type Category = v.InferOutput<typeof Category>;

export const WorkbookView = v.strictObject({
  headline: Text,
  groups: v.array(
    v.strictObject({
      category: Category,
      object: v.nullable(v.picklist(["macros", "queries", "pivots"])),
      title: Text,
      summary: Text,
      cards: v.array(WorkbookCard),
      more: Count,
    }),
  ),
  unmatched: v.array(v.strictObject({ sheet: Text, summary: Text })),
  untracked: v.array(Text),
  notes: v.array(Text),
  /** Macro code is new or changed: for a file that came back, the warning (decision 0027). */
  newMacroCode: v.boolean(),
  signature: v.nullable(Text),
});
export type WorkbookView = v.InferOutput<typeof WorkbookView>;

/** What one step writes into a cell. Numbers by their lexeme, text exactly. */
export const CellWrite = v.variant("kind", [
  v.strictObject({ kind: v.literal("clear") }),
  v.strictObject({ kind: v.literal("number"), text: v.pipe(Text, v.minLength(1)) }),
  v.strictObject({ kind: v.literal("text"), text: Text }),
  v.strictObject({ kind: v.literal("bool"), value: v.boolean() }),
  v.strictObject({ kind: v.literal("error"), text: v.pipe(Text, v.startsWith("#")) }),
  v.strictObject({ kind: v.literal("formula"), text: v.pipe(Text, v.startsWith("=")) }),
]);
export type CellWrite = v.InferOutput<typeof CellWrite>;

const Row = v.pipe(v.number(), v.integer(), v.minValue(1), v.maxValue(1_048_576));
const Rows = v.pipe(Text, v.regex(/^[1-9][0-9]{0,6}:[1-9][0-9]{0,6}$/, "a row range"));

/** One step of an apply plan, at the addresses the workbook has when it runs. */
export const CellStep = v.variant("kind", [
  v.strictObject({
    kind: v.literal("insertRows"),
    sheet: SheetName,
    at: Row,
    count: Row,
    rows: Rows,
  }),
  v.strictObject({
    kind: v.literal("deleteRows"),
    sheet: SheetName,
    at: Row,
    count: Row,
    rows: Rows,
  }),
  v.strictObject({
    kind: v.literal("set"),
    sheet: SheetName,
    address: CellAddress,
    value: CellWrite,
  }),
]);
export type CellStep = v.InferOutput<typeof CellStep>;

export const WorkbookPlan = v.strictObject({
  steps: v.array(CellStep),
  undo: v.array(CellStep),
  expect: v.array(v.strictObject({ sheet: SheetName, address: CellAddress, value: CellWrite })),
  skipped: v.array(v.strictObject({ id: Text, reason: Text })),
  brought: Count,
  deferred: Count,
  text: Text,
});
export type WorkbookPlan = v.InferOutput<typeof WorkbookPlan>;

export const CellDetail = v.strictObject({
  address: CellAddress,
  shown: Text,
  formula: v.nullable(Text),
  value: Text,
  kind: v.picklist(["number", "text", "true or false", "error", "date", "empty"]),
  format: v.nullable(Text),
});
export type CellDetail = v.InferOutput<typeof CellDetail>;

export const SideBySide = v.strictObject({
  sheet: Text,
  before: v.nullable(CellDetail),
  after: v.nullable(CellDetail),
});
export type SideBySide = v.InferOutput<typeof SideBySide>;

/** The way back worked: exactly, or with the person's own typing kept. */
export const PutBack = v.strictObject({ theirsKept: v.boolean() });
export type PutBack = v.InferOutput<typeof PutBack>;

/**
 * A range Excel reported as edited in this session, not by Lineage, and
 * whose edit it was: the event's `source` (decision 0032). Left out, the
 * person's own.
 */
export const Hint = v.strictObject({
  sheet: SheetName,
  address: v.pipe(Text, v.regex(/^[A-Z]{1,3}[1-9][0-9]{0,6}(:[A-Z]{1,3}[1-9][0-9]{0,6})?$/)),
  by: v.optional(v.picklist(["you", "coAuthor"])),
});
export type Hint = v.InferOutput<typeof Hint>;

export const WhatChanged = v.strictObject({
  headline: Text,
  changes: v.array(ChangeCard),
  formatting: v.array(ChangeCard),
  overlaps: v.array(OverlapCard),
  byHand: v.array(ChangeCard),
  alreadyThere: v.array(ChangeCard),
  /** A workbook's changes in the five categories; absent for documents and decks. */
  workbook: v.optional(WorkbookView),
});
export type WhatChanged = v.InferOutput<typeof WhatChanged>;

/** The pitch deck panel (BLUEPRINT screen 4, decision 0018). */
export const DeckPanel = v.strictObject({
  master: MasterPanel,
  justChanged: v.nullable(
    v.strictObject({ headline: Text, cards: v.array(ChangeCard), unsaved: v.boolean() }),
  ),
  targets: v.array(
    v.strictObject({ id: Id, name: Text, checked: v.boolean(), context: Text, storageRef: Text }),
  ),
  button: v.nullable(Text),
  lastSent: v.nullable(v.strictObject({ sent: Id, text: Text, button: Text })),
});
export type DeckPanel = v.InferOutput<typeof DeckPanel>;

export const RunResult = v.picklist(["updated", "upToDate", "needsChoice", "notUpdated"]);
export type RunResult = v.InferOutput<typeof RunResult>;

/** One version's line after "Update N versions", or Bring in on an open deck. */
export const VersionRun = v.strictObject({
  node: Id,
  name: Text,
  result: RunResult,
  text: Text,
  broughtIn: v.pipe(v.number(), v.integer(), v.minValue(0)),
  savedEdits: v.boolean(),
  left: WhatChanged,
  hasFile: v.boolean(),
  storageRef: Text,
});
export type VersionRun = v.InferOutput<typeof VersionRun>;

/** "Update N versions": what to append, and a line per version. */
export const UpdateRun = v.strictObject({
  outcome: Outcome,
  headline: Text,
  versions: v.array(VersionRun),
});
export type UpdateRun = v.InferOutput<typeof UpdateRun>;

/** A slide as Office.js names it: "<sldId>#<creationId>" (decision 0019). */
export const SlideName = v.pipe(v.string(), v.regex(/^\d{1,10}#\d{1,10}$/));

/** One step PowerPoint takes on the open deck. */
export const SlideOp = v.variant("kind", [
  v.strictObject({ kind: v.literal("delete"), slide: SlideName }),
  v.strictObject({ kind: v.literal("insert"), slide: SlideName, after: v.nullable(SlideName) }),
  v.strictObject({
    kind: v.literal("move"),
    slide: SlideName,
    index: v.pipe(v.number(), v.integer(), v.minValue(0)),
  }),
]);
export type SlideOp = v.InferOutput<typeof SlideOp>;

/** The steps from the open deck to another, and the slide order they lead to. */
export const SlideOps = v.strictObject({ ops: v.array(SlideOp), order: v.array(SlideName) });
export type SlideOps = v.InferOutput<typeof SlideOps>;

/** Bring in on an open deck, first half: its line, and the steps to the merged deck. */
export const OpenDeckPlan = v.strictObject({
  version: VersionRun,
  steps: SlideOps,
  hasFile: v.boolean(),
  /** Something to record even when no slide changes ("Keep mine" on everything). */
  records: v.boolean(),
});
export type OpenDeckPlan = v.InferOutput<typeof OpenDeckPlan>;

export const Updates = v.strictObject({
  headline: Text,
  groups: v.array(v.strictObject({ checkpoint: Id, label: Text, view: WhatChanged })),
});
export type Updates = v.InferOutput<typeof Updates>;

/** What a sharing link lets its holder do (decision 0032). */
export const LinkAccess = v.picklist(["view", "edit"]);
export type LinkAccess = v.InferOutput<typeof LinkAccess>;

const LinkUrl = v.pipe(Text, v.maxLength(2048), v.startsWith("https://"));

/** A sent copy's sharing link, as "What I've sent" shows it. */
export const SentLink = v.strictObject({
  /** "Shared as a link · Can edit", or "Link stopped 21 Oct". */
  line: Text,
  /** "Anyone with the link can open it". */
  who: Text,
  url: LinkUrl,
  access: LinkAccess,
  live: v.boolean(),
  /** The storage's id for the link, and the shared file, to stop it. */
  id: v.pipe(Text, v.minLength(1), v.maxLength(2048)),
  fileRef: v.pipe(Text, v.minLength(1), v.maxLength(2048)),
});
export type SentLink = v.InferOutput<typeof SentLink>;

export const SentRow = v.strictObject({
  id: Id,
  recipient: Text,
  version: Text,
  purpose: v.nullable(Text),
  format: Text,
  date: Text,
  chip: v.nullable(Text),
  /** The latest file that came back from this copy. */
  returned: v.nullable(Id),
  button: Text,
  link: v.nullable(SentLink),
  /**
   * While an edit link works: the shared file, and the SHA-256 of every
   * version of it Lineage has seen. Bytes there with another hash are the
   * recipients' edits.
   */
  shared: v.nullable(
    v.strictObject({ fileRef: v.pipe(Text, v.maxLength(2048)), known: v.array(BlobHash) }),
  ),
});
export type SentRow = v.InferOutput<typeof SentRow>;

export const SentList = v.strictObject({
  title: Text,
  groups: v.array(v.strictObject({ month: Text, rows: v.array(SentRow) })),
  note: Text,
});
export type SentList = v.InferOutput<typeof SentList>;

export const Verdict = v.picklist(["certain", "choose", "unknown"]);
export type Verdict = v.InferOutput<typeof Verdict>;

/** One copy a returned file could be. */
export const Candidate = v.strictObject({
  family: Id,
  checkpoint: Id,
  sent: v.nullable(Id),
  /** The file the copy was made from, and its name. */
  node: Id,
  nodeName: Text,
  title: Text,
  detail: Text,
  score: Text,
});
export type Candidate = v.InferOutput<typeof Candidate>;

/** "Which copy is this?" */
export const Recognition = v.strictObject({
  verdict: Verdict,
  byIds: v.boolean(),
  headline: Text,
  note: v.nullable(Text),
  candidates: v.array(Candidate),
});
export type Recognition = v.InferOutput<typeof Recognition>;

/** The review of a file that came back. */
/** Whether kept changes bring only their words, and why (decision 0014). */
export const KeepFormatting = v.strictObject({
  wordsOnly: v.boolean(),
  note: v.nullable(Text),
  formattingNote: v.nullable(Text),
});
export type KeepFormatting = v.InferOutput<typeof KeepFormatting>;

export const ReturnReview = v.strictObject({
  returned: Id,
  headline: Text,
  from: Text,
  notes: v.array(Text),
  theirs: Text,
  fromNode: Id,
  fromName: Text,
  view: WhatChanged,
  formatting: KeepFormatting,
});
export type ReturnReview = v.InferOutput<typeof ReturnReview>;

export const Written = v.strictObject({
  ok: v.boolean(),
  sameAccepted: v.boolean(),
  sameRejected: v.boolean(),
  lineageExpected: Count,
  lineageFound: Count,
  otherAdded: Count,
});
export type Written = v.InferOutput<typeof Written>;

// My documents (decision 0015).

const Millis = v.pipe(v.number(), v.integer());

export const LibraryEntry = v.strictObject({
  familyId: Id,
  name: Text,
  format: Text,
  kind: Text,
  masterFile: v.nullable(Text),
  tailored: Count,
  checkpoints: Count,
  sent: Count,
  returned: Count,
  toReview: Count,
  lastAt: Millis,
  summary: Text,
  lastActivity: Text,
  attention: v.nullable(Text),
  canOpen: v.boolean(),
  damage: v.nullable(Text),
  /** "Contains macros". */
  badges: v.array(Text),
});
export type LibraryEntry = v.InferOutput<typeof LibraryEntry>;

export const Library = v.strictObject({ entries: v.array(LibraryEntry), empty: v.nullable(Text) });
export type Library = v.InferOutput<typeof Library>;

export const Refreshed = v.strictObject({ library: Library, indexChanged: v.boolean() });
export type Refreshed = v.InferOutput<typeof Refreshed>;

export const TimelineEntry = v.strictObject({
  id: Id,
  label: Text,
  note: Text,
  date: Text,
  milestone: v.nullable(Text),
  author: v.nullable(Text),
  sent: v.nullable(Text),
});
export type TimelineEntry = v.InferOutput<typeof TimelineEntry>;

export const FileHistory = v.strictObject({
  node: Id,
  name: Text,
  role: Role,
  chip: Text,
  status: Text,
  archived: v.boolean(),
  storageRef: Text,
  entries: v.array(TimelineEntry),
});
export type FileHistory = v.InferOutput<typeof FileHistory>;

export const DocumentPage = v.strictObject({
  familyId: Id,
  name: Text,
  format: Text,
  kind: Text,
  canChange: v.boolean(),
  note: v.nullable(Text),
  files: v.array(FileHistory),
  sent: SentList,
  badges: v.array(Text),
});
export type DocumentPage = v.InferOutput<typeof DocumentPage>;

export const LaneKind = v.picklist(["master", "version", "sent", "returned"]);
export type LaneKind = v.InferOutput<typeof LaneKind>;
export const NodeKind = v.picklist(["checkpoint", "sent", "returned"]);
export type NodeKind = v.InferOutput<typeof NodeKind>;
export const EdgeKind = v.picklist([
  "next",
  "madeFrom",
  "broughtIn",
  "detected",
  "restored",
  "sent",
  "cameBack",
  "keptInto",
]);
export type EdgeKind = v.InferOutput<typeof EdgeKind>;

export const Lane = v.strictObject({
  id: Text,
  kind: LaneKind,
  title: Text,
  chip: v.nullable(Text),
  archived: v.boolean(),
});
export type Lane = v.InferOutput<typeof Lane>;

export const GraphNode = v.strictObject({
  id: Id,
  lane: Text,
  kind: NodeKind,
  label: Text,
  detail: Text,
  at: Millis,
  when: Text,
  milestone: v.nullable(Text),
  archived: v.boolean(),
});
export type GraphNode = v.InferOutput<typeof GraphNode>;

export const GraphEdge = v.strictObject({
  from: Id,
  to: Id,
  kind: EdgeKind,
  label: v.nullable(Text),
});
export type GraphEdge = v.InferOutput<typeof GraphEdge>;

export const FamilyGraph = v.strictObject({
  lanes: v.array(Lane),
  nodes: v.array(GraphNode),
  edges: v.array(GraphEdge),
});
export type FamilyGraph = v.InferOutput<typeof FamilyGraph>;

export const DocumentView = v.strictObject({ page: DocumentPage, graph: FamilyGraph });
export type DocumentView = v.InferOutput<typeof DocumentView>;

export const ErrorKind = v.picklist([
  "engine",
  "ledger",
  "missingBlobs",
  "wrongFile",
  "notAllowed",
  "badInput",
]);
export type ErrorKind = v.InferOutput<typeof ErrorKind>;

/** The envelope every engine call returns. */
export const Envelope = v.variant("ok", [
  v.strictObject({ ok: v.literal(true), value: v.unknown() }),
  v.strictObject({
    ok: v.literal(false),
    error: v.strictObject({
      kind: ErrorKind,
      userMessage: v.pipe(Text, v.minLength(1)),
      detail: Text,
      hashes: v.optional(v.array(BlobHash)),
      family: v.optional(Id),
    }),
  }),
]);
export type Envelope = v.InferOutput<typeof Envelope>;

// Inputs, checked before they cross (the engine checks them again).

export const CardDecision = v.strictObject({
  card: v.pipe(Text, v.minLength(1)),
  decision: Decision,
});
export type CardDecision = v.InferOutput<typeof CardDecision>;

/** A decision on a card a previous "Update N versions" left, for one version. */
export const VersionChoice = v.strictObject({
  node: Id,
  card: v.pipe(Text, v.minLength(1)),
  decision: Decision,
});
export type VersionChoice = v.InferOutput<typeof VersionChoice>;

export const SendArgs = v.strictObject({
  recipient: v.pipe(Text, v.trim(), v.minLength(1), v.maxLength(200)),
  purpose: v.nullable(v.pipe(Text, v.maxLength(200))),
  embedId: v.boolean(),
});
export type SendArgs = v.InferOutput<typeof SendArgs>;

/** "Share a link": what the storage made. */
export const LinkArgs = v.strictObject({
  id: v.pipe(Text, v.minLength(1), v.maxLength(2048)),
  access: LinkAccess,
  url: LinkUrl,
  fileRef: v.pipe(Text, v.minLength(1), v.maxLength(2048)),
  scope: v.pipe(Text, v.minLength(1), v.maxLength(64)),
});
export type LinkArgs = v.InferOutput<typeof LinkArgs>;

export const MatchedBy = v.picklist(["ids", "content", "person"]);
export type MatchedBy = v.InferOutput<typeof MatchedBy>;

export const RecordArgs = v.strictObject({
  name: v.pipe(Text, v.trim(), v.maxLength(200)),
  checkpoint: Id,
  sent: v.nullable(Id),
  matchedBy: MatchedBy,
});
export type RecordArgs = v.InferOutput<typeof RecordArgs>;

/** A milestone label: up to 60 characters; null takes the mark off. */
export const MilestoneLabel = v.nullable(v.pipe(Text, v.trim(), v.maxLength(60)));

/** The value schema each golden reply's `value` must match, by file name. */
export const goldenValues: Readonly<Record<string, v.GenericSchema>> = {
  "start-tracking": Outcome,
  "make-tailored-version": Outcome,
  "save-checkpoint": Outcome,
  "offer-to-versions": Outcome,
  decide: Outcome,
  "send-copy": Outcome,
  "read-ids": Ids,
  "read-ids-untracked": v.null(),
  family: Family,
  "open-file": OpenFile,
  "master-panel-new": MasterPanel,
  "master-panel-callout": MasterPanel,
  "master-panel-offered": MasterPanel,
  "version-panel": VersionPanel,
  "version-panel-updates": VersionPanel,
  "what-changed": WhatChanged,
  updates: Updates,
  "sent-list": SentList,
  "sent-list-shared": SentList,
  "record-link": Outcome,
  "stop-link": Outcome,
  "sent-list-empty": SentList,
  "check-written": Written,
  "check-written-failed": Written,
  "referenced-blobs": v.array(BlobHash),
  "open-file-copy": OpenFile,
  "recognise-certain": Recognition,
  "recognise-choose": Recognition,
  "recognise-unknown": Recognition,
  "record-returned": Outcome,
  "version-panel-returned": VersionPanel,
  "sent-list-returned": SentList,
  "returned-review": ReturnReview,
  "returned-review-words-only": ReturnReview,
  "decide-returned": Outcome,
  "stale-families": v.array(Id),
  "refresh-library": Refreshed,
  "library-from-index": Library,
  "library-from-index-untrusted": v.null(),
  document: DocumentView,
  "document-damaged": DocumentView,
  compare: WhatChanged,
  "restore-as-new-version": Outcome,
  "mark-milestone": Outcome,
  relink: Outcome,
  "send-copy-deck": Outcome,
  "deck-panel": DeckPanel,
  "update-versions": UpdateRun,
  "prepare-open-deck": OpenDeckPlan,
  "deck-slide-ops": SlideOps,
  "finish-open-deck": Outcome,
  "updates-deck": Updates,
  "goes-with-copy": v.array(Text),
  "send-copy-workbook": Outcome,
  "workbook-what-changed": WhatChanged,
  "workbook-what-changed-co-author": WhatChanged,
  "cell-side-by-side": SideBySide,
  "updates-workbook": Updates,
  "prepare-workbook-offer": WorkbookPlan,
  "finish-workbook-offer": Outcome,
  "returned-review-workbook": ReturnReview,
  "prepare-workbook-keep": WorkbookPlan,
  "finish-workbook-keep": Outcome,
  "workbook-way-back": v.array(CellStep),
  "workbook-put-back": PutBack,
  "workbook-what-changed-objects": WhatChanged,
  "returned-review-macros": ReturnReview,
  "sent-list-macros": SentList,
  "document-macros": DocumentView,
};
