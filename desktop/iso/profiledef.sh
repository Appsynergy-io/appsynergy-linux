#!/usr/bin/env bash
# shellcheck disable=SC2034

iso_name="appsynergy-linux"
# Joliet volid max 16 chars
iso_label="ASYN_$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y%m)"
iso_publisher="AppSynergy <https://git.appsynergy.io/imabee>"
iso_application="AppSynergy Linux installer (Z690 workstation)"
iso_version="$(date --date="@${SOURCE_DATE_EPOCH:-$(date +%s)}" +%Y.%m.%d)"
install_dir="appsynergy"
buildmodes=('iso')
bootmodes=('bios.syslinux'
           'uefi.systemd-boot')
pacman_conf="pacman.conf"
airootfs_image_type="squashfs"
# zstd, not xz: the airootfs is ~3.4G dominated by already-compressed payloads
# (brave/kernel .pkg.tar.zst, grok/claude binaries). xz spent 47min to reach 22%
# for a few percent size; zstd-19 squashes the same tree in minutes.
airootfs_image_tool_options=('-comp' 'zstd' '-Xcompression-level' '19' '-b' '1M')
bootstrap_tarball_compression=('zstd' '-c' '-T0' '--auto-threads=logical' '--long' '-19')
file_permissions=(
  ["/etc/shadow"]="0:0:400"
  ["/root"]="0:0:750"
  ["/root/.automated_script.sh"]="0:0:755"
  ["/root/.gnupg"]="0:0:700"
  ["/usr/local/bin/choose-mirror"]="0:0:755"
  ["/usr/local/bin/Installation_guide"]="0:0:755"
  ["/usr/local/bin/livecd-sound"]="0:0:755"
  ["/usr/local/bin/appsynergy-install"]="0:0:755"
  ["/usr/local/bin/appsynergy-tpm-enroll"]="0:0:755"
  ["/usr/local/bin/appsynergy-banner"]="0:0:755"
  ["/usr/local/bin/appsynergy-sanitize-mirrors"]="0:0:755"
  ["/usr/local/bin/grok"]="0:0:755"
  ["/usr/local/bin/claude"]="0:0:755"
  ["/usr/local/bin/agent"]="0:0:755"
)
