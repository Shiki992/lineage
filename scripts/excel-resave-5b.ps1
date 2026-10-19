# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# Phase 5B, stop 1: real Excel opens and saves the synthetic workbooks with
# macros, pivot tables and Power Query (invented content only), and makes
# the pivot edits people make in Excel. Writes fixtures/xlsx/excel/ and a
# report of what Excel did to each file.
#
# Nothing in a file is ever run:
# - macros are force-disabled for this Excel instance (AutomationSecurity),
#   and no macro is called; the VBA project is never opened through COM, and
#   no Trust Center setting is read or changed;
# - Power Query queries are never refreshed (that would evaluate M);
# - only pivot caches built on a range in the same workbook are refreshed,
#   which reads cells, not code.
#
# The VBA spike (decision 0028): model-macros-only.xlsm holds a project in
# the spec's on-write form (version 0xFFFF, no performance cache). If Excel
# accepts it, the project it saves keeps every module's code, and Lineage's
# test (crates/core/tests/excel_fixtures.rs) checks that.
#
# Excel writes a repair log (error*.xml in %TEMP%) when it has to fix a
# file; any new one is reported, with its text.
#
# This starts its own Excel instance (Excel.Application always does through
# COM) and quits only that one. Excel windows the maintainer has open are
# never touched.
#
# Usage: powershell -NoProfile -File scripts/excel-resave-5b.ps1

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$synthetic = Join-Path $root "fixtures\xlsx\synthetic"
$out = Join-Path $root "fixtures\xlsx\excel"
New-Item -ItemType Directory -Force $out | Out-Null

$xlOpenXMLWorkbook = 51
$xlOpenXMLWorkbookMacroEnabled = 52
$xlRowField = 1
$xlHidden = 0
$msoAutomationSecurityForceDisable = 3

$app = New-Object -ComObject Excel.Application
$app.Visible = $false
$app.DisplayAlerts = $false
$app.AutomationSecurity = $msoAutomationSecurityForceDisable
$app.EnableEvents = $false

Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$utf8 = New-Object System.Text.UTF8Encoding($false)

# As in excel-resave.ps1: take out who saved the file, where, and this
# computer's sensitivity label. Pivot caches also name who refreshed them.
function Scrub($path) {
  $zip = [System.IO.Compression.ZipFile]::Open($path, "Update")
  try {
    $edits = @{
      "docProps/core.xml" = { param($t) $t -replace "<cp:lastModifiedBy>[^<]*</cp:lastModifiedBy>", "<cp:lastModifiedBy>Lineage fixture generator</cp:lastModifiedBy>" }
      "xl/workbook.xml" = { param($t) $t -replace '<mc:AlternateContent[^>]*><mc:Choice Requires="x15"><x15ac:absPath[^>]*/></mc:Choice></mc:AlternateContent>', "" }
      "[Content_Types].xml" = { param($t) $t -replace '<Override PartName="/docMetadata/LabelInfo.xml"[^>]*/>', "" }
      "_rels/.rels" = { param($t) $t -replace '<Relationship [^>]*Target="docMetadata/LabelInfo.xml"[^>]*/>', "" }
    }
    $label = $zip.GetEntry("docMetadata/LabelInfo.xml")
    if ($null -ne $label) { $label.Delete() }
    $caches = @($zip.Entries | Where-Object { $_.FullName -like "xl/pivotCache/pivotCacheDefinition*.xml" } | ForEach-Object { $_.FullName })
    foreach ($c in $caches) {
      $edits[$c] = { param($t) $t -replace 'refreshedBy="[^"]*"', 'refreshedBy="Lineage fixture generator"' }
    }
    foreach ($name in $edits.Keys) {
      $entry = $zip.GetEntry($name)
      if ($null -eq $entry) { continue }
      $reader = New-Object System.IO.StreamReader($entry.Open(), $utf8)
      $text = $reader.ReadToEnd()
      $reader.Close()
      $entry.Delete()
      $new = $zip.CreateEntry($name)
      $writer = New-Object System.IO.StreamWriter($new.Open(), $utf8)
      $writer.Write((& $edits[$name] $text))
      $writer.Close()
    }
  } finally {
    $zip.Dispose()
  }
}

$report = New-Object System.Collections.Generic.List[string]

function Make($source, $name, [scriptblock]$edit) {
  $before = @(Get-ChildItem -Path $env:TEMP -Filter "error*.xml" -ErrorAction SilentlyContinue | ForEach-Object { $_.FullName })
  $format = if ($name.EndsWith(".xlsm")) { $xlOpenXMLWorkbookMacroEnabled } else { $xlOpenXMLWorkbook }
  # Open(Filename, UpdateLinks = 0, ReadOnly = true)
  try {
    $wb = $app.Workbooks.Open((Join-Path $synthetic $source), 0, $true)
  } catch {
    $report.Add("$name : Excel refused $source : $($_.Exception.Message)")
    Write-Host "Excel refused $source"
    return
  }
  try {
    & $edit $wb
    $target = Join-Path $out $name
    if (Test-Path $target) { Remove-Item $target }
    $wb.SaveAs($target, $format)
  } finally {
    $wb.Close($false)
  }
  Scrub $target
  $after = @(Get-ChildItem -Path $env:TEMP -Filter "error*.xml" -ErrorAction SilentlyContinue | Where-Object { $before -notcontains $_.FullName })
  if ($after.Count -gt 0) {
    foreach ($log in $after) {
      $report.Add("$name : Excel repaired $source : " + ((Get-Content $log.FullName -Raw) -replace "\s+", " "))
    }
  } else {
    $report.Add("$name : opened and saved without repair")
  }
  Write-Host "wrote fixtures/xlsx/excel/$name"
}

try {
  # The spike's file: macros only, no form.
  Make "model-macros-only.xlsm" "model-macros-saved.xlsm" { param($wb) }
  Make "model-pivots-only.xlsx" "model-pivots-saved.xlsx" { param($wb) }
  Make "model-queries-only.xlsx" "model-queries-saved.xlsx" { param($wb) }
  Make "model-full-for-excel.xlsm" "model-full-saved.xlsm" { param($wb) }
  # Excel's own pivot edits, from the saved full model's source.
  Make "model-full-for-excel.xlsm" "model-full-pivot-rows-excel.xlsm" {
    param($wb)
    $pt = $wb.Worksheets.Item("Pivots").PivotTables("SeatsByPlan")
    $pt.PivotFields("Plan").Orientation = $xlHidden
    $pt.PivotFields("Month").Orientation = $xlRowField
  }
  Make "model-full-for-excel.xlsm" "model-full-refreshed-excel.xlsm" {
    param($wb)
    # A sale corrected on Sales, then the pivots refreshed from that range.
    $wb.Worksheets.Item("Sales").Range("E13").Value2 = 420
    $wb.Worksheets.Item("Pivots").PivotTables("RevenueByRegion").PivotCache().Refresh()
  }
  Make "model-full-for-excel.xlsm" "model-full-slicer-excel.xlsm" {
    param($wb)
    $cache = $wb.SlicerCaches.Item("Slicer_Region")
    $cache.SlicerItems.Item("South").Selected = $false
  }
} finally {
  $app.Quit()
  [System.Runtime.InteropServices.Marshal]::ReleaseComObject($app) | Out-Null
}

$reportPath = Join-Path $env:TEMP "lineage-phase-5b-resave-report.txt"
[System.IO.File]::WriteAllLines($reportPath, $report, $utf8)
$report | ForEach-Object { Write-Host $_ }
