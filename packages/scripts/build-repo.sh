#!/usr/bin/bash
# Build any/ PKGBUILDs + assemble repo/x86_64 with repo-add.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$ROOT/repo/x86_64"
mkdir -p "$REPO"

build_pkg() {
  local dir="$1"
  # -d: these are file-shipping any/ packages; depends= are runtime-only and
  # need not be installed on the build host.
  (cd "$dir" && makepkg -f --noconfirm -c -d 2>/dev/null || makepkg -f --noconfirm -d)
  shopt -s nullglob
  for f in "$dir"/*.pkg.tar.zst; do
    cp -a "$f" "$REPO/"
  done
  shopt -u nullglob
}

echo "==> Building appsynergy any/ packages"
build_pkg "$ROOT/pkgbuilds/appsynergy-mirrorlist"
build_pkg "$ROOT/pkgbuilds/appsynergy-branding"
build_pkg "$ROOT/pkgbuilds/appsynergy-wallpapers"
build_pkg "$ROOT/pkgbuilds/appsynergy-branding-desktop"

# Stage custom kernel if present on this machine
for f in \
  /home/imma/src/linux-cachyos/linux-cachyos/linux-appsynergy-[0-9]*.pkg.tar.zst \
  /home/imma/src/linux-cachyos/linux-cachyos/linux-appsynergy-headers-*.pkg.tar.zst \
  /home/imma/src/linux-cachyos/linux-cachyos/linux-cachyos-igpu-[0-9]*.pkg.tar.zst \
  /home/imma/src/linux-cachyos/linux-cachyos/linux-cachyos-igpu-headers-*.pkg.tar.zst
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
