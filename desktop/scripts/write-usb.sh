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

# The target is typed by hand and dd is unrecoverable, so check it programmatically
# instead of trusting the operator to read the table above.
KNAME=$(lsblk -ndo KNAME "$DEV")
TRAN=$(lsblk -ndo TRAN "$DEV" | tr -d ' ')
RM=$(cat "/sys/block/$KNAME/removable" 2>/dev/null || echo 0)
if [[ "$RM" != 1 && "$TRAN" != usb ]]; then
  echo "Refusing $DEV: removable=$RM transport=${TRAN:-none} model=$(lsblk -ndo MODEL "$DEV")"
  echo "  That is an internal disk, not USB/removable media."
  [[ "${I_KNOW_ITS_NOT_USB:-0}" == 1 ]] || { echo "  Set I_KNOW_ITS_NOT_USB=1 to override."; exit 1; }
  echo "  I_KNOW_ITS_NOT_USB=1 — proceeding anyway."
fi
# Never overridable: the running system's own disk, and any live filesystem
# outside the removable-media mount roots (the umount below clears those).
ROOTDISK=$(lsblk -nsro NAME "$(findmnt -no SOURCE --nofsroot /)" 2>/dev/null | tail -1)
[[ "$KNAME" != "$ROOTDISK" ]] || { echo "Refusing $DEV: it is the disk backing / ($ROOTDISK)"; exit 1; }
mapfile -t MOUNTS < <(lsblk -nro MOUNTPOINTS "$DEV" | sed 's/\\x0a/\n/g' | grep -v '^$' || true)
for mp in "${MOUNTS[@]:-}"; do
  case "$mp" in ''|/run/media/*|/media/*|/mnt/*) continue ;; esac
  echo "Refusing $DEV: hosts a mounted filesystem at $mp"; exit 1
done

read -r -p "Type $DEV to wipe and write: " c
[[ "$c" == "$DEV" ]] || { echo "aborted"; exit 1; }

umount "${DEV}"* 2>/dev/null || true
dd if="$ISO" of="$DEV" bs=4M status=progress oflag=sync
sync
echo "Done. Reboot from USB (UEFI)."
