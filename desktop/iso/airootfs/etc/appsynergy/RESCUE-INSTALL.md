# AppSynergy Server — rescue install (OVH / any non-Arch rescue)

No ISO. The payload ships an Arch bootstrap (pacstrap + arch-chroot, keyring and
mirrors already baked in), so a Debian rescue needs no Arch tooling of its own.

**Target:** dual NVMe → LUKS per disk → btrfs RAID1, skylake kernel, initrd SSH unlock.
**Metal:** server1 E3-1270 v6.

## 0. Build the payload (on your workstation)

```bash
cd ~/projects/appsynergy-desktop
sudo ./scripts/build-bootstrap.sh          # Arch bootstrap tarball → out/
./scripts/stage-rescue-payload.sh          # FLAVOUR=tigerlake|all to change kernels
```

Produces `out/appsynergy-server-rescue-YYYYMMDD.tar.zst` (~272 MB, skylake only).
Both scripts fail loudly rather than emit an unusable payload.

## 1. Boot rescue and copy the payload

OVH Manager/API: boot mode **rescue** → reboot → `ssh root@<ip>` (rescue password).

```bash
scp out/appsynergy-server-rescue-*.tar.zst root@SERVER_IP:/root/
```

On the rescue host — `--long=31` is required, the payload is compressed with `--long`:

```bash
cd /root
zstd -dc --long=31 appsynergy-server-rescue-*.tar.zst | tar -x
cd appsynergy-server-rescue
```

## 2. Preflight — read this before anything destructive

```bash
bash rescue-preflight.sh
```

Reports firmware mode, CPU/RAM, TPM, **actual disk names and sizes**, existing
filesystems, NICs, addressing, DNS, and rescue tooling. Exit 1 means a human
decision is needed. Two things it catches that would otherwise strand you:

- **A `/32` address.** OVH sometimes assigns one with an off-subnet gateway. The
  shipped `server-network/20-wired.network` uses `DHCP=yes` and will not
  reproduce it — capture the static config first, or the host comes back with no
  network and the SSH unlock never answers.
- **Disk names.** Never assume `nvme0n1`/`nvme1n1`. Use what preflight prints.

## 3. Install

```bash
bash rescue-install.sh --disk /dev/nvme0n1,/dev/nvme1n1
```

Optional: `--flavour tigerlake`, `--yes`, `--password-file /path/to/key`.
Anything unrecognised is passed through to `appsynergy-install`.

The script verifies payload checksums, unpacks the bootstrap, checks the tooling
is present, wires `etc/` and `pkgs/` into the chroot, bind-mounts
`/proc /sys /dev /run`, and runs the installer. It always unmounts on exit —
including on failure — and warns rather than leaving a half-mounted tree.

The installer then confirms interactively: **you type the exact disk paths** to
proceed. It refuses outright if no SSH pubkey is available, before touching a
single disk, because a headless host with no unlock key is unrecoverable.

What it does: partition (1 G ESP + LUKS2 rest per disk) → btrfs RAID1 with
`@ @home @var @log @cache @snapshots @srv` → pacstrap `packages-target-server.txt`
→ local kernel/branding packages → fstab/crypttab → server overlay (sysctl, nft,
sshd hardening, journald caps, watchdog, networkd) → dropbear initrd unlock →
systemd-boot on both ESPs (mirrored) → TPM enrol when a TPM exists.

## 4. Reboot into the installed system

OVH: boot mode **hard disk** → reboot.

First boot unlocks in this order: TPM (if present) → **initrd SSH** → console.

```bash
ssh -i <your-key> root@SERVER_IP     # initrd dropbear: you get the unlock agent
# enter the LUKS passphrase, session closes, host continues booting
ssh -i <your-key> root@SERVER_IP     # normal login
uname -r                              # expect *-appsynergy-server-skylake
```

## 5. Verify

```bash
lsblk -o NAME,SIZE,FSTYPE,MOUNTPOINT   # btrfs RAID1 over two crypt devices
btrfs filesystem df /                  # RAID1 for data and metadata
systemctl is-enabled sshd nftables systemd-networkd apparmor docker
nft list ruleset | head                # fail-closed policy
cat /etc/appsynergy/UNLOCK.txt         # unlock order for this host
```

## Notes

- The live ISO + `appsynergy-install` is the path when you can boot the ISO.
  This is the path when you cannot — the normal case on rented metal.
- The payload's `ssh-unlock.pub` is a **public** key; the private key never leaves
  your workstation.
- The manual step-by-step remains in git history (this file before 2026-07-29)
  if you ever need to do this without the scripts.
