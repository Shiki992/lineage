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

export const WhatChanged = v.strictObject({
  headline: Text,
  changes: v.array(ChangeCard),
  formatting: v.array(ChangeCard),
  overlaps: v.array(OverlapCard),
  byHand: v.array(ChangeCard),
  alreadyThere: v.array(ChangeCard),
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
};
