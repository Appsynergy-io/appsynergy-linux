AppSynergy Linux — installer USB
================================

Defaults (this workstation):
  Disk:     /dev/nvme0n1  (ENTIRE DISK WIPED)
  Layout:   2G EFI + LUKS2 + btrfs (@ @home @log @cache @snapshots)
  User:     imma  (bash login; fish installed)
  Locale:   en_US.UTF-8
  Timezone: America/Sao_Paulo
  Keymap:   us
  Desktop:  Plasma (Breeze) + Firefox + btrfs-assistant
  Dev:      docker, rustup, go, node, clang, code, …

Before install:
  1. Ethernet or: nmtui  (Wi-Fi)
  2. Confirm backup of old machine is OFF this disk
  3. Optional local kernel: packages under /opt/appsynergy/pkgs/

Install:
  sudo appsynergy-install
  # or: sudo appsynergy-install --kernel repo
  # or: sudo appsynergy-install --disk /dev/nvme0n1 --yes   # no prompts (careful)

After reboot:
  - Unlock LUKS with the passphrase you set
  - Login as imma
  - Restore ~/.ssh and ~/projects from backup
  - Optional TPM: see /etc/appsynergy/TPM.txt on the installed system

Branding: os-release + ASCII banner + stock Breeze Dark only.
Never: Firefox, Cachy theme packs, pride/hyfetch, flashy SDDM, archlinux.gay mirrors.
Browsers: Brave (local pkg on USB) + Thorium (local pkg if present, else paru after boot).

Bazel host packages are in packages-target.txt (bazelisk, jdk17, gcc, pnpm, musl, …).
See /etc/appsynergy/BAZEL-HOST.txt on the live USB and installed system.
NativeLink + ~/bin/bazel wrapper: restore from backup after first boot.
