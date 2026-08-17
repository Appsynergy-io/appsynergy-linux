#!/usr/bin/bash
# Populate packages/repo/x86_64 from the published GitHub Release.
# ISO builds read that staging dir via file://; it is gitignored.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$ROOT/repo/x86_64"
SERVER_FILE="$ROOT/pacman/SERVER"
[[ -f "$SERVER_FILE" ]] || { echo "missing $SERVER_FILE"; exit 1; }
BASE="$(sed -n '1p' "$SERVER_FILE" | tr -d '[:space:]')"
[[ -n "$BASE" ]] || { echo "empty $SERVER_FILE"; exit 1; }

mkdir -p "$REPO"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "==> $BASE"
curl -fsSL "$BASE/appsynergy.db.tar.gz" -o "$tmp/appsynergy.db.tar.gz"

mapfile -t want < <(tar xzOf "$tmp/appsynergy.db.tar.gz" --wildcards '*/desc' 2>/dev/null \
  | awk '/^%FILENAME%/{getline; print}')
((${#want[@]})) || { echo "published db indexes nothing"; exit 1; }

for n in appsynergy.db.tar.gz appsynergy.db.tar.gz.sig \
         appsynergy.db appsynergy.db.sig \
         appsynergy.files.tar.gz appsynergy.files.tar.gz.sig \
         appsynergy.files appsynergy.files.sig; do
  if curl -fsSL "$BASE/$n" -o "$tmp/$n"; then
    cp -a "$tmp/$n" "$REPO/$n"
    echo "  $n"
  else
    echo "  skip $n (not on Release)"
  fi
done

for name in "${want[@]}"; do
  echo "  $name"
  curl -fsSL "$BASE/$name" -o "$REPO/$name"
  if curl -fsSL "$BASE/$name.sig" -o "$REPO/$name.sig"; then
    echo "    +sig"
  else
    rm -f "$REPO/$name.sig"
    echo "    (no .sig)"
  fi
done

echo "Staging dir: $REPO"
