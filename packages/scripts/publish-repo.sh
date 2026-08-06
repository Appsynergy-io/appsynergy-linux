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
# Publish exactly what the database indexes — never a directory glob. Staging
# holds packages the db deliberately does not name (a server kernel built for a
# later release, a superseded rel kept as a rollback target); uploading those
# puts them in front of production without anyone deciding to.
mapfile -t want < <(tar xzOf "$REPO/appsynergy.db.tar.gz" --wildcards '*/desc' 2>/dev/null \
  | awk '/^%FILENAME%/{getline; print}')
((${#want[@]})) || { echo "db indexes nothing — run build-repo.sh first"; exit 1; }

pkgs=() dbs=()
for name in "${want[@]}"; do
  [[ -f "$REPO/$name" ]] || { echo "FAIL: db names a file missing from staging: $name"; exit 1; }
  pkgs+=("$REPO/$name")
  # SigLevel Required makes pacman fetch <pkg>.sig — a package published without
  # its sig hard-fails every client install once signatures are enforced.
  [[ -f "$REPO/$name.sig" ]] && pkgs+=("$REPO/$name.sig")
done
for f in "$REPO"/appsynergy.db* "$REPO"/appsynergy.files*; do
  [[ -f "$f" && "$f" != *.old ]] && dbs+=("$f")
done
skipped=$(( ${#all[@]} - ${#pkgs[@]} - ${#dbs[@]} ))
((skipped > 0)) && echo "  ($skipped file(s) in staging not indexed by the db — not published)"

put_file() {
  local f="$1" force="${2:-}" name
  name=$(basename "$f")
  # A package filename encodes name-ver-rel-arch, so it is immutable: if one of
  # the same size is already up, re-uploading only opens a window where clients
  # 404 on it between the DELETE and the PUT. The db must always be replaced.
  if [[ "$force" != "force" ]]; then
    local remote local_len
    local_len=$(stat -c%s "$f")
    remote=$(curl -sSI "$BASE/$name" 2>/dev/null \
      | awk 'tolower($1)=="content-length:"{gsub(/\r/,"");print $2}' | tail -1)
    if [[ -n "$remote" && "$remote" == "$local_len" ]]; then
      echo "  same $name (already published, ${local_len}B) — skipped"
      return 0
    fi
  fi
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
for f in "${dbs[@]}"; do put_file "$f" force; done

# Snapshot: a dated copy of the db (+sig) survives the next publish, so
# rolling back = pointing pacman -U at the packages a dated db names — old
# package files are never deleted from the registry, only the live db moves.
STAMP=$(date +%Y%m%d-%H%M)
for f in "$REPO/appsynergy.db.tar.gz" "$REPO/appsynergy.db.tar.gz.sig"; do
  [[ -f "$f" ]] || continue
  name="appsynergy.db-$STAMP${f##*appsynergy.db}"
  echo "  PUT $name (snapshot)"
  curl -sS -o /dev/null -w "    %{http_code}\n" -X PUT \
    -H "Authorization: token $TOK" -H "Content-Type: application/octet-stream" \
    --data-binary @"$f" "$BASE/$name"
done

echo
echo "Public Server line:"
echo "  Server = $BASE"
echo "Test:"
echo "  curl -fsSL $BASE/appsynergy.db -o /dev/null && echo db_ok"
