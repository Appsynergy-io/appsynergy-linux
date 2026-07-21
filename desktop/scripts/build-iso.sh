#!/usr/bin/bash
# Build AppSynergy Linux installer ISO (requires root + archiso).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE="$ROOT/iso"
OUT="$ROOT/out"
WORK="$ROOT/work"

[[ "$(id -u)" -eq 0 ]] || { echo "Run as root: sudo $0"; exit 1; }
command -v mkarchiso >/dev/null || { echo "install archiso"; exit 1; }

# Refresh local kernel packages into profile if present on build host
SRC_PKG=/home/imma/src/linux-cachyos/linux-cachyos
DST_PKG="$PROFILE/airootfs/opt/appsynergy/pkgs"
mkdir -p "$DST_PKG"
# Prefer linux-appsynergy; keep legacy igpu name as fallback
if compgen -G "$SRC_PKG/linux-appsynergy-[0-9]*.pkg.tar.zst" > /dev/null; then
  cp -a "$SRC_PKG"/linux-appsynergy-[0-9]*.pkg.tar.zst "$DST_PKG/" 2>/dev/null || true
  cp -a "$SRC_PKG"/linux-appsynergy-headers-*.pkg.tar.zst "$DST_PKG/" 2>/dev/null || true
elif compgen -G "$SRC_PKG/linux-cachyos-igpu-[0-9]*.pkg.tar.zst" > /dev/null; then
  cp -a "$SRC_PKG"/linux-cachyos-igpu-[0-9]*.pkg.tar.zst "$DST_PKG/" 2>/dev/null || true
  cp -a "$SRC_PKG"/linux-cachyos-igpu-headers-*.pkg.tar.zst "$DST_PKG/" 2>/dev/null || true
else
  echo "WARN: no local kernel packages (linux-appsynergy or linux-cachyos-igpu); use --kernel repo"
fi
# do not ship dbg into ISO
rm -f "$DST_PKG"/*-dbg-*.pkg.tar.zst
echo "Local kernel pkgs:"
ls -lh "$DST_PKG"/linux-*.pkg.tar.zst 2>/dev/null || true

# Browsers (no Firefox): pull brave from pacman cache if present
if compgen -G /var/cache/pacman/pkg/brave-bin-*.pkg.tar.zst > /dev/null; then
  cp -a /var/cache/pacman/pkg/brave-bin-*.pkg.tar.zst "$DST_PKG/"
fi
# Thorium is AUR — drop a built package here if you have one:
#   cp thorium-browser-bin-*.pkg.tar.zst iso/airootfs/opt/appsynergy/pkgs/
if compgen -G /var/cache/pacman/pkg/thorium-browser-bin-*.pkg.tar.zst > /dev/null; then
  cp -a /var/cache/pacman/pkg/thorium-browser-bin-*.pkg.tar.zst "$DST_PKG/"
fi
echo "Local browser pkgs in image:"
ls -lh "$DST_PKG"/brave-bin-* "$DST_PKG"/thorium-browser-bin-* 2>/dev/null || echo "  (brave/thorium none yet)"

echo "==> Stage rescue CLIs (grok + claude) into live image"
# Preserve real user home under sudo
if [[ -n "${SUDO_USER:-}" ]]; then
  sudo -u "$SUDO_USER" bash "$ROOT/scripts/stage-rescue-clis.sh" \
    || bash "$ROOT/scripts/stage-rescue-clis.sh"
else
  bash "$ROOT/scripts/stage-rescue-clis.sh"
fi
# Fail build if rescue tools missing (USB is for recovery)
if [[ ! -x "$PROFILE/airootfs/usr/local/bin/grok" || ! -x "$PROFILE/airootfs/usr/local/bin/claude" ]]; then
  echo "ERROR: grok and/or claude not staged into airootfs — aborting ISO build"
  ls -la "$PROFILE/airootfs/usr/local/bin/" || true
  exit 1
fi

mkdir -p "$OUT"
# clean work for repeatable builds (slow but safe)
if [[ "${CLEAN:-1}" == "1" ]]; then
  rm -rf "$WORK"
fi
mkdir -p "$WORK"

echo "==> mkarchiso -v -w $WORK -o $OUT $PROFILE"
mkarchiso -v -w "$WORK" -o "$OUT" "$PROFILE"

echo
echo "ISO(s):"
ls -lh "$OUT"/*.iso
echo
echo "Write example:"
echo "  sudo dd if=$OUT/appsynergy-linux-*.iso of=/dev/sdX bs=4M status=progress oflag=sync"
echo "  # or: sudo cp $OUT/appsynergy-linux-*.iso /dev/sdX   # not for all tools"
echo "  # recommended: sudo dd ... or balenaEtcher / usbimager"
