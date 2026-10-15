# lineage-wasm

wasm-bindgen bindings that expose `lineage-core` and `lineage` to JavaScript. The TypeScript wrapper lives in `packages/shell`.

The bridge is bytes in, bytes out. TypeScript never parses or edits document XML.

Build it through the workspace script, which also checks the wasm-bindgen CLI version:

```bash
pnpm build:wasm
```

`wasm-bindgen` is pinned exactly (`=0.2.129`) in the workspace `Cargo.toml`. The CLI must be the same version:

```bash
cargo install --locked wasm-bindgen-cli --version 0.2.129
```
