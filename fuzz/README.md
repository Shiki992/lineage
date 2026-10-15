# Fuzzing

cargo-fuzz targets for the readers. Returned files are untrusted input (CLAUDE.md), so every parser has a target.

| Target        | What it feeds                                                                       |
| ------------- | ----------------------------------------------------------------------------------- |
| `docx_reader` | Any bytes into `Docx::open` (both views), then diff with itself and a clean copy    |
| `zip_reader`  | Any bytes into `Package::read`; whatever is accepted must write back and read again |

This folder is its own Cargo workspace: `libfuzzer-sys` needs a nightly toolchain and is never shipped. CI runs each target for a short time on Linux (`.github/workflows/ci.yml`, job "Fuzz").

```bash
rustup toolchain install nightly
cargo install cargo-fuzz --locked
mkdir -p fuzz/corpus/docx_reader && cp fixtures/docx/synthetic/*.docx fuzz/corpus/docx_reader/
cargo +nightly fuzz run docx_reader -- -max_total_time=60
```

On Windows, libFuzzer needs the MSVC AddressSanitizer runtime; Linux (or WSL) is simpler. The property test `the_reader_never_panics_on_damaged_files` in `crates/core/tests/properties.rs` covers the same ground on the stable toolchain in every `cargo test`.
