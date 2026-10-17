#!/usr/bin/env bash
# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# Linux and macOS counterpart of libreoffice-resave.ps1. Makes the "re-saved by
# LibreOffice" fixtures for returned files (Phase 3): writes the synthetic inputs
# with `cargo xtask returned-inputs`, then has LibreOffice open each one and save
# it again as .docx, the way a person would who received a copy and edited it in
# LibreOffice Writer. Outputs go to fixtures/docx/returned/libreoffice/ and are
# committed; the inputs are thrown away.
#
# Development only. Needs LibreOffice (apt install libreoffice-writer, or
# https://www.libreoffice.org/). Record the version it prints in
# fixtures/docx/returned/README.md when the outputs change.
#
# Usage: bash scripts/libreoffice-resave.sh [path-to-soffice]

set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
soffice="${1:-$(command -v soffice || command -v libreoffice || true)}"
if [ -z "$soffice" ] && [ -x /Applications/LibreOffice.app/Contents/MacOS/soffice ]; then
  soffice=/Applications/LibreOffice.app/Contents/MacOS/soffice
fi
[ -n "$soffice" ] && [ -x "$soffice" ] || {
  echo "LibreOffice not found. Install it, or pass the path to soffice." >&2
  exit 1
}

out="$root/fixtures/docx/returned/libreoffice"
work="$(mktemp -d)"
inputs="$work/inputs"
# A profile of its own, so the person's LibreOffice settings and recent files are untouched.
profile="$work/profile"
trap 'rm -rf "$work"' EXIT

(cd "$root" && cargo xtask returned-inputs "$inputs")

mkdir -p "$out"
echo "Using $("$soffice" --version | head -n 1)"

for file in "$inputs"/*.docx; do
  name="$(basename "$file")"
  "$soffice" "-env:UserInstallation=file://$profile" --headless --norestore \
    --convert-to "docx:MS Word 2007 XML" --outdir "$out" "$file" >/dev/null
  [ -f "$out/$name" ] || { echo "LibreOffice could not re-save $name" >&2; exit 1; }
  echo "re-saved $name"
done

echo "Outputs in $out. Note the version above in fixtures/docx/returned/README.md."
