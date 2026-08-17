#!/usr/bin/bash
# Copy appsynergy-linux{,-headers} from the published Release into staging.
# packages.yml runs this before build-repo.sh so repo-add indexes a kernel.
# New pkgver-pkgrel files the sandbox uploaded (not yet in the db) are pulled
# too — build-repo.sh keeps the newest and drops superseded.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$ROOT/repo/x86_64"
SERVER_FILE="$ROOT/pacman/SERVER"
[[ -f "$SERVER_FILE" ]] || { echo "missing $SERVER_FILE"; exit 1; }
BASE="$(sed -n '1p' "$SERVER_FILE" | tr -d '[:space:]')"
[[ -n "$BASE" ]] || { echo "empty $SERVER_FILE"; exit 1; }

if [[ "$BASE" =~ ^https://github.com/([^/]+)/([^/]+)/releases/download/([^/]+)$ ]]; then
  OWNER="${BASH_REMATCH[1]}"
  GHREPO="${BASH_REMATCH[2]}"
  TAG="${BASH_REMATCH[3]}"
else
  echo "SERVER is not a GitHub Release download URL: $BASE"
  exit 1
fi
GH="$OWNER/$GHREPO"
command -v gh >/dev/null || { echo "gh not found"; exit 1; }
mkdir -p "$REPO"

if ! gh release view "$TAG" --repo "$GH" >/dev/null 2>&1; then
  echo "FAIL: Release $GH@$TAG does not exist — bootstrap it once (upload kernel 7.1.6-1)"
  exit 1
fi

echo "==> pulling kernel assets from $GH@$TAG"
if ! gh release download "$TAG" --repo "$GH" --dir "$REPO" --clobber \
      --pattern 'appsynergy-linux-*.pkg.tar.zst'; then
  echo "FAIL: no appsynergy-linux packages on $GH@$TAG"
  exit 1
fi
gh release download "$TAG" --repo "$GH" --dir "$REPO" --clobber \
  --pattern 'appsynergy-linux-*.pkg.tar.zst.sig' || true

if ! compgen -G "$REPO/appsynergy-linux-[0-9]*.pkg.tar.zst" > /dev/null; then
  echo "FAIL: download reported ok but staging has no appsynergy-linux-[0-9]*.pkg.tar.zst"
  exit 1
fi
ls -lh "$REPO"/appsynergy-linux-*.pkg.tar.zst
