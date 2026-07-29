#!/usr/bin/env bash
# Run FIRST on the OVH (or any) rescue system. Reports facts, changes nothing.
# Purpose: replace assumptions about disks/NICs/firmware with observed values
# before anything destructive runs. Safe to run repeatedly.
#
#   bash rescue-preflight.sh
#
# Exit 0 = looks installable. Exit 1 = something needs a human decision.
set -uo pipefail

warn=0
note() { printf '  %s\n' "$*"; }
head2() { printf '\n== %s ==\n' "$*"; }
bad()  { printf '  !! %s\n' "$*"; warn=1; }

printf '=== AppSynergy server preflight — %s ===\n' "$(date -Is 2>/dev/null || date)"
note "host: $(uname -srm)"
note "rescue distro: $(. /etc/os-release 2>/dev/null && echo "${PRETTY_NAME:-unknown}" || echo unknown)"

head2 "Firmware mode"
if [[ -d /sys/firmware/efi ]]; then
  note "UEFI (required — installer writes an ESP + systemd-boot)"
else
  bad "Legacy BIOS: no /sys/firmware/efi. The installer assumes UEFI; stop and reassess."
fi

head2 "CPU / RAM"
note "cpu:  $(grep -m1 '^model name' /proc/cpuinfo 2>/dev/null | cut -d: -f2- | sed 's/^ //')"
note "cores: $(nproc 2>/dev/null || echo '?')"
note "ram:  $(awk '/MemTotal/{printf "%.1f GiB\n",$2/1048576}' /proc/meminfo 2>/dev/null)"
# Which server kernel flavour applies
if grep -qiE 'E3-1270|Skylake|Kaby' /proc/cpuinfo 2>/dev/null; then
  note "flavour hint: skylake"
elif grep -qiE '1185G7|Tiger' /proc/cpuinfo 2>/dev/null; then
  note "flavour hint: tigerlake"
else
  note "flavour hint: unrecognised CPU — choose the kernel flavour explicitly"
fi

head2 "TPM"
if [[ -e /dev/tpm0 || -e /dev/tpmrm0 ]]; then
  note "TPM present — automatic LUKS unlock possible"
else
  note "no TPM — unlock will rely on the initrd SSH key (expected on rented metal)"
fi

head2 "Disks"
lsblk -dno NAME,SIZE,TYPE,MODEL 2>/dev/null | while read -r l; do note "$l"; done
ndisks=$(lsblk -dno NAME,TYPE 2>/dev/null | awk '$2=="disk"' | wc -l)
note "whole disks visible: $ndisks"
case "$ndisks" in
  0) bad "no disks detected — cannot install" ;;
  1) note "single disk: RAID1 not possible; installer will use a single-disk layout" ;;
  2) note "two disks: btrfs RAID1 layout applies" ;;
  *) note "more than two disks: pass --disk explicitly so the right pair is chosen" ;;
esac

head2 "Existing data on those disks (MUST review before wiping)"
lsblk -o NAME,SIZE,FSTYPE,LABEL,MOUNTPOINT 2>/dev/null | sed 's/^/  /'
if lsblk -no MOUNTPOINT 2>/dev/null | grep -qE '^/(?!$)' 2>/dev/null; then
  note "(mounted paths above are the rescue system's own, not the target)"
fi

head2 "Network"
ip -brief link 2>/dev/null | sed 's/^/  /'
ip -brief addr 2>/dev/null | sed 's/^/  /'
note "default route: $(ip route show default 2>/dev/null | head -1 || echo NONE)"
if ! ip route show default 2>/dev/null | grep -q .; then
  bad "no default route — pacstrap cannot fetch packages"
fi
# OVH often hands out a /32 with an off-subnet gateway; DHCP config would be wrong.
if ip -4 addr show 2>/dev/null | grep -qE 'inet .*/32'; then
  bad "a /32 address is configured: the shipped 20-wired.network uses DHCP=yes and will NOT reproduce this. Capture the static address/gateway before installing."
else
  note "addressing looks DHCP-compatible (no /32 found)"
fi
note "dns: $(awk '/^nameserver/{printf "%s ",$2}' /etc/resolv.conf 2>/dev/null || echo none)"
if getent hosts geo.mirror.pkgbuild.com >/dev/null 2>&1; then
  note "DNS resolves Arch mirrors: ok"
else
  bad "cannot resolve Arch mirror hostnames — pacstrap will fail"
fi

head2 "Rescue tooling"
for b in zstd tar sha256sum mount chroot; do
  if command -v "$b" >/dev/null 2>&1; then note "ok      $b"; else bad "missing $b (needed to unpack/run the bootstrap)"; fi
done
# zstd must understand --long=31; the payload is compressed with --long.
if command -v zstd >/dev/null 2>&1; then
  if echo test | zstd -q --long=31 -c >/dev/null 2>&1; then note "ok      zstd --long=31"; else bad "zstd here does not accept --long=31"; fi
fi

head2 "Free space for the bootstrap chroot"
avail=$(df -Pk /root 2>/dev/null | awk 'NR==2{print $4}')
if [[ -n "${avail:-}" ]]; then
  note "/root free: $((avail/1024)) MiB"
  (( avail > 2500000 )) || bad "need ~2.5 GiB free under /root to unpack the bootstrap and cache packages"
fi

printf '\n=== VERDICT ===\n'
if (( warn )); then
  printf '  ATTENTION NEEDED — resolve the !! items above before installing.\n'
  exit 1
fi
printf '  No blockers found. Record the disk names above; pass them to rescue-install.sh.\n'
exit 0
