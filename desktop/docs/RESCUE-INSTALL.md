# AppSynergy Server — rescue install (step-by-step)

No ISO required. No Java. No process kills. You run each step after checking disks.

**Target:** dual NVMe, LUKS each disk, btrfs RAID1, skylake kernel, your baked SSH key.  
**Metal:** server1 E3-1270 v6 — default boot entry skylake.

## 0. From your laptop

1. Build/stage payload: `./scripts/stage-rescue-payload.sh`
2. Result: `out/appsynergy-server-rescue-YYYYMMDD.tar.zst`
3. OVH: set boot to **rescue**, reboot (API or Manager)
4. SSH: `ssh root@<public-ip>` (OVH rescue password)

## 1. On rescue — identify disks

```bash
lsblk -o NAME,SIZE,TYPE,FSTYPE,MOUNTPOINT,MODEL
# expect something like nvme0n1 + nvme1n1 (both large). Adjust names if different.
```

**Stop if mounts look wrong.** Do not wipe the wrong disk.

## 2. Fetch payload

```bash
# from your laptop (other terminal):
scp out/appsynergy-server-rescue-*.tar.zst root@SERVER_IP:/root/

# on rescue:
cd /root
zstd -d appsynergy-server-rescue-*.tar.zst -c | tar -x
cd appsynergy-server-rescue
sha256sum -c SHA256SUMS
ls pkgs/
```

## 3. Set LUKS passphrase (you choose)

```bash
printf 'YOUR_PASSPHRASE' > /tmp/luks-key
chmod 600 /tmp/luks-key
```

## 4. Partition both disks (EFI 1G + rest Linux)

Example for `/dev/nvme0n1` and `/dev/nvme1n1` — **edit names**:

```bash
for d in /dev/nvme0n1 /dev/nvme1n1; do
  sgdisk -Z "$d"
  sgdisk -n1:0:+1G -t1:ef00 -c1:EFI "$d"
  sgdisk -n2:0:0 -t2:8309 -c2:LUKS "$d"
  partprobe "$d" || true
done
```

## 5. LUKS + open

```bash
cryptsetup luksFormat --type luks2 -q /dev/nvme0n1p2 /tmp/luks-key
cryptsetup luksFormat --type luks2 -q /dev/nvme1n1p2 /tmp/luks-key
cryptsetup open --key-file /tmp/luks-key /dev/nvme0n1p2 crypt0
cryptsetup open --key-file /tmp/luks-key /dev/nvme1n1p2 crypt1
```

## 6. btrfs RAID1 + subvolumes

```bash
mkfs.btrfs -f -L appsynergy-server -d raid1 -m raid1 /dev/mapper/crypt0 /dev/mapper/crypt1
mkfs.fat -F32 -n EFI0 /dev/nvme0n1p1
mkfs.fat -F32 -n EFI1 /dev/nvme1n1p1

mkdir -p /mnt
mount /dev/mapper/crypt0 /mnt
btrfs subvolume create /mnt/@
btrfs subvolume create /mnt/@home
btrfs subvolume create /mnt/@var
btrfs subvolume create /mnt/@log
btrfs subvolume create /mnt/@cache
btrfs subvolume create /mnt/@snapshots
btrfs subvolume create /mnt/@srv
umount /mnt

mount -o subvol=@ /dev/mapper/crypt0 /mnt
mkdir -p /mnt/{home,var,var/log,var/cache,snapshots,srv,boot,boot/efi}
mount -o subvol=@home /dev/mapper/crypt0 /mnt/home
mount -o subvol=@var /dev/mapper/crypt0 /mnt/var
mount -o subvol=@log /dev/mapper/crypt0 /mnt/var/log
mount -o subvol=@cache /dev/mapper/crypt0 /mnt/var/cache
mount -o subvol=@snapshots /dev/mapper/crypt0 /mnt/snapshots
mount -o subvol=@srv /dev/mapper/crypt0 /mnt/srv
mount /dev/nvme0n1p1 /mnt/boot/efi
```

## 7. Bootstrap Arch root

Rescue is usually Debian. Options:

**A.** If rescue has `pacstrap` / arch-chroot (uncommon): use pacstrap with `etc/packages-target-server.txt`  
**B.** From another Arch machine: pacstrap into a tarball, scp, extract to `/mnt`  
**C.** On rescue: install `arch-install-scripts` if available, or use `debootstrap` is wrong — prefer Arch bootstrap tarball

Minimal pattern once Arch tools exist:

```bash
# example only when pacstrap works and mirrors work:
pacstrap -K /mnt $(grep -v '^#' etc/packages-target-server.txt | grep -v '^$')
```

Then local packages:

```bash
cp -a pkgs/*.pkg.tar.zst /mnt/root/
arch-chroot /mnt pacman -U --noconfirm /root/linux-appsynergy-server-skylake-*.pkg.tar.zst \
  /root/linux-appsynergy-server-skylake-headers-*.pkg.tar.zst \
  /root/appsynergy-branding-*.pkg.tar.zst 2>/dev/null || true
# optional second kernel:
# arch-chroot /mnt pacman -U --noconfirm /root/linux-appsynergy-server-tigerlake-*.pkg.tar.zst ...
```

## 8. fstab / crypttab / key

```bash
genfstab -U /mnt >> /mnt/etc/fstab
# crypttab both volumes; add TPM later if desired
printf 'crypt0 UUID=%s none luks,discard\n' "$(blkid -s UUID -o value /dev/nvme0n1p2)" >> /mnt/etc/crypttab
printf 'crypt1 UUID=%s none luks,discard\n' "$(blkid -s UUID -o value /dev/nvme1n1p2)" >> /mnt/etc/crypttab
```

## 9. Overlay configs + SSH key

```bash
cp etc/sysctl-server.conf /mnt/etc/sysctl.d/99-appsynergy-server.conf
cp etc/modules-load-server.conf /mnt/etc/modules-load.d/appsynergy-server.conf
cp etc/server-nftables.conf /mnt/etc/nftables.conf
mkdir -p /mnt/root/.ssh /mnt/home/imma/.ssh
cp etc/ssh-unlock.pub /mnt/root/.ssh/authorized_keys
cp etc/ssh-unlock.pub /mnt/home/imma/.ssh/authorized_keys
chmod 700 /mnt/root/.ssh /mnt/home/imma/.ssh
chmod 600 /mnt/root/.ssh/authorized_keys /mnt/home/imma/.ssh/authorized_keys
# copy server/* hooks as needed for dropbear unlock
```

## 10. Bootloader + users + services

```bash
arch-chroot /mnt bootctl install
# write loader entries for vmlinuz-*-appsynergy-server-skylake
# enable: sshd systemd-networkd systemd-resolved nftables apparmor containerd fstrim.timer
# set hostname appsynergy-server, user imma, root password / same key
arch-chroot /mnt mkinitcpio -P
```

Mirror ESP to second disk EFI partition when ready.

## 11. Leave rescue

```bash
umount -R /mnt
cryptsetup close crypt0
cryptsetup close crypt1
# API/Manager: boot mode hard disk → reboot
```

## 12. First boot

- Unlock LUKS (console or SSH initrd if dropbear armed)
- `ssh -i <yourkey> root@IP` or `imma@IP`
- `uname -r` → expect `*-appsynergy-server-skylake`

## Notes

- Live ISO + `appsynergy-install` is the automated path when you can boot the ISO.
- This doc is the **rescue** path: you verify each step.
- Payload pubkey is public key only; private key stays on your laptop.
