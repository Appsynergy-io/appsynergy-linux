#!/usr/bin/bash
# Stage a downloadable payload for OVH (or any) rescue install — no process kills.
# Output: out/appsynergy-server-rescue-YYYYMMDD.tar.zst + out/appsynergy-server-rescue/
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="${REPO:-/home/imma/projects/appsynergy-packages/repo/x86_64}"
STAMP=$(date +%Y%m%d)
OUT_DIR="$ROOT/out/appsynergy-server-rescue"
TAR="$ROOT/out/appsynergy-server-rescue-${STAMP}.tar.zst"

mkdir -p "$OUT_DIR"/{pkgs,etc,docs}
# kernels
shopt -s nullglob
for f in \
  "$REPO"/linux-appsynergy-server-skylake-*.pkg.tar.zst \
  "$REPO"/linux-appsynergy-server-skylake-headers-*.pkg.tar.zst \
  "$REPO"/linux-appsynergy-server-tigerlake-*.pkg.tar.zst \
  "$REPO"/linux-appsynergy-server-tigerlake-headers-*.pkg.tar.zst \
  "$ROOT"/iso/airootfs/opt/appsynergy/pkgs/linux-appsynergy-server-skylake-*.pkg.tar.zst \
  "$ROOT"/iso/airootfs/opt/appsynergy/pkgs/linux-appsynergy-server-skylake-headers-*.pkg.tar.zst \
  "$ROOT"/iso/airootfs/opt/appsynergy/pkgs/linux-appsynergy-server-tigerlake-*.pkg.tar.zst \
  "$ROOT"/iso/airootfs/opt/appsynergy/pkgs/linux-appsynergy-server-tigerlake-headers-*.pkg.tar.zst \
  "$REPO"/appsynergy-branding-*.pkg.tar.zst \
  "$ROOT"/iso/airootfs/opt/appsynergy/pkgs/appsynergy-branding-*.pkg.tar.zst \
  "$REPO"/appsynergy-mirrorlist-*.pkg.tar.zst \
  "$ROOT"/iso/airootfs/opt/appsynergy/pkgs/appsynergy-mirrorlist-*.pkg.tar.zst
do
  [[ -f $f ]] || continue
  [[ $f == *dbg* ]] && continue
  cp -a "$f" "$OUT_DIR/pkgs/"
done
shopt -u nullglob

# configs from live profile
AS="$ROOT/iso/airootfs/etc/appsynergy"
for f in \
  packages-target-server.txt \
  machine-server.env \
  sysctl-server.conf \
  modules-load-server.conf \
  server-nftables.conf \
  ssh-unlock.pub \
  PASSWORD-AND-TPM.md
do
  [[ -f $AS/$f ]] && cp -a "$AS/$f" "$OUT_DIR/etc/"
done
if [[ -d $AS/server ]]; then
  mkdir -p "$OUT_DIR/etc/server"
  cp -a "$AS"/server/* "$OUT_DIR/etc/server/" 2>/dev/null || true
fi
if [[ -d $AS/server-network ]]; then
  mkdir -p "$OUT_DIR/etc/server-network"
  cp -a "$AS"/server-network/* "$OUT_DIR/etc/server-network/" 2>/dev/null || true
fi

# step-by-step (source of truth for rescue, not a wipe script)
cp -a "$ROOT/docs/RESCUE-INSTALL.md" "$OUT_DIR/docs/" 2>/dev/null \
  || cp -a "$ROOT/iso/airootfs/etc/appsynergy/RESCUE-INSTALL.md" "$OUT_DIR/docs/" 2>/dev/null \
  || true

# installer binary for reference (live-oriented; rescue steps are in the doc)
if [[ -x $ROOT/iso/airootfs/usr/local/bin/appsynergy-install ]]; then
  cp -a "$ROOT/iso/airootfs/usr/local/bin/appsynergy-install" "$OUT_DIR/"
elif [[ -x $ROOT/installer/target/release/appsynergy-install ]]; then
  cp -a "$ROOT/installer/target/release/appsynergy-install" "$OUT_DIR/"
fi

# checksums (paths only; no secrets printed beyond pubkey file which is public)
(
  cd "$OUT_DIR"
  find . -type f -print0 | sort -z | xargs -0 sha256sum
) >"$OUT_DIR/SHA256SUMS"

mkdir -p "$ROOT/out"
tar -C "$ROOT/out" -c appsynergy-server-rescue | zstd -T0 -19 -o "$TAR"
ls -lah "$TAR" "$OUT_DIR"
echo "Payload: $TAR"
echo "Unpacked: $OUT_DIR"
