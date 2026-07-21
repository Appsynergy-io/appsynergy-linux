#!/usr/bin/bash
# Build any/ PKGBUILDs + assemble repo/x86_64 with repo-add.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$ROOT/repo/x86_64"
mkdir -p "$REPO"

build_pkg() {
  local dir="$1"
  (cd "$dir" && makepkg -f --noconfirm -c 2>/dev/null || makepkg -f --noconfirm)
  shopt -s nullglob
  for f in "$dir"/*.pkg.tar.zst; do
    cp -a "$f" "$REPO/"
  done
  shopt -u nullglob
}

echo "==> Building appsynergy-mirrorlist / branding"
build_pkg "$ROOT/pkgbuilds/appsynergy-mirrorlist"
build_pkg "$ROOT/pkgbuilds/appsynergy-branding"

# Stage custom kernel if present on this machine
for f in \
  /home/imma/src/linux-cachyos/linux-cachyos/linux-cachyos-igpu-7*.pkg.tar.zst \
  /home/imma/src/linux-cachyos/linux-cachyos/linux-cachyos-igpu-headers-7*.pkg.tar.zst
 do
  [[ -f "$f" ]] || continue
  [[ "$f" == *dbg* ]] && continue
  cp -a "$f" "$REPO/"
  echo "    staged $(basename "$f")"
done

echo "==> repo-add"
cd "$REPO"
rm -f appsynergy.db* appsynergy.files*
# repo-add wants a db name
repo-add -n appsynergy.db.tar.gz ./*.pkg.tar.zst
# pacman also looks for appsynergy.db (symlink created by repo-add usually)
ls -lh
echo "Done. Staging dir: $REPO"
