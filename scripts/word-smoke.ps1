# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# Local smoke test (Windows with Microsoft Word installed): opens every .docx in a
# folder read-only in a hidden Word instance and reports what Word sees. Any file
# Word cannot open is reported as FAILED and the script exits with code 1.
# Used instead of the LibreOffice headless check when LibreOffice isn't installed.
#
# Usage: powershell -NoProfile -File scripts/word-smoke.ps1 [folder]

param([string]$Folder = (Join-Path $PSScriptRoot "..\fixtures\docx\synthetic"))

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
