# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# Local smoke test (Windows with Microsoft PowerPoint installed): opens every
# .pptx in a folder read-only, without a window, and reports what PowerPoint
# sees. A file PowerPoint can't open (or would have to repair) is reported as
# FAILED and the script exits with code 1. PowerPoint's alerts are off, so a
# file that needs repairing fails to open instead of asking.
#
# With -Compare, every file that has <name>.powerpoint.txt next to it (written
# by the engine tests when LINEAGE_SMOKE_DIR is set) is also checked: per slide,
# its number, whether it is hidden, and the non-empty paragraphs of its text
# shapes must be what the engine wrote. A mismatch prints the first differing
# line; add -Private for personal decks, which prints its number only.
#
# PowerPoint has one instance per user: if PowerPoint is already open, this
# attaches to it. Close your own presentations first. The script only closes
# the presentations it opened, and quits PowerPoint only if it started it.
#
# Usage: powershell -NoProfile -File scripts/powerpoint-smoke.ps1 [folder] [-Compare] [-Private]

param(
  [string]$Folder = (Join-Path $PSScriptRoot "..\fixtures\pptx\synthetic"),
  [switch]$Compare,
  [switch]$Private
)

$msoTrue = -1
$msoFalse = 0
$ppAlertsNone = 1

# Text shapes: placeholders (14) and text boxes (17), and autoshapes (1) that have text.
function Get-Lines($pres) {
  $lines = @()
  $n = 0
  foreach ($slide in $pres.Slides) {
    $n += 1
    $hidden = if ($slide.SlideShowTransition.Hidden -eq $msoTrue) { " (hidden)" } else { "" }
    $lines += "# Slide $n$hidden"
    foreach ($shape in $slide.Shapes) {
      if (@(1, 14, 17) -notcontains [int]$shape.Type) { continue }
      if ($shape.HasTextFrame -ne $msoTrue) { continue }
      $range = $shape.TextFrame2.TextRange
      for ($i = 1; $i -le $range.Paragraphs().Count; $i++) {
        $text = [string]$range.Paragraphs($i).Text
        $text = $text.TrimEnd([char]13, [char]10)
        if ($text.Trim().Length -gt 0) { $lines += $text }
      }
    }
  }
  return ,$lines
}

$running = $null -ne (Get-Process POWERPNT -ErrorAction SilentlyContinue)
$app = New-Object -ComObject PowerPoint.Application
$app.DisplayAlerts = $ppAlertsNone
$failed = 0
try {
  foreach ($file in Get-ChildItem -Path $Folder -Filter *.pptx | Sort-Object Name) {
    $pres = $null
    try {
      $pres = $app.Presentations.Open($file.FullName, $msoTrue, $msoFalse, $msoFalse)
      $slides = $pres.Slides.Count
      $status = "ok  slides=$slides"
      $expectedFile = [System.IO.Path]::ChangeExtension($file.FullName, ".powerpoint.txt")
      if ($Compare -and (Test-Path $expectedFile)) {
        $got = Get-Lines $pres
        $want = @()
        $text = [System.IO.File]::ReadAllText($expectedFile, [System.Text.Encoding]::UTF8)
        if ($text.Length -gt 0) { $want = $text.Split("`n") }
        $mismatch = $null
        if ($got.Count -ne $want.Count) {
          $mismatch = "PowerPoint has $($got.Count) lines, expected $($want.Count)"
        }
        for ($i = 0; $null -eq $mismatch -and $i -lt $want.Count; $i++) {
          if ($got[$i] -ne $want[$i]) {
            $mismatch = if ($Private) { "line $i" } else { "line ${i}: PowerPoint '$($got[$i])' expected '$($want[$i])'" }
          }
        }
        if ($mismatch) {
          $status = "MISMATCH  $mismatch"
          $failed += 1
        } else {
          $status += "  text matches"
        }
      }
      Write-Host ("{0}: {1}" -f $file.Name, $status)
    } catch {
      Write-Host ("{0}: FAILED  {1}" -f $file.Name, $_.Exception.Message)
      $failed += 1
    } finally {
      if ($null -ne $pres) { $pres.Close() }
    }
  }
} finally {
  if (-not $running) { $app.Quit() }
  [System.Runtime.InteropServices.Marshal]::ReleaseComObject($app) | Out-Null
}
if ($failed -gt 0) { exit 1 }
