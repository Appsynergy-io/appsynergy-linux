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

pkgs=() dbs=() unsigned=()
for name in "${want[@]}"; do
  [[ -f "$REPO/$name" ]] || { echo "FAIL: db names a file missing from staging: $name"; exit 1; }
  pkgs+=("$REPO/$name")
  # SigLevel Required makes pacman fetch <pkg>.sig — a package published without
  # its sig hard-fails every client install once signatures are enforced.
  if [[ -f "$REPO/$name.sig" ]]; then
    pkgs+=("$REPO/$name.sig")
  else
    unsigned+=("$name")
  fi
done
[[ -f "$REPO/appsynergy.db.tar.gz.sig" ]] || unsigned+=("appsynergy.db.tar.gz")
if ((${#unsigned[@]})); then
  if [[ "${ALLOW_UNSIGNED:-0}" == "1" ]]; then
    echo "##############################################################"
    echo "# WARNING: PUBLISHING UNSIGNED (ALLOW_UNSIGNED=1)"
    echo "# Missing detached signatures:"
    printf '#   %s.sig\n' "${unsigned[@]}"
    echo "# SigLevel Required clients will refuse everything listed."
    echo "##############################################################"
  else
    echo "FAIL: refusing unsigned publish — missing .sig for:"
    printf '  %s\n' "${unsigned[@]}"
    echo "Rebuild with SIGN=1 (build-repo.sh default); ALLOW_UNSIGNED=1 overrides."
    exit 1
  fi
fi
for f in "$REPO"/appsynergy.db* "$REPO"/appsynergy.files*; do
  [[ -f "$f" && "$f" != *.old ]] && dbs+=("$f")
done
skipped=$(( ${#all[@]} - ${#pkgs[@]} - ${#dbs[@]} ))
((skipped > 0)) && echo "  ($skipped file(s) in staging not indexed by the db — not published)"

# What is published, by content, read once from the PUBLISHED database.
#
# The skip below used to compare Content-Length, which is all a HEAD gives, and
# that silently corrupted the repo. A detached ed25519 signature is always 119
# bytes, so every .sig was skipped forever after its first publish, while a
# non-reproducible rebuild that changed a package's byte count did upload. Six
# of the eight published packages ended up carrying a signature over different
# bytes, and SigLevel = Required refuses every one of them:
#   error: appsynergy-keyring: signature ... is invalid
# A hash cannot agree the way a length can, and the db is small enough to fetch.
declare -A PUB_SUM
pub_db=$(mktemp)
if curl -fsS -o "$pub_db" "$BASE/appsynergy.db.tar.gz" 2>/dev/null; then
  while read -r n s; do PUB_SUM["$n"]="$s"; done < <(
    tar xzOf "$pub_db" --wildcards '*/desc' 2>/dev/null | awk '
      /^%FILENAME%/  {getline; f=$0}
      /^%SHA256SUM%/ {getline; print f, $0}')
  echo "  (published db describes ${#PUB_SUM[@]} package(s) to compare against)"
else
  echo "  (no published db yet — publishing everything)"
fi
rm -f "$pub_db"

put_file() {
  local f="$1" force="${2:-}" name
  name=$(basename "$f")
  # A package filename encodes name-ver-rel-arch, so identical content really is
  # already up and re-uploading only opens a window where clients 404 on it
  # between the DELETE and the PUT. Anything the published db does not describe
  # — every .sig — is always uploaded, which is what keeps a package and its
  # signature in step. The db itself is always replaced.
  if [[ "$force" != "force" && -n "${PUB_SUM[$name]:-}" ]]; then
    if [[ "${PUB_SUM[$name]}" == "$(sha256sum "$f" | cut -d' ' -f1)" ]]; then
      echo "  same $name (published bytes match) — skipped"
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

# Publishing is not fire-and-forget: assert production == staging, every time.
# Last command, so its exit code is the publish exit code.
"$ROOT/scripts/verify-repo.sh"
