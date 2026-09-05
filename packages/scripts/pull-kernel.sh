#!/usr/bin/bash
# Copy appsynergy-linux{,-headers} from the published Release into staging.
# ci.yml's publish job runs this before build-repo.sh so repo-add indexes a kernel.
# New pkgver-pkgrel files the sandbox uploaded (not yet in the db) are pulled
# too — build-repo.sh keeps the newest and drops superseded.
#
# Uses the public GitHub API + Release download URLs (curl). `gh` is not
# required: Actions GITHUB_TOKEN has been seen to 404 a public tag that a
# user PAT created, and this script must work in that job.
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
mkdir -p "$REPO"

api="https://api.github.com/repos/${OWNER}/${GHREPO}/releases/tags/${TAG}"
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
curl_api=(curl -sS -o "$tmp" -w '%{http_code}')
if [[ -n "${GITHUB_TOKEN:-}${GH_TOKEN:-}" ]]; then
  curl_api+=(-H "Authorization: Bearer ${GITHUB_TOKEN:-$GH_TOKEN}")
fi
code=$("${curl_api[@]}" "$api" || echo 000)
if [[ "$code" != "200" ]]; then
  echo "FAIL: GET $api -> HTTP $code"
  echo "      expected Release $OWNER/$GHREPO@$TAG (public download base $BASE)"
  exit 1
fi

mapfile -t files < <(awk -F'"' '
  /"name":/ && $4 ~ /^appsynergy-linux.*\.pkg\.tar\.zst(\.sig)?$/ { print $4 }
' "$tmp")
((${#files[@]})) || { echo "FAIL: Release $OWNER/$GHREPO@$TAG has no appsynergy-linux assets"; exit 1; }

echo "==> pulling kernel assets from $BASE"
got_pkg=0
for name in "${files[@]}"; do
  echo "  $name"
  curl -fsSL "$BASE/$name" -o "$REPO/$name"
  if [[ "$name" == appsynergy-linux-[0-9]*.pkg.tar.zst ]]; then
    got_pkg=1
  fi
done
((got_pkg)) || {
  echo "FAIL: downloaded assets but no appsynergy-linux-[0-9]*.pkg.tar.zst"
  exit 1
}
ls -lh "$REPO"/appsynergy-linux-*.pkg.tar.zst
