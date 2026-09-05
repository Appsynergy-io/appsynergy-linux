#!/usr/bin/bash
# Upload repo/x86_64/* to the GitHub Release named in packages/pacman/SERVER.
# Auth: gh (GITHUB_TOKEN in Actions, or `gh auth login` locally).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$ROOT/repo/x86_64"
SERVER_FILE="$ROOT/pacman/SERVER"

[[ -f "$SERVER_FILE" ]] || { echo "missing $SERVER_FILE"; exit 1; }
BASE="$(sed -n '1p' "$SERVER_FILE" | tr -d '[:space:]')"
[[ -n "$BASE" ]] || { echo "empty $SERVER_FILE"; exit 1; }

# https://github.com/OWNER/REPO/releases/download/TAG
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
[[ -d "$REPO" ]] || { echo "missing $REPO — run build-repo.sh first"; exit 1; }

echo "Publishing to $BASE (GitHub Release $GH@$TAG)"

rm -f "$REPO/.published-stamp"
shopt -s nullglob
all=("$REPO"/*)
shopt -u nullglob
((${#all[@]})) || { echo "no files in $REPO"; exit 1; }

# Publish order matters. The database names the packages it indexes, so it must
# go up LAST: a client syncing mid-publish against a new db whose packages are
# not uploaded yet gets 404s on every install. Packages first, db/files after.
# Publish exactly what the database indexes — never a directory glob.
mapfile -t want < <(tar xzOf "$REPO/appsynergy.db.tar.gz" --wildcards '*/desc' 2>/dev/null \
  | awk '/^%FILENAME%/{getline; print}')
((${#want[@]})) || { echo "db indexes nothing — run build-repo.sh first"; exit 1; }

pkgs=() dbs=() unsigned=()
for name in "${want[@]}"; do
  [[ -f "$REPO/$name" ]] || { echo "FAIL: db names a file missing from staging: $name"; exit 1; }
  pkgs+=("$REPO/$name")
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

declare -A PUB_SUM PUB_NAME
pub_db=$(mktemp)
if curl -fsSL -o "$pub_db" "$BASE/appsynergy.db.tar.gz" 2>/dev/null; then
  while read -r n s; do PUB_SUM["$n"]="$s"; done < <(
    tar xzOf "$pub_db" --wildcards '*/desc' 2>/dev/null | awk '
      /^%FILENAME%/  {getline; f=$0}
      /^%SHA256SUM%/ {getline; print f, $0}')
  while IFS= read -r n; do PUB_NAME["$n"]=1; done < <(
    tar xzOf "$pub_db" --wildcards '*/desc' 2>/dev/null | awk '/^%NAME%/{getline; print}')
  echo "  (published db describes ${#PUB_SUM[@]} package(s) to compare against)"
else
  echo "  (no published db yet — publishing everything)"
fi
rm -f "$pub_db"

# Never silently drop a package. A staging dir built without pull-kernel.sh
# indexes six packages, and publishing that db removes appsynergy-linux from
# every client's view until the next publish — it happened once, from a
# workstation. Retiring a package is deliberate: ALLOW_DROP=1.
declare -A LOCAL_NAME
while IFS= read -r n; do LOCAL_NAME["$n"]=1; done < <(
  tar xzOf "$REPO/appsynergy.db.tar.gz" --wildcards '*/desc' 2>/dev/null | awk '/^%NAME%/{getline; print}')
dropped=()
for n in "${!PUB_NAME[@]}"; do [[ -n "${LOCAL_NAME[$n]:-}" ]] || dropped+=("$n"); done
if ((${#dropped[@]})) && [[ "${ALLOW_DROP:-0}" != "1" ]]; then
  echo "FAIL: staging db drops package(s) the published db names:"
  printf '  %s\n' "${dropped[@]}"
  echo "Run pull-kernel.sh / rebuild the missing package, or ALLOW_DROP=1 to retire it."
  exit 1
fi

# Idempotent: ci.yml runs this on every push to main. When the published db
# already names exactly these files with exactly these sums there is nothing to
# ship — repo-add output differs byte-for-byte every build (tar mtimes), so
# compare what the db says, not the db. Skipping here also stops a dated
# snapshot landing on the Release for a docs-only merge.
# FORCE_PUBLISH=1 skips this — for a published db that is wrong or stale
# (GitHub's asset CDN can serve the previous db for minutes after a clobber).
if ((${#PUB_SUM[@]})) && [[ "${FORCE_PUBLISH:-0}" != "1" ]]; then
  same=1
  while read -r n s; do
    [[ "${PUB_SUM[$n]:-}" == "$s" ]] || { same=0; break; }
  done < <(tar xzOf "$REPO/appsynergy.db.tar.gz" --wildcards '*/desc' 2>/dev/null | awk '
      /^%FILENAME%/  {getline; f=$0}
      /^%SHA256SUM%/ {getline; print f, $0}')
  ((${#want[@]} == ${#PUB_SUM[@]})) || same=0
  if ((same)); then
    echo "==> published db already describes these ${#want[@]} package(s) — nothing to publish"
    exec "$ROOT/scripts/verify-repo.sh"
  fi
fi

if ! gh release view "$TAG" --repo "$GH" >/dev/null 2>&1; then
  gh release create "$TAG" --repo "$GH" --latest=false \
    --title "pacman $TAG" \
    --notes "AppSynergy Linux pacman repository ($TAG). Packages first, database last."
fi

put_file() {
  local f="$1" force="${2:-}" name
  name=$(basename "$f")
  if [[ "$force" != "force" && -n "${PUB_SUM[$name]:-}" ]]; then
    if [[ "${PUB_SUM[$name]}" == "$(sha256sum "$f" | cut -d' ' -f1)" ]]; then
      echo "  same $name (published bytes match) — skipped"
      return 0
    fi
  fi
  echo "  PUT $name ($(du -h "$f" | awk '{print $1}'))"
  gh release upload "$TAG" "$f" --repo "$GH" --clobber
  echo "    OK"
}

echo "==> packages (${#pkgs[@]})"
for f in "${pkgs[@]}"; do put_file "$f"; done
echo "==> database (${#dbs[@]}) — last, so it never names an absent package"
for f in "${dbs[@]}"; do put_file "$f" force; done

# Snapshot: a dated copy of the db (+sig) survives the next publish, so
# rolling back = pointing pacman -U at the packages a dated db names — old
# package files are never deleted from the registry, only the live db moves.
STAMP=$(date +%Y%m%d-%H%M)
# ci.yml tags published/$STAMP on the commit that ran this; the file is the
# handoff (gitignored with the rest of staging). Removed first so a no-op run
# never re-tags with a stale stamp.
printf '%s\n' "$STAMP" > "$REPO/.published-stamp"
snapdir=$(mktemp -d)
trap 'rm -rf "$snapdir"' EXIT
for f in "$REPO/appsynergy.db.tar.gz" "$REPO/appsynergy.db.tar.gz.sig"; do
  [[ -f "$f" ]] || continue
  name="appsynergy.db-$STAMP${f##*appsynergy.db}"
  cp -a "$f" "$snapdir/$name"
  echo "  PUT $name (snapshot)"
  gh release upload "$TAG" "$snapdir/$name" --repo "$GH"
done

echo
echo "Public Server line:"
echo "  Server = $BASE"
echo "Test:"
echo "  curl -fsSL $BASE/appsynergy.db -o /dev/null && echo db_ok"

"$ROOT/scripts/verify-repo.sh"
