#!/usr/bin/env bash
# Copyright 2026 Hrishikesh Vedantam
# SPDX-License-Identifier: Apache-2.0

# Linux and macOS counterpart of dev-cert.ps1: a certificate for
# https://localhost:3000, so Word on the web will load the task pane from the
# development server. Office add-ins must be served over HTTPS; a self-signed
# certificate is fine once your browser trusts it.
# https://learn.microsoft.com/office/dev/add-ins/develop/add-in-manifests#hosting-requirements
#
#   bash scripts/dev-cert.sh            # make it, and trust it in Chrome/Edge/Firefox if possible
#   bash scripts/dev-cert.sh --remove   # undo
#
# Files go to ~/.lineage-dev-certs (never the repository), the same names the
# Windows script uses, so apps/office-addin/vite.config.ts finds them unchanged.
# Needs openssl. Trusting uses NSS's certutil (package libnss3-tools on
# Debian/Ubuntu, nss-tools on Fedora) for your own browser profiles only; it never
# uses sudo. System-wide trust is printed as a command for you to run yourself.

set -euo pipefail

dir="$HOME/.lineage-dev-certs"
nick="Lineage development (localhost)"

nss_dbs() {
  # Chrome/Chromium/Edge share ~/.pki/nssdb; each Firefox profile has its own.
  [ -d "$HOME/.pki/nssdb" ] && echo "sql:$HOME/.pki/nssdb"
  for p in "$HOME"/.mozilla/firefox/*/ "$HOME"/snap/firefox/common/.mozilla/firefox/*/; do
    [ -f "$p/cert9.db" ] && echo "sql:${p%/}"
  done
  return 0
}

if [ "${1:-}" = "--remove" ]; then
  if command -v certutil >/dev/null; then
    while read -r db; do
      [ -n "$db" ] && certutil -D -d "$db" -n "$nick" 2>/dev/null || true
    done < <(nss_dbs)
  fi
  rm -rf "$dir"
  echo "Removed the Lineage development certificate."
  echo "If you trusted it system-wide, also run: sudo rm /usr/local/share/ca-certificates/lineage-localhost.crt && sudo update-ca-certificates --fresh"
  exit 0
fi

command -v openssl >/dev/null || { echo "openssl is needed." >&2; exit 1; }

if [ -f "$dir/localhost.pfx" ]; then
  echo "A Lineage development certificate already exists in $dir. Use --remove to start again."
  exit 0
fi

mkdir -p "$dir"
chmod 700 "$dir"
openssl req -x509 -newkey rsa:2048 -sha256 -days 365 -nodes \
  -keyout "$dir/localhost.key" -out "$dir/localhost.crt" \
  -subj "/CN=localhost" \
  -addext "subjectAltName=DNS:localhost,IP:127.0.0.1" \
  -addext "extendedKeyUsage=serverAuth" 2>/dev/null

pass="$(openssl rand -base64 24)"
printf '%s' "$pass" > "$dir/localhost.pass"
openssl pkcs12 -export -out "$dir/localhost.pfx" \
  -inkey "$dir/localhost.key" -in "$dir/localhost.crt" \
  -passout "pass:$pass" -name "$nick"
rm "$dir/localhost.key"
chmod 600 "$dir"/*

trusted=0
if command -v certutil >/dev/null; then
  while read -r db; do
    [ -z "$db" ] && continue
    certutil -A -d "$db" -n "$nick" -t "C,," -i "$dir/localhost.crt" && trusted=1
  done < <(nss_dbs)
fi

if [ "$trusted" = 1 ]; then
  echo "Done: https://localhost:3000 is trusted in your browser profiles for a year. Restart the browser."
else
  echo "Made the certificate in $dir, but couldn't trust it automatically (certutil not found, or no browser profile yet)."
  echo "Install libnss3-tools (Debian/Ubuntu) or nss-tools (Fedora) and run this again, or import $dir/localhost.crt in your browser's certificate settings."
fi
echo "Optional system-wide trust (Debian/Ubuntu), run yourself:"
echo "  sudo cp $dir/localhost.crt /usr/local/share/ca-certificates/lineage-localhost.crt && sudo update-ca-certificates"
