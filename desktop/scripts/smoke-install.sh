#!/usr/bin/bash
# Operator smoke for the installer disk layout. Not run in GitHub CI
# (ISO-in-CI stays forbidden). Needs root, sgdisk, cryptsetup, mkfs.btrfs.
#
# Proves the subvolume names the installer will create. A full USB/rescue
# install is still the proof of pacstrap, initrd unlock, and branding split:
#   server: lsinitcpio /boot/initramfs-appsynergy-linux.img | grep -E 'dropbear|authorized_keys'
#           pacman -Q appsynergy-branding-desktop  → not installed
#   desktop: pacman -Q plasma-desktop k3s          → plasma yes, k3s no
#   resume:  kill after pacstrap, re-invoke; journal in /run/appsynergy-install
set -euo pipefail

[[ $EUID -eq 0 ]] || { echo "run as root" >&2; exit 1; }
for b in sgdisk cryptsetup mkfs.fat mkfs.btrfs losetup; do
  command -v "$b" >/dev/null || { echo "missing $b" >&2; exit 1; }
done

WORK=$(mktemp -d /tmp/appsynergy-smoke.XXXXXX)
IMG="$WORK/disk.img"
MNT="$WORK/mnt"
KEY="$WORK/key"
LOOP=""
MAPPER="as-smoke-crypt"

cleanup() {
  umount -R "$MNT" 2>/dev/null || true
  cryptsetup close "$MAPPER" 2>/dev/null || true
  [[ -n "$LOOP" ]] && losetup -d "$LOOP" 2>/dev/null || true
  rm -rf "$WORK"
}
trap cleanup EXIT

printf 'smoke\n' > "$KEY"
truncate -s 3G "$IMG"
LOOP=$(losetup --find --show --partscan "$IMG")
# 256M EFI + rest LUKS — same partition order as the installer (p1 EFI, p2 LUKS).
sgdisk -Z "$LOOP" >/dev/null
sgdisk -n 1:0:+256M -t 1:ef00 -n 2:0:0 -t 2:8309 "$LOOP" >/dev/null
partprobe "$LOOP" 2>/dev/null || true
# wait for partition nodes
for _ in 1 2 3 4 5; do
  [[ -b ${LOOP}p2 ]] && break
  sleep 0.2
done
P1="${LOOP}p1"
P2="${LOOP}p2"
[[ -b $P1 && -b $P2 ]] || { echo "loop partitions missing under $LOOP" >&2; lsblk "$LOOP"; exit 1; }

mkfs.fat -F32 -n EFI "$P1" >/dev/null
cryptsetup luksFormat --type luks2 --batch-mode --key-file "$KEY" "$P2"
cryptsetup open --key-file "$KEY" "$P2" "$MAPPER"
mkfs.btrfs -L appsynergy-server "/dev/mapper/$MAPPER" >/dev/null
mkdir -p "$MNT"
mount "/dev/mapper/$MAPPER" "$MNT"
for sv in @ @home @var @log @cache @snapshots @srv; do
  btrfs subvolume create "$MNT/$sv" >/dev/null
done
found=$(btrfs subvolume list "$MNT" | awk '{print $NF}' | sort | tr '\n' ' ')
want="@ @cache @home @log @snapshots @srv @var "
[[ $found == "$want" ]] || { echo "subvols mismatch: got '$found' want '$want'" >&2; exit 1; }
echo "OK  loop LUKS+btrfs server subvols on $LOOP"
echo "    full USB/rescue install remains the operator proof (see header)"
