# lineage-wasm

wasm-bindgen bindings that expose `lineage-core` and `lineage` to JavaScript. The TypeScript side lives in `packages/shell`. Decision 0010 describes the boundary.

- `src/api.rs`: `Session`, one method per panel button and view, in plain Rust and tested natively. Every method returns a `Reply`: a JSON envelope (`{ok, value}` or `{ok: false, error: {kind, userMessage, detail}}`) with bytes beside it.
- `src/dto.rs`: what crosses, as camelCase JSON. Inputs refuse unknown fields and check every id.
- `src/cache.rs`: fetched blobs, checked against their hashes and capped at 64 MB. A missing part reports the rest of its snapshot too.
- `src/lib.rs`: the `#[wasm_bindgen]` `Engine` and `Reply` classes, thin wrappers around `api.rs`.
- `tests/golden.rs`: writes a golden reply of every kind to `packages/shell/test/golden/` and fails on drift. After a deliberate change, run it with `LINEAGE_UPDATE_GOLDEN=1`, update `packages/shell/src/schemas.ts` to match, and commit both.

The bridge is bytes in, bytes out. TypeScript never parses or edits document XML.

Build it through the workspace script, which checks the wasm-bindgen CLI version, uses the size-tuned `wasm` Cargo profile and drops the name section:

```bash
pnpm build:wasm
```

`wasm-bindgen` is pinned exactly (`=0.2.129`) in the workspace `Cargo.toml`. The CLI must be the same version:

```bash
cargo install --locked wasm-bindgen-cli --version 0.2.129
```
