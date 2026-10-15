# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# Sideloads the development add-in into Word on Windows, or removes it.
#
# Word on Windows lists every manifest named under
# HKCU\Software\Microsoft\Office\16.0\Wef\Developer (value name: the manifest's
# Id, value: the manifest's path) as a developer add-in. This is what
# Microsoft's office-addin-dev-settings tool writes for an add-in-only (XML)
# manifest (lib/dev-settings-windows.js, registerAddIn); doing it here avoids
# that tool's dependencies. Only your own user's settings change.
#
#   powershell -ExecutionPolicy Bypass -File scripts/sideload-word.ps1           # add
#   powershell -ExecutionPolicy Bypass -File scripts/sideload-word.ps1 -Remove   # remove
#
# Close Word first; it reads the list when it starts.

param([switch]$Remove)

$ErrorActionPreference = "Stop"
$manifest = Resolve-Path (Join-Path $PSScriptRoot "..\apps\office-addin\manifest.xml")
$id = ([xml](Get-Content -Raw $manifest)).OfficeApp.Id
if (-not $id) { throw "No <Id> in $manifest" }
$key = "HKCU:\Software\Microsoft\Office\16.0\Wef\Developer"

if ($Remove) {
    if (Test-Path $key) { Remove-ItemProperty -Path $key -Name $id -ErrorAction SilentlyContinue }
    Write-Host "Removed the Lineage development add-in from Word."
    exit 0
}

New-Item -Path $key -Force | Out-Null
New-ItemProperty -Path $key -Name $id -Value $manifest.Path -PropertyType String -Force | Out-Null
Write-Host "Sideloaded $id from $($manifest.Path)."
Write-Host "Start Word, then Home > Add-ins > Lineage (or Insert > My Add-ins > Developer Add-ins)."
