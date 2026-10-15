# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# Makes the "re-saved by LibreOffice" fixtures for returned files (Phase 3):
# writes the synthetic inputs with `cargo xtask returned-inputs`, then has
# LibreOffice open each one and save it again as .docx, the way a person would
# who received a copy and edited it in LibreOffice Writer. The outputs go to
# fixtures/docx/returned/libreoffice/ and are committed; the inputs are thrown away.
#
# Development only. Needs LibreOffice (https://www.libreoffice.org/), for example
# `winget install TheDocumentFoundation.LibreOffice`. Record the version it prints
# in fixtures/docx/returned/README.md when the outputs change.
#
# Usage: powershell -NoProfile -File scripts/libreoffice-resave.ps1 [-Soffice <path>]

param(
  [string]$Soffice = "$env:ProgramFiles\LibreOffice\program\soffice.com"
)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$out = Join-Path $root "fixtures\docx\returned\libreoffice"
$inputs = Join-Path ([System.IO.Path]::GetTempPath()) "lineage-libreoffice-inputs"
# A profile of its own, so the person's LibreOffice settings and recent files are untouched.
$profileDir = Join-Path ([System.IO.Path]::GetTempPath()) "lineage-libreoffice-profile"

if (-not (Test-Path $Soffice)) {
  Write-Error "LibreOffice not found at $Soffice. Install it, or pass -Soffice <path to soffice.com>."
}

Remove-Item -Recurse -Force $inputs -ErrorAction SilentlyContinue
& cargo xtask returned-inputs $inputs
if ($LASTEXITCODE -ne 0) { exit 1 }

New-Item -ItemType Directory -Force $out | Out-Null
$version = (& $Soffice --version | Select-Object -First 1)
Write-Host "Using $version"

$profileUrl = "file:///" + ($profileDir -replace "\\", "/")
foreach ($file in Get-ChildItem $inputs -Filter *.docx) {
  & $Soffice "-env:UserInstallation=$profileUrl" --headless --norestore `
    --convert-to "docx:MS Word 2007 XML" --outdir $out $file.FullName | Out-Null
  if ($LASTEXITCODE -ne 0 -or -not (Test-Path (Join-Path $out $file.Name))) {
    Write-Error "LibreOffice could not re-save $($file.Name)"
  }
  Write-Host "re-saved $($file.Name)"
}

Remove-Item -Recurse -Force $inputs, $profileDir -ErrorAction SilentlyContinue
Write-Host "Outputs in $out. Note the version above in fixtures/docx/returned/README.md."
