#!/usr/bin/bash
# Build AppSynergy Linux installer ISO (requires root + archiso).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
# Monorepo root: sibling subtrees packages/ and kernel/ live beside desktop/.
MONO="$(cd "$ROOT/.." && pwd)"
PROFILE="$ROOT/iso"
OUT="${OUT:-$ROOT/out}"
# Unique work dir by default so we never need to wipe a busy tree.
WORK="${WORK:-$ROOT/work-$(date +%Y%m%d-%H%M%S)}"

[[ "$(id -u)" -eq 0 ]] || { echo "Run as root: sudo $0"; exit 1; }
command -v mkarchiso >/dev/null || { echo "install archiso"; exit 1; }

build_user="${SUDO_USER:-}"
[[ -n "$build_user" && "$build_user" != root ]] || {
  echo "ERROR: SUDO_USER unset — run via sudo as the user whose cargo/rescue CLIs stage into the image"
  exit 1
}
build_home=$(getent passwd "$build_user" | cut -d: -f6)
# cargo as the real user (not root) so rustup/toolchain resolves
if [[ -x "$build_home/.cargo/bin/cargo" ]]; then
  CARGO="$build_home/.cargo/bin/cargo"
elif command -v cargo >/dev/null; then
  CARGO=cargo
else
  echo "ERROR: cargo not found for $build_user — install rustup stable"
  exit 1
fi

# runuser, not sudo -u: we are already root and sudo would re-prompt for a
# password when the build is launched via pkexec rather than sudo.
as_build_user() { runuser -u "$build_user" -- "$@"; }

echo "==> Build appsynergy-install (Rust)"
INSTALLER_SRC="$ROOT/installer"
INSTALLER_BIN="$PROFILE/airootfs/usr/local/bin/appsynergy-install"
as_build_user env HOME="$build_home" PATH="$build_home/.cargo/bin:/usr/bin:$PATH" \
  bash -c "cd '$INSTALLER_SRC' && $CARGO build --release"
install -m755 "$INSTALLER_SRC/target/release/appsynergy-install" "$INSTALLER_BIN"
file "$INSTALLER_BIN"
"$INSTALLER_BIN" --help | head -5 || true

# Refresh local packages into profile (kernel, branding, browsers). Everything
# comes from the staging repo: the kernel build stages itself there and
# fetch-repo.sh fills it from the published Release.
PKG_REPO="$MONO/packages/repo/x86_64"
# pacman.conf cannot express a relative path, so the committed profile carries a
# @PKG_REPO@ placeholder that is rendered into $WORK below and passed to
# mkarchiso -C. Assert the placeholder is still there: a literal path pasted
# back in would silently build against whatever that path holds.
grep -qxF 'Server = file://@PKG_REPO@' "$PROFILE/pacman.conf" || {
  echo "ERROR: $PROFILE/pacman.conf [appsynergy] must read 'Server = file://@PKG_REPO@'"
  echo "  actual: $(grep -n '^Server = file://' "$PROFILE/pacman.conf" || echo '<none>')"
  exit 1
}
DST_PKG="$PROFILE/airootfs/opt/appsynergy/pkgs"
mkdir -p "$DST_PKG"
# One kernel, both variants: appsynergy-linux. Version-anchored `-[0-9]*` so the
# package glob never swallows `-headers-`, same rule as the branding globs.
if compgen -G "$PKG_REPO/appsynergy-linux-[0-9]*.pkg.tar.zst" > /dev/null; then
  cp -a "$PKG_REPO"/appsynergy-linux-[0-9]*.pkg.tar.zst "$DST_PKG/" 2>/dev/null || true
  cp -a "$PKG_REPO"/appsynergy-linux-headers-[0-9]*.pkg.tar.zst "$DST_PKG/" 2>/dev/null || true
  # Retired per-CPU kernels must not ride along.
  rm -f "$DST_PKG"/linux-appsynergy-*.pkg.tar.zst "$DST_PKG"/linux-cachyos-igpu-*.pkg.tar.zst
else
  echo "WARN: no appsynergy-linux package in $PKG_REPO; run packages/scripts/fetch-repo.sh"
fi
# No separate server kernel: appsynergy-linux is both variants. The per-metal
# skylake/tigerlake packages are retired — see kernel/CLAUDE.md.
# AppSynergy identity packages (required offline after pacstrap).
# Globs are version-anchored with [0-9]: a bare appsynergy-branding-* also
# matches appsynergy-branding-desktop-*, which would corrupt both the copy below
# and the prune that follows (the prune would keep -desktop and delete identity).
declare -A PKG_GLOB=(
  [appsynergy-branding]='appsynergy-branding-[0-9]*.pkg.tar.zst'
  [appsynergy-branding-desktop]='appsynergy-branding-desktop-[0-9]*.pkg.tar.zst'
  [appsynergy-wallpapers]='appsynergy-wallpapers-[0-9]*.pkg.tar.zst'
  [appsynergy-mirrorlist]='appsynergy-mirrorlist-[0-9]*.pkg.tar.zst'
  [appsynergy-ca-certificates]='appsynergy-ca-certificates-[0-9]*.pkg.tar.zst'
  [appsynergy-keyring]='appsynergy-keyring-[0-9]*.pkg.tar.zst'
)
for src in "$PKG_REPO" \
           "$MONO/packages/pkgbuilds/appsynergy-branding" \
           "$MONO/packages/pkgbuilds/appsynergy-branding-desktop" \
           "$MONO/packages/pkgbuilds/appsynergy-wallpapers" \
           "$MONO/packages/pkgbuilds/appsynergy-mirrorlist" \
           "$MONO/packages/pkgbuilds/appsynergy-ca-certificates" \
           "$MONO/packages/pkgbuilds/appsynergy-keyring"; do
  for base in "${!PKG_GLOB[@]}"; do
    if compgen -G "$src/${PKG_GLOB[$base]}" > /dev/null; then
      cp -a "$src"/${PKG_GLOB[$base]} "$DST_PKG/" 2>/dev/null || true
    fi
  done
done
# do not ship dbg into ISO
rm -f "$DST_PKG"/*-dbg-*.pkg.tar.zst
# Keep only the newest release of each package: build-iso.sh only ever copies in,
# so stale versions accumulate and pacman -U on the target gets ambiguous input.
for base in "${!PKG_GLOB[@]}"; do
  mapfile -t old < <(ls -1v "$DST_PKG"/${PKG_GLOB[$base]} 2>/dev/null | head -n -1)
  for f in "${old[@]:-}"; do
    [[ -n "$f" ]] && { echo "  prune stale $(basename "$f")"; rm -f "$f"; }
  done
done
# Stray artifacts that must never reach the image
rm -f "$PROFILE/airootfs/usr/local/bin/appsynergy-install.bin"
echo "Local kernel/branding pkgs:"
ls -lh "$DST_PKG"/appsynergy-*.pkg.tar.zst 2>/dev/null || true

# Fail if kernel or branding missing when local kernel is expected
if ! compgen -G "$DST_PKG"/appsynergy-linux-[0-9]*.pkg.tar.zst > /dev/null; then
  echo "ERROR: no appsynergy-linux .pkg.tar.zst in $DST_PKG — aborting"
  exit 1
fi
if ! compgen -G "$DST_PKG"/appsynergy-linux-headers-[0-9]*.pkg.tar.zst > /dev/null; then
  echo "ERROR: appsynergy-linux-headers missing from $DST_PKG — aborting"
  exit 1
fi
if ! compgen -G "$DST_PKG"/appsynergy-branding-[0-9]*.pkg.tar.zst > /dev/null; then
  echo "ERROR: appsynergy-branding missing from $DST_PKG — aborting"
  exit 1
fi

# Pre-seeding a file that a repo package also owns makes pacstrap abort with
# "exists in filesystem". Catch it here instead of 20s into the package install.
echo "==> Check airootfs for package-owned files"
conflict_found=0
for pkg in "$PKG_REPO"/appsynergy-branding-[0-9]*.pkg.tar.zst \
           "$PKG_REPO"/appsynergy-branding-desktop-[0-9]*.pkg.tar.zst \
           "$PKG_REPO"/appsynergy-mirrorlist-[0-9]*.pkg.tar.zst; do
  [[ -f "$pkg" ]] || continue
  while read -r pf; do
    [[ -e "$PROFILE/airootfs$pf" ]] || continue
    echo "  CONFLICT: airootfs$pf is owned by $(basename "$pkg")"
    conflict_found=1
  done < <(tar tf "$pkg" 2>/dev/null | grep -v '/$' | grep -vE '^\.(PKGINFO|MTREE|BUILDINFO|INSTALL)' | sed 's|^|/|')
done
if (( conflict_found )); then
  echo "ERROR: airootfs pre-seeds package-owned files — delete them from the profile"
  echo "       (the package supplies them; see README 'no pre-seed of package-owned files')"
  exit 1
fi
echo "  none"

echo "==> Stage rescue CLIs (grok + claude) into live image"
# Preserve real user home under sudo
as_build_user env HOME="$build_home" bash "$ROOT/scripts/stage-rescue-clis.sh" \
  || bash "$ROOT/scripts/stage-rescue-clis.sh"
# Fail build if rescue tools missing (USB is for recovery)
if [[ ! -x "$PROFILE/airootfs/usr/local/bin/grok" || ! -x "$PROFILE/airootfs/usr/local/bin/claude" ]]; then
  echo "ERROR: grok and/or claude not staged into airootfs — aborting ISO build"
  ls -la "$PROFILE/airootfs/usr/local/bin/" || true
  exit 1
fi

echo "==> Stage k3s (server payload)"
as_build_user env HOME="$build_home" bash "$ROOT/scripts/stage-k3s.sh" \
  || bash "$ROOT/scripts/stage-k3s.sh"
if [[ ! -x "$PROFILE/airootfs/opt/appsynergy/k3s/k3s" ]]; then
  echo "ERROR: k3s binary not staged — server installs require it"
  exit 1
fi

mkdir -p "$OUT"
# Only remove WORK if empty/unused and CLEAN=1; never kill processes.
if [[ "${CLEAN:-0}" == "1" && -d $WORK ]]; then
  if findmnt "$WORK" >/dev/null 2>&1; then
    echo "ERROR: $WORK has mounts — pick a new WORK= path (do not force-clean)"
    exit 1
  fi
  rm -rf "$WORK"
fi
mkdir -p "$WORK"

# Pre-flight: a previous aborted run can leave chroot mounts behind. Squashing a
# tree with a live /proc under it packs /proc/kcore and never terminates.
stale_mounts=$(findmnt -rno TARGET | grep "^$ROOT/work-" || true)
if [[ -n "$stale_mounts" ]]; then
  echo "ERROR: stale mounts from a previous build — unmount before building:"
  echo "$stale_mounts"
  exit 1
fi

# mkarchiso never unmounts the chroot before mksquashfs; the shim does it.
export PATH="$ROOT/scripts/buildshims:$PATH"
command -v mksquashfs | grep -q buildshims \
  || { echo "ERROR: mksquashfs shim not on PATH"; exit 1; }

# Render the profile's pacman.conf with this checkout's staging path. The
# committed file keeps the placeholder; only this copy names a real directory.
sed "s|@PKG_REPO@|$PKG_REPO|" "$PROFILE/pacman.conf" > "$WORK/pacman.conf"
grep -qxF "Server = file://$PKG_REPO" "$WORK/pacman.conf" || {
  echo "ERROR: failed to render [appsynergy] Server into $WORK/pacman.conf"; exit 1; }

echo "==> mkarchiso -v -w $WORK -o $OUT -C $WORK/pacman.conf $PROFILE"
mkarchiso -v -w "$WORK" -o "$OUT" -C "$WORK/pacman.conf" "$PROFILE"

# Post-build: mkarchiso exits 0 even when the image is unusable.
ISO_PATH=$(ls -1t "$OUT"/*.iso 2>/dev/null | head -1 || true)
[[ -n "$ISO_PATH" && -f "$ISO_PATH" ]] || { echo "ERROR: no ISO produced"; exit 1; }
iso_bytes=$(stat -c %s "$ISO_PATH")
(( iso_bytes > 800000000 )) || { echo "ERROR: ISO only $iso_bytes bytes — truncated"; exit 1; }
echo "==> verify ISO structure (airootfs.sfs must be present and non-trivial)"
xorriso -indev "$ISO_PATH" -lsl /appsynergy/x86_64/ 2>/dev/null | grep -i airootfs \
  || { echo "ERROR: airootfs.sfs missing from ISO"; exit 1; }

echo
echo "ISO(s):"
ls -lh "$OUT"/*.iso
sha256sum "$ISO_PATH" | tee "$ISO_PATH.sha256"
echo
echo "Write example:"
echo "  sudo dd if=$OUT/appsynergy-linux-*.iso of=/dev/sdX bs=4M status=progress oflag=sync"
echo "  # or: sudo cp $OUT/appsynergy-linux-*.iso /dev/sdX   # not for all tools"
echo "  # recommended: sudo dd ... or balenaEtcher / usbimager"
