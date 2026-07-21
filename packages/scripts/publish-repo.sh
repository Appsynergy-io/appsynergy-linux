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
files=("$REPO"/*)
shopt -u nullglob
((${#files[@]})) || { echo "no files in $REPO"; exit 1; }

for f in "${files[@]}"; do
  [[ -f "$f" ]] || continue
  # skip nested dirs
  name=$(basename "$f")
  # skip .sig for now if any
  echo "  PUT $name ($(du -h "$f" | awk '{print $1}'))"
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
done

echo
echo "Public Server line:"
echo "  Server = $BASE"
echo "Test:"
echo "  curl -fsSL $BASE/appsynergy.db -o /dev/null && echo db_ok"
