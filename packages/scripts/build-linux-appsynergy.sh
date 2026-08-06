#!/usr/bin/bash
# Build linux-appsynergy (+ headers) from the CachyOS PKGBUILD with AppSynergy suffix.
# Reuses the igpu-tuned config from the previous linux-cachyos-igpu build.
set -euo pipefail

KDIR="${KDIR:-/home/imma/src/linux-cachyos/linux-cachyos}"
OUT_REPO="${OUT_REPO:-$(cd "$(dirname "$0")/.." && pwd)/repo/x86_64}"
CFG_SRC="${CFG_SRC:-}"

# Pin assert — same contract as build-linux-appsynergy-server-flavor.sh.
PIN_FILE="$(cd "$(dirname "$0")/../.." && pwd)/kernel/upstream/PIN"
if [[ -f $PIN_FILE ]]; then
  want_commit=$(sed -n 's/^UPSTREAM_COMMIT=//p' "$PIN_FILE")
  have_commit=$(git -C "$KDIR" rev-parse --short HEAD 2>/dev/null || echo unknown)
  if [[ "$have_commit" != "$want_commit" ]]; then
    echo "PIN MISMATCH: KDIR at $have_commit, pin says $want_commit ($PIN_FILE)"
    [[ "${PIN_OVERRIDE:-0}" == "1" ]] || { echo "set PIN_OVERRIDE=1 to build anyway, then update the pin"; exit 1; }
    echo "PIN_OVERRIDE=1 — building unpinned"
  fi
fi

cd "$KDIR"

# Prefer the last known-good igpu config
if [[ -z $CFG_SRC ]]; then
  for c in config-*-cachyos-igpu config-*-appsynergy config; do
    # shellcheck disable=SC2086
    if compgen -G "$c" >/dev/null; then
      CFG_SRC=$(ls -1t $c 2>/dev/null | head -1)
      break
    fi
  done
fi
[[ -n $CFG_SRC && -f $CFG_SRC ]] || { echo "No kernel config found in $KDIR"; exit 1; }

echo "==> Using config: $CFG_SRC"
cp -a "$CFG_SRC" "$KDIR/config"

# Package / uname suffix: linux-appsynergy → 7.x.y-N-appsynergy
export _pkgsuffix=appsynergy
export _use_lto_suffix=no
export _use_gcc_suffix=no

echo "==> Building pkgbase=linux-appsynergy (this takes a long time)"
# -f rebuild; skip deps if already installed
makepkg -f --noconfirm 2>&1 | tee /tmp/linux-appsynergy-build.log

mkdir -p "$OUT_REPO"
shopt -s nullglob
for f in linux-appsynergy-*.pkg.tar.zst linux-appsynergy-headers-*.pkg.tar.zst; do
  [[ -f $f ]] || continue
  [[ $f == *dbg* ]] && continue
  cp -a "$f" "$OUT_REPO/"
  echo "    staged $f → $OUT_REPO"
done
shopt -u nullglob

echo "Done. Install with:"
echo "  sudo pacman -U $OUT_REPO/linux-appsynergy-*.pkg.tar.zst $OUT_REPO/linux-appsynergy-headers-*.pkg.tar.zst"
echo "Then: sudo bootctl update; reboot"
