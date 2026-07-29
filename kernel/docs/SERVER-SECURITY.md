# Server unlock and host hardening

Canonical **keep list** and full OS description: **[SERVER-OS.md](SERVER-OS.md)**.

## Unlock order (keep)

Same FS as desktop: full-disk **LUKS2 + btrfs**.

1. **TPM2** — auto when enrolled (`tpm2-device=auto`).  
2. **SSH initrd** — only if TPM fails/absent **and** `--ssh-pubkey` at install.  
   dropbear key-only → `appsynergy-initrd-unlock` → LUKS passphrase.  
3. **Console / IPMI** — passphrase always; keyslot never wiped by TPM.

```bash
sudo appsynergy-install --variant server --disk /dev/sda --yes \
  --password-file /tmp/appsynergy-key \
  --ssh-pubkey /root/id_ed25519.pub
```

TPM miss at boot: `ssh root@<dhcp-ip>` (verify dropbear fingerprint on serial/IPMI first).

## Keep (hardening files)

| File on target | Purpose |
|----------------|---------|
| `/etc/nftables.conf` | fail-closed ingress |
| AppArmor (kernel + package) | MAC; `aa-status` after boot; stock profiles |
| `/etc/ssh/sshd_config.d/10-appsynergy.conf` | key-only when pubkey armed |
| `/etc/sysctl.d/99-appsynergy-server.conf` | FQ/BBR/forward/harden |
| `/etc/systemd/journald.conf.d/10-appsynergy.conf` | journal caps |
| `/etc/systemd/system.conf.d/10-watchdog.conf` | RuntimeWatchdogSec |
| `/etc/modules-load.d/appsynergy-server.conf` | nf_conntrack, nf_tables |
| `/etc/dropbear/root_key` | arms initrd SSH unlock |
| `/etc/appsynergy/UNLOCK.txt` | operator unlock notes |

## Not keep

agent, pets, console SPA, RAUC, verity, UKI, AppArmor agent, fabric CLI — see [SERVER-OS.md](SERVER-OS.md) § Explicitly NOT keep.
