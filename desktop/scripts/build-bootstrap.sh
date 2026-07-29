#!/usr/bin/bash
# Build the Arch bootstrap tarball used to install from a non-Arch rescue system.
# OVH rescue is Debian: no pacstrap, no arch-chroot. This tarball supplies them.
# Output: out/appsynergy-bootstrap-<ver>-x86_64.tar.zst
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PROFILE="$ROOT/iso"
OUT="${OUT:-$ROOT/out}"
WORK="${WORK:-$ROOT/work-bootstrap-$(date +%Y%m%d-%H%M%S)}"

[[ "$(id -u)" -eq 0 ]] || { echo "Run as root: sudo $0"; exit 1; }
command -v mkarchiso >/dev/null || { echo "install archiso"; exit 1; }

stale_mounts=$(findmnt -rno TARGET | grep "^$ROOT/work-" || true)
if [[ -n "$stale_mounts" ]]; then
  echo "ERROR: stale mounts from a previous build:"; echo "$stale_mounts"; exit 1
fi

# Same chroot-/proc-leak guard as the ISO build; bootstrap uses bsdtar, not
# mksquashfs, but the shim is harmless and keeps one code path.
export PATH="$ROOT/scripts/buildshims:$PATH"

mkdir -p "$OUT" "$WORK"
echo "==> mkarchiso -m bootstrap -w $WORK -o $OUT"
mkarchiso -v -m bootstrap -w "$WORK" -o "$OUT" "$PROFILE"

TARBALL=$(ls -1t "$OUT"/*bootstrap*.tar.zst 2>/dev/null | head -1 || true)
[[ -n "$TARBALL" && -f "$TARBALL" ]] || { echo "ERROR: no bootstrap tarball produced"; exit 1; }

# Verify the tarball actually carries what the installer calls. A bootstrap that
# is missing sgdisk is worse than no bootstrap: it fails after the operator has
# already booted into rescue and committed.
echo "==> verifying required binaries are present in the tarball"
# --long=31 is mandatory: profiledef compresses with `--long`, and a plain
# `zstd -dc` fails with "Frame requires too much memory". The rescue host must
# use the same flag; see docs/RESCUE-INSTALL.md.
listing=$(zstd -dc --long=31 "$TARBALL" | tar -t 2>/dev/null)
missing=0
# Here-string, not `printf | grep -q`: grep -q exits at the first match, printf
# then takes SIGPIPE, and `set -o pipefail` reports the whole pipeline as failed —
# turning a successful match into a false "MISSING".
for b in pacstrap arch-chroot genfstab sgdisk cryptsetup mkfs.fat mkfs.btrfs bootctl blkid efibootmgr mkinitcpio; do
  if grep -qE "(usr/s?bin|bin)/${b}$" <<<"$listing"; then
    echo "    ok   $b"
  else
    echo "    MISSING $b"
    missing=1
  fi
done
(( missing == 0 )) || { echo "ERROR: bootstrap tarball incomplete — fix iso/bootstrap_packages"; exit 1; }

# ---------------------------------------------------------------------------
# Post-process: a stock bootstrap tarball ships an EMPTY pacman keyring and no
# mirrorlist, so pacstrap on the rescue host fails with signature errors and
# "no servers configured". Both are fixed here so nothing is improvised at
# install time on a box you can only reach over SSH.
# ---------------------------------------------------------------------------
REPACK="$WORK/repack"
rm -rf "$REPACK"; mkdir -p "$REPACK"
echo "==> post-processing: keyring + mirrorlist"
zstd -dc --long=31 "$TARBALL" | tar -C "$REPACK" -x

R="$REPACK/root.x86_64"
[[ -d "$R" ]] || { echo "ERROR: unexpected tarball layout (no root.x86_64)"; exit 1; }

# Seed a known-good mirrorlist from this build host rather than shipping an
# empty one. Deterministic: whatever built the ISO also feeds the bootstrap.
if grep -qE '^\s*Server\s*=' /etc/pacman.d/mirrorlist; then
  install -Dm644 /etc/pacman.d/mirrorlist "$R/etc/pacman.d/mirrorlist"
  echo "    mirrorlist: seeded ($(grep -cE '^\s*Server\s*=' "$R/etc/pacman.d/mirrorlist") servers)"
else
  echo "ERROR: build host /etc/pacman.d/mirrorlist has no active Server lines"; exit 1
fi

cleanup_repack() {
  umount -R "$R/proc" "$R/sys" "$R/dev" 2>/dev/null || true
}
trap cleanup_repack EXIT

mount --bind /proc "$R/proc"
mount --bind /sys  "$R/sys"
mount --bind /dev  "$R/dev"
cp /etc/resolv.conf "$R/etc/resolv.conf"

chroot "$R" /usr/bin/bash -c '
  set -e
  pacman-key --init
  pacman-key --populate archlinux
' || { echo "ERROR: keyring init/populate failed"; exit 1; }

keys=$(chroot "$R" /usr/bin/bash -c 'pacman-key --list-keys 2>/dev/null | grep -c "^pub"' || true)
(( keys > 0 )) || { echo "ERROR: keyring still empty after populate"; exit 1; }
echo "    keyring: $keys archlinux keys"

# Do not bake the build host's DNS into the image.
: > "$R/etc/resolv.conf"

cleanup_repack
trap - EXIT

echo "==> repacking"
rm -f "$TARBALL"
( cd "$REPACK" && tar -c . ) | zstd -T0 --long -19 -o "$TARBALL" -f


sha256sum "$TARBALL" | tee "$TARBALL.sha256"
ls -lh "$TARBALL"
echo "Bootstrap: $TARBALL"
