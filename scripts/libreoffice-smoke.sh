#!/usr/bin/env bash
# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# Linux and macOS smoke test, the counterpart of word-smoke.ps1 where Word isn't
# available: LibreOffice opens every .docx in a folder headless and converts it to
# plain text. A file LibreOffice can't open or convert is reported as FAILED and
# the script exits with code 1.
#
# This checks that each file opens. It does not do word-smoke.ps1's -Compare
# (accept and reject all revisions, then compare the text), which needs Word.
#
# Usage: bash scripts/libreoffice-smoke.sh [folder] [path-to-soffice]

set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
folder="${1:-$root/fixtures/docx/synthetic}"
soffice="${2:-$(command -v soffice || command -v libreoffice || true)}"
if [ -z "$soffice" ] && [ -x /Applications/LibreOffice.app/Contents/MacOS/soffice ]; then
  soffice=/Applications/LibreOffice.app/Contents/MacOS/soffice
fi
[ -n "$soffice" ] && [ -x "$soffice" ] || {
  echo "LibreOffice not found. Install it, or pass the path to soffice as the second argument." >&2
  exit 1
}

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
profile="$work/profile"

echo "Using $("$soffice" --version | head -n 1)"
failed=0
shopt -s nullglob
for file in "$folder"/*.docx; do
  name="$(basename "$file" .docx)"
  if timeout 120 "$soffice" "-env:UserInstallation=file://$profile" --headless --norestore \
       --convert-to txt:Text --outdir "$work" "$file" >/dev/null 2>&1 \
     && [ -f "$work/$name.txt" ]; then
    echo "$name.docx: ok  lines=$(wc -l < "$work/$name.txt")"
  else
    echo "$name.docx: FAILED"
    failed=$((failed + 1))
  fi
done

[ "$failed" -eq 0 ] || exit 1
