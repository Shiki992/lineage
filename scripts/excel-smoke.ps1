# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# Opens workbooks in real Excel (through COM) and reports whether Excel had to
# repair any of them. Used on the synthetic fixtures and on every workbook the
# engine writes. Invented content only: never point it at real workbooks.
#
# Excel writes a repair log (error*.xml) to %TEMP% when it repairs a file on
# open, even with alerts off, so a new log means the file was damaged.
#
# This starts its own Excel instance (Excel.Application always does through
# COM), opens each file read-only with links and macros off, and quits only
# that instance. Excel windows the maintainer has open are never touched.
#
# Usage: powershell -NoProfile -File scripts/excel-smoke.ps1 <file or folder> [...]

param([Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)][string[]]$Paths)

$ErrorActionPreference = "Stop"
$files = foreach ($path in $Paths) {
  if (Test-Path $path -PathType Container) {
    Get-ChildItem $path -File | Where-Object { $_.Extension -in ".xlsx", ".xlsm" } | ForEach-Object { $_.FullName }
  } else {
    (Resolve-Path $path).Path
  }
}

$msoAutomationSecurityForceDisable = 3
$app = New-Object -ComObject Excel.Application
$app.Visible = $false
$app.DisplayAlerts = $false
$app.AutomationSecurity = $msoAutomationSecurityForceDisable
$failed = 0
try {
  foreach ($file in $files) {
    $before = @(Get-ChildItem $env:TEMP -Filter "error*.xml" -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })
    try {
      # Open(Filename, UpdateLinks = 0, ReadOnly = true)
      $wb = $app.Workbooks.Open($file, 0, $true)
      $sheets = @($wb.Worksheets | ForEach-Object { $_.Name }) -join ", "
      $wb.Close($false)
      $after = @(Get-ChildItem $env:TEMP -Filter "error*.xml" -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })
      $logs = @($after | Where-Object { $before -notcontains $_ })
      if ($logs.Count -gt 0) {
        $failed++
        Write-Host "REPAIRED $file"
        foreach ($log in $logs) { Get-Content $log | Write-Host }
      } else {
        Write-Host "ok       $(Split-Path $file -Leaf): $sheets"
      }
    } catch {
      $failed++
      Write-Host "FAILED   $file : $($_.Exception.Message)"
    }
  }
} finally {
  $app.Quit()
  [System.Runtime.InteropServices.Marshal]::ReleaseComObject($app) | Out-Null
}
if ($failed -gt 0) { exit 1 }
