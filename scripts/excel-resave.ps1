# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# Makes the real-Excel workbook fixtures in fixtures/xlsx/excel/ from the
# synthetic workbooks (invented content only): each saved by Excel unchanged,
# and the edits people make in Excel (a changed input, an inserted row, a
# filled-down formula change, a sorted table, a new table row, a filter). The
# engine's tests read them to check that what Excel writes is read and
# matched the way the synthetic workbooks are
# (crates/core/tests/excel_fixtures.rs).
#
# This starts its own Excel instance (Excel.Application always does through
# COM) and quits only that one. Excel windows the maintainer has open are
# never touched. Data connections are never refreshed.
#
# Usage: powershell -NoProfile -File scripts/excel-resave.ps1

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$synthetic = Join-Path $root "fixtures\xlsx\synthetic"
$out = Join-Path $root "fixtures\xlsx\excel"
New-Item -ItemType Directory -Force $out | Out-Null

$xlOpenXMLWorkbook = 51
$xlDescending = 2
$xlAscending = 1
$xlYes = 1
$xlShiftDown = -4121
$msoAutomationSecurityForceDisable = 3

$app = New-Object -ComObject Excel.Application
$app.Visible = $false
$app.DisplayAlerts = $false
$app.AutomationSecurity = $msoAutomationSecurityForceDisable

Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$utf8 = New-Object System.Text.UTF8Encoding($false)

# Excel stamps who saved the file, where it was saved, and this computer's
# sensitivity label (with the organisation's tenant id) into every workbook.
# None of that may be committed, so it is taken out again; the sheets stay
# exactly as Excel wrote them.
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

function Make($source, $name, [scriptblock]$edit) {
  # Open(Filename, UpdateLinks = 0, ReadOnly = true)
  $wb = $app.Workbooks.Open((Join-Path $synthetic $source), 0, $true)
  try {
    & $edit $wb
    $app.CalculateFull()
    $target = Join-Path $out $name
    if (Test-Path $target) { Remove-Item $target }
    $wb.SaveAs($target, $xlOpenXMLWorkbook)
  } finally {
    $wb.Close($false)
  }
  Scrub $target
  Write-Host "wrote fixtures/xlsx/excel/$name"
}

try {
  Make "model.xlsx" "model-saved.xlsx" { param($wb) }
  Make "model.xlsx" "model-price-55.xlsx" {
    param($wb)
    $wb.Worksheets.Item("Assumptions").Range("B4").Value2 = 55
  }
  Make "model.xlsx" "model-row-inserted.xlsx" {
    param($wb)
    # A new assumption above "Price per seat": every formula and name that
    # refers below it moves down one row.
    $sheet = $wb.Worksheets.Item("Assumptions")
    $sheet.Rows(4).Insert($xlShiftDown) | Out-Null
    $sheet.Range("A4").Value2 = "Annual discount"
    $sheet.Range("B4").Value2 = 0.05
  }
  Make "model.xlsx" "model-cogs-formula.xlsx" {
    param($wb)
    # Hosting added to COGS, filled down the column, as people do.
    $sheet = $wb.Worksheets.Item("Revenue")
    $sheet.Range("D2").Formula = "=C2*CogsShare+250"
    $sheet.Range("D2").AutoFill($sheet.Range("D2:D25")) | Out-Null
  }
  Make "budget.xlsx" "budget-saved.xlsx" { param($wb) }
  Make "budget.xlsx" "budget-sorted.xlsx" {
    param($wb)
    $table = $wb.Worksheets.Item("Transactions").ListObjects.Item("Transactions")
    $table.Sort.SortFields.Clear()
    $table.Sort.SortFields.Add($table.ListColumns.Item("Amount").Range, 0, $xlDescending) | Out-Null
    $table.Sort.Header = $xlYes
    $table.Sort.Apply()
  }
  Make "budget.xlsx" "budget-row-added.xlsx" {
    param($wb)
    $table = $wb.Worksheets.Item("Transactions").ListObjects.Item("Transactions")
    $row = $table.ListRows.Add()
    $row.Range.Cells.Item(1, 1).Value2 = 46082
    $row.Range.Cells.Item(1, 2).Value2 = "Rent"
    $row.Range.Cells.Item(1, 3).Value2 = "March rent"
    $row.Range.Cells.Item(1, 4).Value2 = 1450
  }
  Make "tracker.xlsx" "tracker-filtered.xlsx" {
    param($wb)
    $sheet = $wb.Worksheets.Item("Applications")
    $sheet.Range("A1:F9").AutoFilter(5, "<>Rejected") | Out-Null
  }
  Make "tracker.xlsx" "tracker-sorted.xlsx" {
    param($wb)
    $sheet = $wb.Worksheets.Item("Applications")
    $sheet.AutoFilterMode = $false
    $sheet.Range("A1:F9").Sort($sheet.Range("B1"), $xlAscending, $null, $null, $xlAscending, $null, $xlAscending, $xlYes) | Out-Null
  }
  Make "orders-1000.xlsx" "orders-sorted.xlsx" {
    param($wb)
    $sheet = $wb.Worksheets.Item("Orders")
    $sheet.Range("A1:F1001").Sort($sheet.Range("B1"), $xlAscending, $null, $null, $xlAscending, $null, $xlAscending, $xlYes) | Out-Null
  }
  Make "awkward-cells.xlsx" "awkward-saved.xlsx" { param($wb) }
  Make "rates.xlsx" "rates-saved.xlsx" { param($wb) }
} finally {
  $app.Quit()
  [System.Runtime.InteropServices.Marshal]::ReleaseComObject($app) | Out-Null
}
