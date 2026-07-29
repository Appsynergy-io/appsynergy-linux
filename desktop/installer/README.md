# appsynergy-install (Rust) — unified desktop | server

Destructive full-disk installer for the live USB. Built into the ISO by
`scripts/build-iso.sh`.

| Flag | Desktop (default) | Server |
|------|-------------------|--------|
| `--variant` | `desktop` | `server` |
| Packages | `packages-target.txt` | `packages-target-server.txt` |
| Kernel (local) | `linux-appsynergy` | `linux-appsynergy-server` |
| Network | NetworkManager + iwd | systemd-networkd + resolved |
| DE | Plasma / SDDM | none |
| Firewall | (ufw in pkg list) | nftables seed + sysctl |

## Normal use (guided)

```bash
sudo appsynergy-install
```

Then answer:

1. **1 = Desktop** or **2 = Server** (default Server)  
2. Disks — if two NVMe are found, **Y** for RAID1  
3. If LUKS / AppSynergy / partitions exist: type **`NUKE`** to destroy (else abort)  
4. Password (or empty to type later)  
5. SSH public key path (Server)  
6. Type **YES** to wipe  

Wizard unit tests: `cargo test guide::`

## Batch (no questions)

```bash
sudo appsynergy-install --yes --variant server \
  --disks /dev/nvme0n1,/dev/nvme1n1 \
  --password-file /tmp/key --ssh-pubkey /root/id_ed25519.pub
```

Docs: `kernel/docs/SERVER-OS.md`. Tests: `cargo test`.
