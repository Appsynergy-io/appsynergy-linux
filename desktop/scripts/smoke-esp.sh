#!/usr/bin/bash
# Exercise the installed sync helper on disposable FAT loop devices.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")" && pwd)
if [[ ${1:-} != --isolated ]]; then
  [[ $EUID == 0 ]] || { echo 'run with sudo' >&2; exit 1; }
  exec unshare --mount "$0" --isolated
fi
mount --make-rprivate /
work=$(mktemp -d)
loops=()
cleanup() {
  umount /boot 2>/dev/null || true
  umount "$work/secondary" 2>/dev/null || true
  for loop in "${loops[@]}"; do losetup -d "$loop"; done
  rm -rf "$work"
}
trap cleanup EXIT
for n in 0 1; do
  truncate -s 128M "$work/$n.img"
  loop=$(losetup --find --show "$work/$n.img")
  loops+=("$loop")
  mkfs.fat -F32 "$loop" >/dev/null
 done
udevadm settle
primary=$(blkid -s UUID -o value "${loops[0]}")
secondary=$(blkid -s UUID -o value "${loops[1]}")
# Shadow /etc only inside this mount namespace; the helper reads its normal path.
mount -t tmpfs tmpfs /etc
mkdir -p /etc/appsynergy
printf '%s\n%s\n' "$primary" "$secondary" > /etc/appsynergy/esp-mirror
umount /boot 2>/dev/null || true
mount "${loops[0]}" /boot
mkdir -p /boot/loader/entries
printf 'kernel-v1\n' > /boot/vmlinuz-test
printf 'initrd-v1\n' > /boot/initramfs-test.img
printf 'test entry\n' > /boot/loader/entries/test.conf
"$ROOT/sync-esp.sh"
printf 'kernel-v2\n' > /boot/vmlinuz-test
printf 'initrd-v2\n' > /boot/initramfs-test.img
"$ROOT/sync-esp.sh"
mkdir -p "$work/secondary"
mount "${loops[1]}" "$work/secondary"
cmp /boot/vmlinuz-test "$work/secondary/vmlinuz-test"
cmp /boot/initramfs-test.img "$work/secondary/initramfs-test.img"
if "$ROOT/sync-esp.sh" 2>/dev/null; then echo 'Double mount was accepted' >&2; exit 1; fi
umount "$work/secondary"
# Fail closed if /boot is not the original filesystem.
umount /boot
mount "${loops[1]}" /boot
if "$ROOT/sync-esp.sh" 2>/dev/null; then echo 'Wrong primary was accepted' >&2; exit 1; fi
echo 'PASS: EFI initial copy, kernel/initramfs update, double-mount and wrong-primary refusal'
