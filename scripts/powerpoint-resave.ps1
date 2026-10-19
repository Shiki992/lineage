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
#        powershell -NoProfile -File scripts/powerpoint-resave.ps1 -ScrubOnly
#        (scrubs the decks already in fixtures/pptx/powerpoint/ without
#        opening PowerPoint)

param([switch]$ScrubOnly)

$ErrorActionPreference = "Stop"
$root = Resolve-Path (Join-Path $PSScriptRoot "..")
$source = Join-Path $root "fixtures\pptx\synthetic\pitch-deck.pptx"
$northgate = Join-Path $root "fixtures\pptx\synthetic\pitch-deck-northgate.pptx"
$out = Join-Path $root "fixtures\pptx\powerpoint"
New-Item -ItemType Directory -Force $out | Out-Null

Add-Type -AssemblyName System.IO.Compression, System.IO.Compression.FileSystem
$utf8 = New-Object System.Text.UTF8Encoding($false)

# Rewrites one part through $edit, only if that changes it. The part keeps its
# timestamp and any byte order mark; every other part is left as it was.
function Edit-Part($zip, $name, [scriptblock]$edit) {
  $entry = $zip.GetEntry($name)
  if ($null -eq $entry) { return }
  $stream = $entry.Open()
  $buffer = New-Object System.IO.MemoryStream
  $stream.CopyTo($buffer)
  $stream.Close()
  $bytes = $buffer.ToArray()
  $bom = $bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF
  $skip = if ($bom) { 3 } else { 0 }
  $text = $utf8.GetString($bytes, $skip, $bytes.Length - $skip)
  $new = & $edit $text
  if ($new -ceq $text) { return }
  $stamp = $entry.LastWriteTime
  $entry.Delete()
  $entry = $zip.CreateEntry($name)
  $entry.LastWriteTime = $stamp
  $stream = $entry.Open()
  if ($bom) { $stream.Write([byte[]](0xEF, 0xBB, 0xBF), 0, 3) }
  $body = $utf8.GetBytes($new)
  $stream.Write($body, 0, $body.Length)
  $stream.Close()
}

# PowerPoint stamps who saved the deck, and this computer's sensitivity label
# (with the organisation's tenant id), into every deck. A comment is signed
# with the account PowerPoint is signed in to, whatever author the script
# passes. None of that may be committed, so it is taken out again; the slides
# stay exactly as PowerPoint wrote them.
function Scrub($path) {
  $zip = [System.IO.Compression.ZipFile]::Open($path, "Update")
  try {
    $label = $zip.GetEntry("docMetadata/LabelInfo.xml")
    if ($null -ne $label) { $label.Delete() }
    Edit-Part $zip "[Content_Types].xml" { param($t) $t -replace '<Override PartName="/docMetadata/LabelInfo.xml"[^>]*/>', "" }
    Edit-Part $zip "_rels/.rels" { param($t) $t -replace '<Relationship [^>]*Target="docMetadata/LabelInfo.xml"[^>]*/>', "" }
    Edit-Part $zip "docProps/core.xml" { param($t) $t -replace "<cp:lastModifiedBy>[^<]*</cp:lastModifiedBy>", "<cp:lastModifiedBy>Lineage fixture generator</cp:lastModifiedBy>" }
    Edit-Part $zip "docProps/custom.xml" { param($t) $t -replace '<property [^>]*name="MSIP_Label_[^"]*"[^>]*>.*?</property>', "" }

    # Comment authors: an invented name, and an invented id in place of the
    # one PowerPoint derives from the account, in the author list and in
    # every comment that points to it.
    $authors = $zip.GetEntry("ppt/authors.xml")
    if ($null -ne $authors) {
      $reader = New-Object System.IO.StreamReader($authors.Open(), $utf8)
      $ids = [regex]::Matches($reader.ReadToEnd(), '<p188:author id="(\{[^"]*\})"') | ForEach-Object { $_.Groups[1].Value }
      $reader.Close()
      $n = 0
      $map = @{}
      foreach ($id in $ids) { $n++; $map[$id] = "{{00000000-0000-4000-8000-{0:D12}}}" -f $n }
      $rename = {
        param($t)
        foreach ($id in $map.Keys) { $t = $t.Replace($id, $map[$id]) }
        $t
      }
      Edit-Part $zip "ppt/authors.xml" {
        param($t)
        $t = $t -replace ' name="[^"]*"', ' name="Jordan Vale"' -replace ' initials="[^"]*"', ' initials="JV"'
        $t = $t -replace ' userId="[^"]*"', ' userId="Jordan Vale"' -replace ' providerId="[^"]*"', ' providerId="None"'
        & $rename $t
      }
      $comments = @($zip.Entries | Where-Object { $_.FullName -like "ppt/comments/*.xml" } | ForEach-Object { $_.FullName })
      foreach ($name in $comments) { Edit-Part $zip $name $rename }
    }
  } finally {
    $zip.Dispose()
  }
}

if ($ScrubOnly) {
  foreach ($deck in Get-ChildItem $out -Filter *.pptx) {
    Scrub $deck.FullName
    Write-Host "scrubbed fixtures/pptx/powerpoint/$($deck.Name)"
  }
  return
}

$msoTrue = -1
$msoFalse = 0
$ppAlertsNone = 1
$ppSaveAsOpenXMLPresentation = 24

$running = $null -ne (Get-Process POWERPNT -ErrorAction SilentlyContinue)
$app = New-Object -ComObject PowerPoint.Application
$app.DisplayAlerts = $ppAlertsNone

function Make($name, [scriptblock]$edit) {
  $pres = $app.Presentations.Open($source, $msoFalse, $msoTrue, $msoFalse)
  $target = Join-Path $out $name
  try {
    & $edit $pres
    $pres.SaveAs($target, $ppSaveAsOpenXMLPresentation)
  } finally {
    $pres.Close()
  }
  Scrub $target
  Write-Host "wrote fixtures/pptx/powerpoint/$name"
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
