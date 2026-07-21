#!/usr/bin/bash
# Write the newest AppSynergy ISO to a USB block device.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ISO=$(ls -1t "$ROOT"/out/appsynergy-linux-*.iso 2>/dev/null | head -1 || true)
[[ -n "$ISO" && -f "$ISO" ]] || { echo "No ISO in $ROOT/out — run scripts/build-iso.sh first"; exit 1; }
[[ $# -eq 1 ]] || { echo "Usage: sudo $0 /dev/sdX   (USB whole disk, not partition)"; exit 1; }
DEV="$1"
[[ -b "$DEV" ]] || { echo "Not a block device: $DEV"; exit 1; }
[[ "$(id -u)" -eq 0 ]] || { echo "run as root"; exit 1; }

echo "ISO: $ISO ($(du -h "$ISO" | awk '{print $1}'))"
lsblk -o NAME,SIZE,MODEL,TRAN,MOUNTPOINTS "$DEV"
read -r -p "Type $DEV to wipe and write: " c
[[ "$c" == "$DEV" ]] || { echo "aborted"; exit 1; }

umount "${DEV}"* 2>/dev/null || true
dd if="$ISO" of="$DEV" bs=4M status=progress oflag=sync
sync
echo "Done. Reboot from USB (UEFI)."
