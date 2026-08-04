#!/usr/bin/env fish
# Verify the stock kernel boot chain is byte-identical to the recorded snapshot
# and still the systemd-boot default. Run (with sudo) after every kernel install.
# Re-snapshot ONLY after a legitimate linux-cachyos package update:
#   scripts/verify-stock-boot.fish --snapshot

set -l root (realpath (dirname (status filename))/..)
set -l sha $root/scripts/stock-boot.sha256
set -l files /boot/vmlinuz-linux-cachyos /boot/initramfs-linux-cachyos.img /boot/loader/entries/linux-cachyos.conf

if test "$argv[1]" = --snapshot
    sudo sha256sum $files > $sha
    echo "Snapshot written: $sha"
    exit 0
end

sudo sha256sum -c $sha; or begin
    echo "STOCK BOOT FILES CHANGED — do not reboot until resolved" >&2
    exit 1
end
set -l def (sudo grep '^default' /boot/loader/loader.conf)
if test "$def" != "default linux-cachyos"
    echo "loader.conf default changed: '$def' (expected 'default linux-cachyos')" >&2
    exit 1
end
echo "OK: stock kernel files identical, stock remains boot default"
