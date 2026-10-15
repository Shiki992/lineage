# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# A certificate for https://localhost:3000, so Word will load the task pane from
# the development server. Office add-ins must be served over HTTPS; a
# self-signed certificate is fine once this computer trusts it.
# https://learn.microsoft.com/office/dev/add-ins/develop/add-in-manifests#hosting-requirements
#
# Run it yourself, once: adding a certificate to your trusted roots changes your
# computer's security settings, and Windows asks you to confirm.
#
#   powershell -ExecutionPolicy Bypass -File scripts/dev-cert.ps1           # make and trust
#   powershell -ExecutionPolicy Bypass -File scripts/dev-cert.ps1 -Remove   # undo
#
# The certificate is for "localhost" only, lasts a year, and lives in your user
# profile (%USERPROFILE%\.lineage-dev-certs), never in the repository.

param([switch]$Remove)

$ErrorActionPreference = "Stop"
$folder = Join-Path $env:USERPROFILE ".lineage-dev-certs"
$name = "Lineage development (localhost)"

function Find-Certs([string]$store) {
    Get-ChildItem "Cert:\CurrentUser\$store" | Where-Object { $_.FriendlyName -eq $name }
}

if ($Remove) {
    Find-Certs "Root" | Remove-Item
    Find-Certs "My" | Remove-Item
    if (Test-Path $folder) { Remove-Item -Recurse -Force $folder }
    Write-Host "Removed the Lineage development certificate."
    exit 0
}

if (Find-Certs "Root") {
    Write-Host "The Lineage development certificate is already trusted. Use -Remove to start again."
    exit 0
}

$cert = New-SelfSignedCertificate `
    -DnsName "localhost", "127.0.0.1" `
    -FriendlyName $name `
    -CertStoreLocation "Cert:\CurrentUser\My" `
    -KeyExportPolicy Exportable `
    -KeyAlgorithm RSA -KeyLength 2048 `
    -Type SSLServerAuthentication `
    -NotAfter (Get-Date).AddYears(1)

New-Item -ItemType Directory -Force $folder | Out-Null
$bytes = New-Object byte[] 24
[System.Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
$pass = [Convert]::ToBase64String($bytes)
Set-Content -Path (Join-Path $folder "localhost.pass") -Value $pass -NoNewline -Encoding ascii
Export-PfxCertificate -Cert $cert -FilePath (Join-Path $folder "localhost.pfx") `
    -Password (ConvertTo-SecureString $pass -AsPlainText -Force) | Out-Null

# Trust it: Windows shows a security warning here and asks you to confirm.
$public = Join-Path $folder "localhost.cer"
Export-Certificate -Cert $cert -FilePath $public | Out-Null
Import-Certificate -FilePath $public -CertStoreLocation "Cert:\CurrentUser\Root" | Out-Null

Write-Host "Done. https://localhost:3000 is trusted on this computer until $($cert.NotAfter.ToShortDateString())."
