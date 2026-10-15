# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# Local smoke test (Windows with Microsoft Word installed): opens every .docx in a
# folder read-only in a hidden Word instance and reports what Word sees. Any file
# Word cannot open is reported as FAILED and the script exits with code 1.
# Used instead of the LibreOffice headless check when LibreOffice isn't installed.
#
# With -Compare, every file that has <name>.accepted.txt and <name>.rejected.txt
# next to it (written by the engine tests when LINEAGE_SMOKE_DIR is set) is also
# checked: Word accepts all revisions and its paragraph text must equal the
# .accepted.txt lines, then Word rejects all revisions and its text must equal the
# .rejected.txt lines. A mismatch prints the first differing line; add -Private
# for personal documents, which prints its number only.
#
# Usage: powershell -NoProfile -File scripts/word-smoke.ps1 [folder] [-Compare] [-Private]

param(
  [string]$Folder = (Join-Path $PSScriptRoot "..\fixtures\docx\synthetic"),
  [switch]$Compare,
  # With personal documents: report mismatching line numbers only, never text.
  [switch]$Private
)

# Word's paragraph text, normalised to what the engine's model shows: paragraph
# and cell marks removed, end-of-row marks skipped, Word's own codes for
# non-breaking and soft hyphens mapped to the Unicode characters, and page-break,
# picture and note-reference placeholders dropped. Manual line breaks stay as
# U+000B, which is how the tests write them too.
function Get-Lines($doc) {
  $lines = @()
  foreach ($p in $doc.Paragraphs) {
    # Each table row ends with an end-of-row mark: in a table (12 =
    # wdWithInTable) but in no cell. It is not a paragraph of the document.
    if ($p.Range.Information(12) -and $p.Range.Cells.Count -eq 0) { continue }
    $text = [string]$p.Range.Text
    # Pictures show as a placeholder character in the text; drop each one by its
    # position (last first), so real characters are never touched.
    $starts = @()
    foreach ($shape in $p.Range.InlineShapes) { $starts += $shape.Range.Start - $p.Range.Start }
    foreach ($at in ($starts | Sort-Object -Descending)) {
      if ($at -ge 0 -and $at -lt $text.Length) { $text = $text.Remove($at, 1) }
    }
    $text = $text.TrimEnd([char]13, [char]7)
    $text = $text.Replace([string][char]30, [string][char]0x2011).Replace([string][char]31, [string][char]0x00AD)
    # 12: page break; 1: inline picture; 2: footnote or endnote reference mark.
    $text = $text.Replace([string][char]12, "").Replace([string][char]1, "").Replace([string][char]2, "")
    $lines += $text
  }
  return ,$lines
}

function Compare-Lines($name, $label, $got, $expectedFile) {
  $expected = [System.IO.File]::ReadAllText($expectedFile, [System.Text.Encoding]::UTF8)
  $want = @()
  if ($expected.Length -gt 0) { $want = $expected.Split("`n") }
  # Word always shows a final empty paragraph after a table that ends the document.
  if ($got.Count -eq $want.Count + 1 -and $got[$got.Count - 1] -eq "") { $got = $got[0..($got.Count - 2)] }
  if ($got.Count -ne $want.Count) {
    Write-Host ("{0}: {1} MISMATCH  Word has {2} paragraphs, expected {3}" -f $name, $label, $got.Count, $want.Count)
    return $false
  }
  for ($i = 0; $i -lt $want.Count; $i++) {
    if ($got[$i] -ne $want[$i]) {
      if ($Private) {
        Write-Host ("{0}: {1} MISMATCH  line {2}" -f $name, $label, $i)
      } else {
        Write-Host ("{0}: {1} MISMATCH  line {2}: Word '{3}' expected '{4}'" -f $name, $label, $i, $got[$i], $want[$i])
      }
      return $false
    }
  }
  return $true
}

$word = New-Object -ComObject Word.Application
$word.Visible = $false
$word.DisplayAlerts = 0
$failed = 0
try {
  foreach ($file in Get-ChildItem $Folder -Filter *.docx | Sort-Object Name) {
    try {
      # Open(FileName, ConfirmConversions, ReadOnly, AddToRecentFiles)
      $doc = $word.Documents.Open($file.FullName, $false, $true, $false)
      "{0}: ok  paragraphs={1} tables={2} footnotes={3} comments={4} revisions={5} sections={6} images={7} fields={8}" -f `
        $file.Name, $doc.Paragraphs.Count, $doc.Tables.Count, $doc.Footnotes.Count, $doc.Comments.Count, `
        $doc.Revisions.Count, $doc.Sections.Count, $doc.InlineShapes.Count, $doc.Fields.Count
      $base = Join-Path $file.DirectoryName $file.BaseName
      if ($Compare -and (Test-Path "$base.accepted.txt") -and (Test-Path "$base.rejected.txt")) {
        $doc.Revisions.AcceptAll()
        $accepted = Compare-Lines $file.Name "accepted" (Get-Lines $doc) "$base.accepted.txt"
        $doc.Close([ref]0)
        $doc = $word.Documents.Open($file.FullName, $false, $true, $false)
        $doc.Revisions.RejectAll()
        $rejected = Compare-Lines $file.Name "rejected" (Get-Lines $doc) "$base.rejected.txt"
        if ($accepted -eq $true -and $rejected -eq $true) {
          "{0}: accept and reject match" -f $file.Name
        } else {
          $failed++
        }
      }
      $doc.Close([ref]0)
    } catch {
      "{0}: FAILED  {1}" -f $file.Name, $_.Exception.Message
      $failed++
    }
  }
} finally {
  $word.Quit([ref]0)
  [void][System.Runtime.InteropServices.Marshal]::ReleaseComObject($word)
}
if ($failed -gt 0) { exit 1 }
