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

export const OpenFile = v.strictObject({
  familyId: Id,
  nodeId: Id,
  role: Role,
  displayName: Text,
  storageRef: Text,
  checkpointLabel: Text,
  isLatest: v.boolean(),
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

export const MasterPanel = v.strictObject({
  nodeId: Id,
  fileName: Text,
  chip: Text,
  follows: Text,
  latest: CheckpointCard,
  callout: v.nullable(Callout),
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
  button: Text,
});
export type SentRow = v.InferOutput<typeof SentRow>;

export const SentList = v.strictObject({
  title: Text,
  groups: v.array(v.strictObject({ month: Text, rows: v.array(SentRow) })),
  note: Text,
});
export type SentList = v.InferOutput<typeof SentList>;

export const Written = v.strictObject({
  ok: v.boolean(),
  sameAccepted: v.boolean(),
  sameRejected: v.boolean(),
  lineageExpected: Count,
  lineageFound: Count,
  otherAdded: Count,
});
export type Written = v.InferOutput<typeof Written>;

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

export const SendArgs = v.strictObject({
  recipient: v.pipe(Text, v.trim(), v.minLength(1), v.maxLength(200)),
  purpose: v.nullable(v.pipe(Text, v.maxLength(200))),
  embedId: v.boolean(),
});
export type SendArgs = v.InferOutput<typeof SendArgs>;

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
};
