# 0001: No command line for users

Date: 2026-10-15. Status: accepted.

## Context

The first plan had a Rust CLI (`crates/cli`) as a developer harness, and Phase 1 was proved through CLI commands such as `lineage offer`. The blueprint canvas also shows "Command line and git drivers" as an optional box. Lineage is for people who never want to see git, and the maintainer decided that nothing should depend on a terminal.

## Decision

- There is no user-facing command line and no git drivers. Every action is a button in the panel.
- What changed is shown in the panel, with removed and added text marked (labelled "what changed", never "diff"). The person decides each change, and both sides of every overlap.
- `crates/harness` replaces `crates/cli`. It is test-only and never shipped. It holds local-folder and in-memory storage, exposes one function per panel button, and runs end-to-end scenario tests.
- Computation stays in Rust. TypeScript draws the UI and calls platform APIs only.

## Alternatives

- Keep a CLI for developers only. Rejected: it pulls design effort towards terminal flows and invites user-facing scripting.
- Prove Phase 1 through the add-in only. Rejected: the engine must be testable without Office running.

## Consequences

- Phase 1 "done" is a scenario test, not a CLI demo.
- Storage adapters in TypeScript must match the harness's storage interface.
- The canvas's "Command line and git drivers" box is out of scope.
