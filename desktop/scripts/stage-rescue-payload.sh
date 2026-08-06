#!/usr/bin/bash
# Stage a downloadable payload for OVH (or any) rescue install — no process kills.
# Output: out/appsynergy-server-rescue-YYYYMMDD.tar.zst + out/appsynergy-server-rescue/
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="${REPO:-$(cd "$ROOT/.." && pwd)/packages/repo/x86_64}"
STAMP=$(date +%Y%m%d)
OUT_DIR="$ROOT/out/appsynergy-server-rescue"
TAR="$ROOT/out/appsynergy-server-rescue-${STAMP}.tar.zst"

mkdir -p "$OUT_DIR"/{pkgs,etc,docs,bootstrap,k3s}
# FLAVOUR=all keeps both server kernels; default ships only what the target needs.
# The payload is copied to the box over the network, so every extra 150MB is
# transfer time during an outage.
FLAVOUR="${FLAVOUR:-skylake}"
# kernels
shopt -s nullglob
if [[ $FLAVOUR == all ]]; then
  KFLAVOURS=(skylake tigerlake)
else
  KFLAVOURS=("$FLAVOUR")
fi
srcs=()
for kf in "${KFLAVOURS[@]}"; do
  srcs+=( "$REPO"/linux-appsynergy-server-${kf}-*.pkg.tar.zst )
  srcs+=( "$ROOT"/iso/airootfs/opt/appsynergy/pkgs/linux-appsynergy-server-${kf}-*.pkg.tar.zst )
done
for f in "${srcs[@]}"; do
  [[ -f $f ]] || continue
  [[ $f == *dbg* ]] && continue
  cp -a "$f" "$OUT_DIR/pkgs/"
done
shopt -u nullglob

# AppSynergy support packages a server pull needs offline. Globs are
# version-anchored with [0-9]: a bare appsynergy-branding-* also matches
# appsynergy-branding-desktop-*, which would corrupt both the copy and the
# prune below (the prune would keep -desktop and delete the identity package).
declare -A PKG_GLOB=(
  [appsynergy-branding]='appsynergy-branding-[0-9]*.pkg.tar.zst'
  [appsynergy-mirrorlist]='appsynergy-mirrorlist-[0-9]*.pkg.tar.zst'
  [appsynergy-ca-certificates]='appsynergy-ca-certificates-[0-9]*.pkg.tar.zst'
  [appsynergy-keyring]='appsynergy-keyring-[0-9]*.pkg.tar.zst'
)
for src in "$REPO" "$ROOT/iso/airootfs/opt/appsynergy/pkgs"; do
  for base in "${!PKG_GLOB[@]}"; do
    if compgen -G "$src/${PKG_GLOB[$base]}" > /dev/null; then
      cp -a "$src"/${PKG_GLOB[$base]} "$OUT_DIR/pkgs/" 2>/dev/null || true
    fi
  done
done
rm -f "$OUT_DIR/pkgs/"*-dbg-*.pkg.tar.zst

# Keep only the newest release of each package. This script only ever copies in,
# so without a prune the payload accumulates every historical build and pacman -U
# gets ambiguous input (the same defect that shipped branding 2-11 on the ISO).
for base in "${!PKG_GLOB[@]}"; do
  mapfile -t old < <(ls -1v "$OUT_DIR/pkgs/"${PKG_GLOB[$base]} 2>/dev/null | head -n -1)
  for f in "${old[@]:-}"; do
    [[ -n "$f" ]] && { echo "  prune stale $(basename "$f")"; rm -f "$f"; }
  done
done

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

# Prefer the freshly built binary over whatever is staged in the ISO profile:
# the profile copy can lag behind the source tree.
if [[ -x $ROOT/installer/target/release/appsynergy-install ]]; then
  cp -a "$ROOT/installer/target/release/appsynergy-install" "$OUT_DIR/"
elif [[ -x $ROOT/iso/airootfs/usr/local/bin/appsynergy-install ]]; then
  cp -a "$ROOT/iso/airootfs/usr/local/bin/appsynergy-install" "$OUT_DIR/"
fi

# k3s: rescue-install.sh wires this into the chroot; the installer hard-fails a
# server install without /opt/appsynergy/k3s/k3s. stage-k3s.sh verifies the
# pinned sha256. k3s.service.env is excluded — secrets never travel in the
# tarball — and replaced by an empty 0600 placeholder.
bash "$ROOT/scripts/stage-k3s.sh"
for f in "$ROOT/iso/airootfs/opt/appsynergy/k3s"/*; do
  [[ $(basename "$f") == k3s.service.env ]] && continue
  cp -a "$f" "$OUT_DIR/k3s/"
done
: >"$OUT_DIR/k3s/k3s.service.env"
chmod 600 "$OUT_DIR/k3s/k3s.service.env"

# Arch bootstrap: OVH rescue is Debian and has no pacstrap/arch-chroot. Without
# this the install cannot proceed at all, so its absence is a hard failure.
BOOTSTRAP=$(ls -1t "$ROOT"/out/*bootstrap*.tar.zst 2>/dev/null | head -1 || true)
if [[ -n "$BOOTSTRAP" && -f "$BOOTSTRAP" ]]; then
  cp -a "$BOOTSTRAP" "$OUT_DIR/bootstrap/"
else
  echo "ERROR: no bootstrap tarball in out/ — run scripts/build-bootstrap.sh first" >&2
  exit 1
fi

# Operator-facing scripts travel with the payload.
install -Dm755 "$ROOT/scripts/rescue-preflight.sh" "$OUT_DIR/rescue-preflight.sh"
install -Dm755 "$ROOT/scripts/rescue-install.sh"   "$OUT_DIR/rescue-install.sh"

# Gate: a payload missing any of these strands the operator mid-outage.
fail=0
[[ -s "$OUT_DIR/etc/ssh-unlock.pub" ]] || { echo "ERROR: etc/ssh-unlock.pub missing — headless host could not be unlocked" >&2; fail=1; }
[[ -x "$OUT_DIR/appsynergy-install" ]] || { echo "ERROR: appsynergy-install missing" >&2; fail=1; }
[[ -s "$OUT_DIR/etc/packages-target-server.txt" ]] || { echo "ERROR: packages-target-server.txt missing" >&2; fail=1; }
compgen -G "$OUT_DIR/pkgs/linux-appsynergy-server-*.pkg.tar.zst" >/dev/null \
  || { echo "ERROR: no server kernel package staged" >&2; fail=1; }
[[ -x "$OUT_DIR/k3s/k3s" ]] || { echo "ERROR: k3s/k3s missing — server install dies after pacstrap without it" >&2; fail=1; }
compgen -G "$OUT_DIR/pkgs/appsynergy-keyring-[0-9]*.pkg.tar.zst" >/dev/null \
  || { echo "ERROR: appsynergy-keyring package missing" >&2; fail=1; }
(( fail == 0 )) || exit 1

# Checksums (paths only; the only key here is a public one).
# Build outside the payload dir, then move in: redirecting straight into
# $OUT_DIR/SHA256SUMS creates the empty file *before* find walks the tree, so it
# hashes itself and `sha256sum -c` then fails on every payload.
SUMS_TMP="$(mktemp)"
(
  cd "$OUT_DIR"
  find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum
) >"$SUMS_TMP"
mv "$SUMS_TMP" "$OUT_DIR/SHA256SUMS"
chmod 644 "$OUT_DIR/SHA256SUMS"
# Prove it verifies now, rather than discovering it on the rescue host.
( cd "$OUT_DIR" && sha256sum -c --quiet SHA256SUMS ) \
  || { echo "ERROR: payload checksums do not verify" >&2; exit 1; }
echo "  SHA256SUMS verified ($(wc -l <"$OUT_DIR/SHA256SUMS") files)"

mkdir -p "$ROOT/out"
tar -C "$ROOT/out" -c appsynergy-server-rescue | zstd -T0 -19 -o "$TAR"
ls -lah "$TAR" "$OUT_DIR"
echo "Payload: $TAR"
echo "Unpacked: $OUT_DIR"
