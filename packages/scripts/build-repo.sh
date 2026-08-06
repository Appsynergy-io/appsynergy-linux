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
  # Copy ONLY what this build produced (makepkg --packagelist), never a dir
  # glob: pkgbuild dirs accumulate stale rels, and a glob re-imports every one
  # of them into staging where repo-add then indexes an arbitrary version.
  # NB: --packagelist also names -debug- packages that are never created; an
  # `[[ -f ]] &&` guard as the loop's last command returns 1 there and set -e
  # kills the whole script silently. Use `if` (set -e exempt) and skip -debug-.
  while IFS= read -r f; do
    if [[ "$f" != *-debug-* && -f "$f" ]]; then
      cp -a "$f" "$REPO/"
    fi
  done < <(cd "$dir" && makepkg --packagelist 2>/dev/null)
}

echo "==> Regenerating payload tarballs (deterministic; sums pinned in PKGBUILDs)"
"$ROOT/scripts/make-srctars.sh"

echo "==> Building appsynergy any/ packages"
build_pkg "$ROOT/pkgbuilds/appsynergy-mirrorlist"
build_pkg "$ROOT/pkgbuilds/appsynergy-ca-certificates"
build_pkg "$ROOT/pkgbuilds/appsynergy-keyring"
build_pkg "$ROOT/pkgbuilds/appsynergy-branding"
build_pkg "$ROOT/pkgbuilds/appsynergy-wallpapers"
build_pkg "$ROOT/pkgbuilds/appsynergy-branding-desktop"

# Stage custom kernel if present on this machine
# (KDIR: host-local kernel build tree; kernel/upstream/PIN records its contract)
KDIR="${KDIR:-/home/imma/src/linux-cachyos/linux-cachyos}"
for f in \
  "$KDIR"/linux-appsynergy-[0-9]*.pkg.tar.zst \
  "$KDIR"/linux-appsynergy-headers-*.pkg.tar.zst \
  "$KDIR"/linux-cachyos-igpu-[0-9]*.pkg.tar.zst \
  "$KDIR"/linux-cachyos-igpu-headers-*.pkg.tar.zst
 do
  [[ -f "$f" ]] || continue
  [[ "$f" == *dbg* ]] && continue
  cp -a "$f" "$REPO/"
  echo "    staged $(basename "$f")"
done

# Signing key: fingerprint pinned in pkgbuilds/appsynergy-keyring. Every package
# gets a detached sig; repo-add --sign covers the database. SIGN=0 skips (dev).
GPGKEY="${GPGKEY:-3B90D92D1E28E9E060D5C53D15D4351CF0D36AD1}"
if [[ "${SIGN:-1}" == "1" ]]; then
  echo "==> Signing packages ($GPGKEY)"
  for f in "$REPO"/*.pkg.tar.zst; do
    # re-sign only when missing or stale (sig older than package)
    if [[ ! -f "$f.sig" || "$f" -nt "$f.sig" ]]; then
      gpg --batch --yes --detach-sign --no-armor -u "$GPGKEY" -o "$f.sig" "$f"
      echo "    signed $(basename "$f")"
    fi
  done
fi

echo "==> repo-add"
cd "$REPO"
rm -f appsynergy.db* appsynergy.files*
if [[ "${SIGN:-1}" == "1" ]]; then
  repo-add -n --sign --key "$GPGKEY" appsynergy.db.tar.gz ./*.pkg.tar.zst
else
  repo-add -n appsynergy.db.tar.gz ./*.pkg.tar.zst
fi
# pacman also looks for appsynergy.db (symlink created by repo-add usually)
ls -lh
echo "Done. Staging dir: $REPO"
