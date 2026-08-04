AppSynergy Linux — installer USB
================================

Install (guided — recommended):
  sudo appsynergy-install
  → pick 1 Desktop or 2 Server, confirm disks, password, SSH key, type YES

Batch server example:
  sudo appsynergy-install --yes --variant server \
    --disks /dev/nvme0n1,/dev/nvme1n1 \
    --password-file /tmp/key --ssh-pubkey /path/to.pub

Desktop defaults:
  Disk:     /dev/nvme0n1  (ENTIRE DISK WIPED)
  Layout:   2G EFI + LUKS2 + btrfs (@ @home @log @cache @snapshots)
  User:     imma  (bash login; fish installed)
  Locale:   en_US.UTF-8
  Timezone: America/Sao_Paulo
  Desktop:  Plasma (Breeze) + Brave + btrfs-assistant
  Dev:      rustup, go, node, clang, code, … (no docker; k3s is server-only)

Server defaults (see kernel docs SERVER-OS.md keep list):
  Disk:     /dev/sda
  Layout:   same LUKS2 + btrfs subvols as desktop
  Network:  systemd-networkd + nftables (no Plasma/NM)
  Unlock:   TPM auto → SSH initrd (baked pubkey) → console
  Kernel:   linux-appsynergy-server (local pkgs) when present
  NOT:      agent, pets, console SPA, RAUC (appsynergy-linux apps)

Before install:
  1. Ethernet or: nmtui  (Wi-Fi) — desktop live env
  2. Confirm backup of old machine is OFF this disk
  3. SSH: operator pubkey is baked (ssh-unlock.pub) — no copy needed

Install:
  sudo appsynergy-install
  sudo appsynergy-install --variant server --disk /dev/sda --yes \
    --password-file /tmp/appsynergy-key --ssh-pubkey /root/id_ed25519.pub
  # --kernel repo  → stock Arch linux

After reboot:
  - Desktop: unlock LUKS (TPM or passphrase) → login imma
  - Server: TPM or ssh root@ip in initrd for passphrase → SSH root/imma with key
  - See /etc/appsynergy/TPM.txt and (server) UNLOCK.txt

Branding: os-release + ASCII banner. Never: Firefox, Cachy themes, archlinux.gay.
Rescue CLIs (live only): grok, claude — /etc/appsynergy/RESCUE-CLI.txt
