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
  # mkarchiso copies airootfs with `cp -af --no-preserve=ownership,mode`, so an
  # executable that is not declared here ships 0644. The installer chmods these
  # three on the target, which is why it was invisible — but they are unusable
  # in the live environment, which is where a rescue operator meets them.
  ["/opt/appsynergy/k3s/k3s"]="0:0:755"
  ["/etc/appsynergy/server/initrd-unlock"]="0:0:755"
  ["/etc/appsynergy/server/initcpio-install-ssh-unlock"]="0:0:755"
  # Runtime secrets. 0644 in the image would publish them to every USB reader.
  ["/opt/appsynergy/k3s/k3s.service.env"]="0:0:600"
)
