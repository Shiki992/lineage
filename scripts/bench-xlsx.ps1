# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# Runs `cargo xtask bench-xlsx` one case at a time (release build) and adds
# each run's peak memory, as Windows records it for the process (peak working
# set), for the Phase 5 budgets in decision 0025. Invented workbooks only.
#
# Usage: powershell -NoProfile -File scripts/bench-xlsx.ps1

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
Push-Location $root
try {
  cargo build --release -p xtask --quiet
  $exe = Join-Path $root "target\release\xtask.exe"
  & $exe bench-xlsx prepare
  $cases = "model-price", "stores-60", "orders-50k-edit", "orders-50k-sorted", "orders-200k-edit", "orders-200k-sorted"
  foreach ($case in $cases) {
    $out = Join-Path $env:TEMP "lineage-bench-$case.txt"
    $p = Start-Process -FilePath $exe -ArgumentList "bench-xlsx", $case -NoNewWindow -PassThru -RedirectStandardOutput $out
    $peak = 0
    while (-not $p.HasExited) {
      try { $p.Refresh(); if ($p.PeakWorkingSet64 -gt $peak) { $peak = $p.PeakWorkingSet64 } } catch { }
      Start-Sleep -Milliseconds 20
    }
    $line = (Get-Content $out -Raw).Trim()
    Write-Host ("{0}`tpeak {1} MB" -f $line, [math]::Round($peak / 1MB))
  }
} finally {
  Pop-Location
}
