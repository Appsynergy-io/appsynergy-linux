#!/usr/bin/bash
# Compare the PUBLISHED pacman database against local staging, and confirm every
# package the published db names is actually downloadable. Exit 1 on drift.
#
# Why this exists: the published repo is the contract — every machine installs
# from it, and nothing else asserted it. appsynergy-branding sat four releases
# behind on production for two weeks, and the server kernels staged locally were
# never published at all, because publishing was fire-and-forget.
#
# Run after publish-repo.sh, and in any check that gates a release.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$ROOT/repo/x86_64"
BASE="${GITEA_BASE:-https://git.appsynergy.io/api/packages/imabee/generic/appsynergy-repo/x86_64}"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/pub"

# db tarball -> one "pkgname-pkgver-pkgrel" per line
entries() { tar tzf "$1" 2>/dev/null | sed -n 's#^\([^/]*\)/$#\1#p' | sort; }

[[ -f "$REPO/appsynergy.db.tar.gz" ]] || {
  echo "no local db at $REPO — run build-repo.sh first"; exit 1; }
entries "$REPO/appsynergy.db.tar.gz" > "$tmp/local.txt"

if curl -fsSL "$BASE/appsynergy.db" -o "$tmp/pub.tar.gz" 2>/dev/null; then
  entries "$tmp/pub.tar.gz" > "$tmp/pub.txt"
  tar xzf "$tmp/pub.tar.gz" -C "$tmp/pub" 2>/dev/null || true
else
  echo "WARN: no published db at $BASE"
  : > "$tmp/pub.txt"
fi

printf '=== local staging (%s) ===\n' "$(wc -l < "$tmp/local.txt")"; sed 's/^/  /' "$tmp/local.txt"
printf '=== published     (%s) ===\n' "$(wc -l < "$tmp/pub.txt")";   sed 's/^/  /' "$tmp/pub.txt"

rc=0

echo "=== drift ==="
while read -r e; do
  [[ -n "$e" ]] || continue
  grep -qxF "$e" "$tmp/pub.txt" || { echo "  NOT PUBLISHED: $e"; rc=1; }
done < "$tmp/local.txt"
while read -r e; do
  [[ -n "$e" ]] || continue
  grep -qxF "$e" "$tmp/local.txt" || echo "  published-only (stale, harmless): $e"
done < "$tmp/pub.txt"
((rc)) || echo "  none"

# A db entry whose package 404s is worse than drift: pacman resolves it, then
# fails mid-transaction. This is the check that catches a half-finished publish.
echo "=== reachability of published packages ==="
shopt -s nullglob
for d in "$tmp"/pub/*/desc; do
  fn=$(awk '/^%FILENAME%/{getline; print; exit}' "$d")
  [[ -n "$fn" ]] || continue
  code=$(curl -sS -o /dev/null -w '%{http_code}' -I "$BASE/$fn" || echo 000)
  sig=$(curl -sS -o /dev/null -w '%{http_code}' -I "$BASE/$fn.sig" || echo 000)
  if [[ "$code" == "200" && "$sig" == "200" ]]; then
    echo "  ok   $fn (+sig)"
  else
    echo "  FAIL $fn (pkg=$code sig=$sig) — SigLevel Required clients cannot install this"
    rc=1
  fi
done
shopt -u nullglob

# SigLevel Required also verifies the database: pacman fetches <db>.sig next to
# the db it synced. Both names must resolve, or every sync fails at the client.
echo "=== database signatures ==="
for n in appsynergy.db.sig appsynergy.db.tar.gz.sig; do
  code=$(curl -sS -o /dev/null -w '%{http_code}' -I "$BASE/$n" || echo 000)
  if [[ "$code" == "200" ]]; then
    echo "  ok   $n"
  else
    echo "  FAIL $n (HTTP $code) — SigLevel Required clients cannot verify the db"
    rc=1
  fi
done

if ((rc)); then
  echo
  echo "FAIL: published repo is behind or incomplete — run publish-repo.sh"
  exit 1
fi
echo
echo "OK: published repo matches local staging and every package is fetchable"
