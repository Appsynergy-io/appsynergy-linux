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

The installer checks UEFI boot, CPU compatibility, required payloads and SSH keys before wiping. Then answer:

1. **1 = Desktop** or **2 = Server** (default Server)
2. Disks — if two NVMe are found, **Y** for RAID1
3. On a new install, confirm existing data with **`NUKE`** (otherwise abort)
4. Password (or empty to type later)
5. SSH public key path (**required for Server**)
6. Confirm the exact target disk path(s), then type **YES** to wipe

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

If a package download or later phase fails, run the same command again on the same live boot. Resume verifies the installer, disk identities, configuration, filesystem UUIDs and mounts. It reopens encrypted volumes and restores missing target mounts. Supply the same disk password again; credentials are not retained for resume.

An interrupted partition/format/subvolume phase is refused rather than repeated automatically. To deliberately restart, unmount the target filesystems (including any temporary `/mnt` mount), close this install's LUKS mappings, then rerun with `--fresh`. Fresh installs refuse mounted disks and active mappings. Never use `--fresh` just to retry a download.

Only one installer may run at a time. `--yes` requires both explicit disks and `--password-file`; it will not stop for password prompts after wiping. Password files must contain one UTF-8 line. Guided credentials use randomly named private files removed on exit; `chpasswd` receives credentials over stdin. The installer no longer automatically reads `/tmp/appsynergy-key`.

## Installed defaults

- **Workstation:** Plasma, NetworkManager, default-drop nftables. SSH is key-only, root SSH is disabled, and sshd is enabled only when an operator key is supplied or found. Local login uses the chosen password.
- **Server:** headless single-node K3s with its embedded container runtime, networkd, key-only SSH and initrd SSH disk unlock. No host Docker/containerd service, development toolchain or tunnel appliance rules. Traefik and ServiceLB are disabled; publish workloads explicitly with K3s services.
- **One kernel, tuned per variant at install time.** `appsynergy-linux` is upstream's server build (lazy preemption, 300 Hz). Workstations boot it with `quiet splash preempt=full nowatchdog` (Plymouth, `appsynergy` theme), swap on zram (`zram-generator`, zstd, half of RAM up to 8 GiB) and `/etc/sysctl.d/99-appsynergy-desktop.conf`. Servers keep lazy preemption and the watchdog, boot with `transparent_hugepage=madvise`, load `sch_fq` + `tcp_bbr`, and get the kubelet's expected kernel defaults, raised inotify limits and kubelet reserved resources (`/etc/rancher/k3s/config.yaml`). Check with `cat /sys/kernel/debug/sched/preempt` and `zramctl`.
- K3s, and NetworkManager on workstations, require the host firewall to start successfully. On servers `sshd` and `systemd-networkd` only order after it, so a ruleset that fails to load stops workloads without locking you out. Pod egress, pod access to the API and K3s-published DNAT services are permitted; NetworkPolicy drops remain effective. Public access to the API, kubelet, etcd and VXLAN is closed by default.
- For additional K3s nodes, add their **private IPv4 addresses** to `cluster_nodes` in `/etc/nftables.conf` on each node, then reload nftables. Pod CIDR defaults to `10.42.0.0/16`; update both firewall and K3s config if changing it. Do not open VXLAN to the Internet.
- RAID1 targets install `appsynergy-sync-esp`, a pacman hook and a mkinitcpio post hook. Kernel/initramfs updates synchronize and verify the secondary EFI partition using checksums. A missing or mismatched ESP is reported as an error. After repairing it, run `sudo appsynergy-sync-esp`. This does not automate replacement of a failed RAID member or guarantee unattended degraded-root boot.

After the first server boot: `sudo kubectl get nodes` and `sudo kubectl get pods -A`. Remote API administration can use an SSH tunnel; there is no need to expose port 6443 publicly.

## Validation

```bash
scripts/check.sh
sudo desktop/scripts/smoke-install.sh   # disposable LUKS/btrfs loop device
sudo desktop/scripts/smoke-firewall.sh  # isolated namespaces, real packet tests
sudo desktop/scripts/smoke-esp.sh       # disposable FAT ESPs, update + refusal tests
```

Before rollout, boot a newly built ISO in a UEFI VM or spare machine and verify installation, reboot, console/SSH disk unlock and a kernel upgrade. Hardware TPM enrollment and full boot recovery need that end-to-end check.

Layout smoke (root, not CI): `sudo desktop/scripts/smoke-install.sh`

Docs: `kernel/docs/SERVER-OS.md`. Tests: `cargo test`.
