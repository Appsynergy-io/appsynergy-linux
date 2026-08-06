#!/usr/bin/bash
# Upload repo/x86_64/* to Gitea generic packages (public GET).
# Auth: GITEA_TOKEN or ~/.config/tea/config.yml
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$ROOT/repo/x86_64"
HOST="${GITEA_HOST:-https://git.appsynergy.io}"
OWNER="${GITEA_OWNER:-imabee}"
PKG_NAME="appsynergy-repo"
PKG_VER="x86_64"

if [[ -n "${GITEA_TOKEN:-}" ]]; then
  TOK="$GITEA_TOKEN"
elif [[ -f "${HOME}/.config/tea/config.yml" ]]; then
  TOK=$(rg -o 'token:.*' "${HOME}/.config/tea/config.yml" | head -1 | sed 's/token: *//')
else
  echo "No GITEA_TOKEN"; exit 1
fi
[[ -n "$TOK" ]] || { echo "empty token"; exit 1; }
[[ -d "$REPO" ]] || { echo "missing $REPO — run build-repo.sh first"; exit 1; }

BASE="$HOST/api/packages/$OWNER/generic/$PKG_NAME/$PKG_VER"
echo "Publishing to $BASE (public package registry)"

shopt -s nullglob
all=("$REPO"/*)
shopt -u nullglob
((${#all[@]})) || { echo "no files in $REPO"; exit 1; }

# Publish order matters. The database names the packages it indexes, so it must
# go up LAST: a client syncing mid-publish against a new db whose packages are
# not uploaded yet gets 404s on every install. Packages first, db/files after.
pkgs=() dbs=()
for f in "${all[@]}"; do
  [[ -f "$f" ]] || continue           # skips nothing-but-dirs
  name=$(basename "$f")
  case "$name" in
    *.old|*.sig)  echo "  skip  $name (not published)"; continue ;;
    appsynergy.db*|appsynergy.files*) dbs+=("$f") ;;
    *.pkg.tar.zst) pkgs+=("$f") ;;
    *) echo "  skip  $name (unrecognised)"; continue ;;
  esac
done
((${#pkgs[@]})) || { echo "no packages in $REPO — run build-repo.sh first"; exit 1; }

put_file() {
  local f="$1" name
  name=$(basename "$f")
  echo "  PUT $name ($(du -h "$f" | awk '{print $1}'))"
  # Gitea generic packages reject a re-PUT of an existing filename with 409.
  # Delete first so republishing is idempotent; 404 on a new file is expected.
  curl -sS -o /dev/null -X DELETE -H "Authorization: token $TOK" "$BASE/$name" || true
  local code
  code=$(curl -sS -o /tmp/gitea-put.out -w "%{http_code}" \
    -X PUT \
    -H "Authorization: token $TOK" \
    -H "Content-Type: application/octet-stream" \
    --data-binary @"$f" \
    "$BASE/$name")
  if [[ "$code" != "201" && "$code" != "204" && "$code" != "200" ]]; then
    echo "FAIL $name HTTP $code"
    cat /tmp/gitea-put.out
    exit 1
  fi
  echo "    OK $code"
}

echo "==> packages (${#pkgs[@]})"
for f in "${pkgs[@]}"; do put_file "$f"; done
echo "==> database (${#dbs[@]}) — last, so it never names an absent package"
for f in "${dbs[@]}"; do put_file "$f"; done

echo
echo "Public Server line:"
echo "  Server = $BASE"
echo "Test:"
echo "  curl -fsSL $BASE/appsynergy.db -o /dev/null && echo db_ok"
