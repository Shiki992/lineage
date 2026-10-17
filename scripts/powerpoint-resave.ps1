# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# Makes the real-PowerPoint deck fixtures in fixtures/pptx/powerpoint/ from the
# synthetic pitch deck (invented content only): the deck saved by PowerPoint
# unchanged, and the edits people make in PowerPoint (a duplicated slide, a new
# slide, a slide reused from another deck, a moved slide, a hidden slide,
# changed text, new speaker notes, a comment). The engine's
# tests read them to check that what PowerPoint writes is matched the way the
# synthetic decks are (crates/core/tests/powerpoint_fixtures.rs).
#
# SmartArt isn't made here: PowerPoint's SmartArtLayouts collection comes back
# empty through COM when PowerPoint has no window.
#
# PowerPoint has one instance per user: if it is already open, this attaches to
# it. It only closes the presentations it opened, and quits PowerPoint only if
# it started it.
#
# Usage: powershell -NoProfile -File scripts/powerpoint-resave.ps1

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$source = Join-Path $root "fixtures\pptx\synthetic\pitch-deck.pptx"
$northgate = Join-Path $root "fixtures\pptx\synthetic\pitch-deck-northgate.pptx"
$out = Join-Path $root "fixtures\pptx\powerpoint"
New-Item -ItemType Directory -Force $out | Out-Null

$msoTrue = -1
$msoFalse = 0
$ppAlertsNone = 1
$ppSaveAsOpenXMLPresentation = 24

$running = $null -ne (Get-Process POWERPNT -ErrorAction SilentlyContinue)
$app = New-Object -ComObject PowerPoint.Application
$app.DisplayAlerts = $ppAlertsNone

function Make($name, [scriptblock]$edit) {
  $pres = $app.Presentations.Open($source, $msoFalse, $msoTrue, $msoFalse)
  try {
    & $edit $pres
    $target = Join-Path $out $name
    $pres.SaveAs($target, $ppSaveAsOpenXMLPresentation)
    Write-Host "wrote fixtures/pptx/powerpoint/$name"
  } finally {
    $pres.Close()
  }
}

function Replace-Text($pres, $slide, $old, $new) {
  foreach ($shape in $pres.Slides($slide).Shapes) {
    if ($shape.HasTextFrame -eq $msoTrue) {
      $shape.TextFrame.TextRange.Replace($old, $new) | Out-Null
    }
  }
}

try {
  Make "pitch-deck-saved.pptx" { param($p) }
  Make "pitch-deck-duplicated.pptx" { param($p) $p.Slides(6).Duplicate() | Out-Null }
  Make "pitch-deck-new-slide.pptx" {
    param($p)
    $slide = $p.Slides.AddSlide(8, $p.Slides(2).CustomLayout)
    $slide.Shapes(1).TextFrame.TextRange.Text = "Pricing"
    $slide.Shapes(2).TextFrame.TextRange.Text = "1,800 dollars per month per lender"
  }
  Make "pitch-deck-reused.pptx" { param($p) $p.Slides.InsertFromFile($northgate, 7, 8, 8) | Out-Null }
  Make "pitch-deck-moved.pptx" { param($p) $p.Slides(4).MoveTo(6) }
  Make "pitch-deck-hidden.pptx" { param($p) $p.Slides(8).SlideShowTransition.Hidden = $msoTrue }
  Make "pitch-deck-text-edited.pptx" { param($p) Replace-Text $p 6 '$1.2M ARR' '$2.0M ARR' }
  Make "pitch-deck-notes.pptx" {
    param($p)
    $p.Slides(2).NotesPage.Shapes.Placeholders(2).TextFrame.TextRange.Text = "Lead with the nine-day close."
  }
  Make "pitch-deck-comment.pptx" {
    param($p)
    $p.Slides(6).Comments.Add(10, 10, "Jordan Vale", "JV", "Is Q4 booked or signed?") | Out-Null
  }
} finally {
  if (-not $running) { $app.Quit() }
  [System.Runtime.InteropServices.Marshal]::ReleaseComObject($app) | Out-Null
}
