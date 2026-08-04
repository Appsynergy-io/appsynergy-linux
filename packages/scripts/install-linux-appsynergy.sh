#!/usr/bin/bash
# Install freshly built linux-appsynergy packages and prefer them in boot.
set -euo pipefail

# Monorepo root: sibling subtree desktop/ lives beside packages/.
MONO="$(cd "$(dirname "$0")/../.." && pwd)"
KDIR="${KDIR:-/home/imma/src/linux-cachyos/linux-cachyos}"
REPO="${REPO:-$(cd "$(dirname "$0")/.." && pwd)/repo/x86_64}"
cd "$KDIR"
shopt -s nullglob
pkgs=(linux-appsynergy-[0-9]*.pkg.tar.zst linux-appsynergy-headers-*.pkg.tar.zst)
# drop dbg
filtered=()
for p in "${pkgs[@]}"; do
  [[ $p == *dbg* ]] && continue
  filtered+=("$p")
done
((${#filtered[@]} >= 2)) || { echo "missing packages in $KDIR"; ls -la linux-appsynergy* 2>/dev/null; exit 1; }
mkdir -p "$REPO"
cp -a "${filtered[@]}" "$REPO/"
echo "Installing: ${filtered[*]}"
sudo pacman -U --noconfirm "${filtered[@]}"
# Stage into desktop ISO payload
ISO_PKGS="$MONO/desktop/iso/airootfs/opt/appsynergy/pkgs"
if [[ -d $ISO_PKGS ]]; then
  cp -a "${filtered[@]}" "$ISO_PKGS/"
  # remove legacy igpu from ISO payload if appsynergy present
  rm -f "$ISO_PKGS"/linux-cachyos-igpu-*.pkg.tar.zst
fi
# Rebuild initramfs / boot
if command -v mkinitcpio >/dev/null; then
  sudo mkinitcpio -P || true
fi
if command -v bootctl >/dev/null; then
  sudo bootctl update || true
fi
echo
echo "Installed. Current uname still old until reboot:"
uname -r
echo "New modules:"
ls -d /usr/lib/modules/*appsynergy* 2>/dev/null || true
echo "Reboot to run: linux-*-appsynergy"
