#!/usr/bin/bash
# Installed on RAID1 targets; runs after package updates and mkinitcpio rebuilds.
set -euo pipefail
exec 9>/run/appsynergy-sync-esp.lock
flock 9
mapfile -t ids < /etc/appsynergy/esp-mirror
[[ ${#ids[@]} -eq 2 && ${ids[0]} != "${ids[1]}" ]] || { echo 'Invalid ESP mirror identities' >&2; exit 1; }
for id in "${ids[@]}"; do
  [[ $id =~ ^[0-9A-Fa-f-]+$ ]] || { echo 'Invalid ESP UUID' >&2; exit 1; }
done
primary=${ids[0]}
secondary=${ids[1]}
[[ $(findmnt -rn --mountpoint /boot -o UUID) == "$primary" ]] || {
  echo 'Refusing ESP sync: /boot is not the recorded primary EFI partition' >&2; exit 1;
}
[[ -b /dev/disk/by-uuid/$secondary ]] || {
  echo 'Failover EFI partition missing: replace the disk and repair boot redundancy' >&2; exit 1;
}
# Never double-mount a filesystem or write through an unrelated mount.
if findmnt -rn -S "UUID=$secondary" >/dev/null; then
  echo 'Failover EFI partition already mounted; unmount it before retrying sync' >&2; exit 1
fi
mnt=$(mktemp -d /run/appsynergy-esp.XXXXXX)
mounted=0
cleanup() {
  if (( mounted )); then umount "$mnt" || return; fi
  rmdir "$mnt"
}
trap cleanup EXIT
mount -o umask=0077 "UUID=$secondary" "$mnt"
mounted=1
# FAT does not support Unix ownership, permissions or hard links.
rsync -rct --delete --exclude=/loader/random-seed /boot/ "$mnt/"
sync -f "$mnt"
# Verify bytes, including loader entries and EFI executables, not just timestamps.
changes=$(rsync -rcn --delete --itemize-changes --exclude=/loader/random-seed /boot/ "$mnt/")
[[ -z $changes ]] || { echo "Failover EFI verification failed: $changes" >&2; exit 1; }
echo 'Failover EFI partition synchronized and verified'
