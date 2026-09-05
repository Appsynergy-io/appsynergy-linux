# appsynergy-install (Rust) — unified desktop | server

Destructive full-disk installer for the live USB. Built into the ISO by
`scripts/build-iso.sh`.

| Flag | Desktop (default) | Server |
|------|-------------------|--------|
| `--variant` | `desktop` (your choice) | `server` (your choice) |
| Packages | `packages-target.txt` (`--dev` adds `packages-target-desktop-dev.txt`) | `packages-target-server.txt` |
| Kernel (local) | `appsynergy-linux` | `appsynergy-linux` — same package. Built `GENERIC_V3`, so a CPU without AVX2/BMI2/FMA is **refused before any disk is touched** rather than given a kernel that fails at boot. |
| FDE | LUKS2 full-disk | same |
| TPM | auto-enroll when `/dev/tpm*` present (`--no-tpm` to skip) | same |
| Network | NetworkManager + iwd | systemd-networkd + resolved |
| DE | Plasma / SDDM | none |
| Firewall | nftables | nftables seed + sysctl |

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

# Desktop + optional toolchain overlay (same ISO):
sudo appsynergy-install --variant desktop --dev --yes --disk /dev/nvme0n1 \
  --password-file /tmp/key
```

A failed run on the same live boot resumes from `/run/appsynergy-install/journal`. `--fresh` starts over.

Docs: `kernel/docs/SERVER-OS.md`. Tests: `cargo test`.
