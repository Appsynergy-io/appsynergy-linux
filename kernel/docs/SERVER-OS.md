# AppSynergy Server (OS variant)

```bash
appsynergy-install --variant server   # APPSYNERGY_VARIANT=server
```

Headless OVH / tunnel host. Same **LUKS2 + btrfs** layout as desktop; same CachyOS
**kernel series**; package **`linux-appsynergy-server`**. Replaces appsynergy-linux
for this role — **keep list only**, no appliance applications.

| Doc | Role |
|-----|------|
| **This** | OS + keep list |
| [SERVER-KERNEL.md](SERVER-KERNEL.md) | kernel fragment / package |
| [SERVER-SECURITY.md](SERVER-SECURITY.md) | TPM / SSH unlock |
| `configs/server.fragment` | Kconfig |
| `appsynergy-desktop` | installer, `packages-target-server.txt` |

## Keep list

### Host posture

| Keep | On target |
|------|-----------|
| Fail-closed **nftables** | `/etc/nftables.conf` (SSH :22, lo, established, DHCP, ICMP/ND) |
| **AppArmor** (MAC) | kernel LSM + `apparmor` package; `systemctl enable apparmor`; stock profiles |
| **Key-only SSH** | `sshd_config.d/10-appsynergy.conf` if `--ssh-pubkey` |
| **TPM → SSH unlock → console** | crypttab TPM; initrd dropbear; passphrase always |
| **Sysctl** FQ/BBR/forward/harden | `sysctl.d/99-appsynergy-server.conf` |
| **journald** cap + rate limit | `journald.conf.d/10-appsynergy.conf` |
| **Watchdog** | `system.conf.d/10-watchdog.conf` |
| **nf_conntrack** before sysctl | modules-load + sysctl unit drop-in |

### Kernel dataplane (`server.fragment`)

WireGuard (y) · nftables+conntrack+NAT+flowtable · policy routing · TUN/veth/ipvlan/vxlan/geneve · eBPF/XDP/BTF/AF_XDP · namespaces · cgroup v2 + CGROUP_BPF · io_uring · igb+igc+virtio · NVMe · dm-crypt · btrfs · overlay · fuse · **AppArmor** LSM (`landlock,lockdown,yama,apparmor,bpf`)

### Userspace

systemd + **networkd/resolved** · openssh · dropbear **initrd only** (mask multi-user) · wireguard-tools · nftables · **apparmor** · **docker + docker-compose** · iproute2 · bpf · cryptsetup · tpm2-* · btrfs-progs · linux-appsynergy-server-skylake/-nuc · branding · **bash** login · rustup+clang toolchain · enable: sshd, nftables, networkd, resolved, apparmor, docker, fstrim

### NOT keep

agent · pets/nspawn · console SPA · RAUC · verity · UKI · agent AppArmor profile · fabric CLI · **edgectl** · Plasma/SDDM/browsers/NM/bluez/mesa · **docker/moby** · MODULE_SIG_FORCE

## Identity and disk

| | Server default |
|--|----------------|
| Name | AppSynergy Server (`VARIANT=Server`) |
| Host / user / tz | `appsynergy-server` / `imma`+root / UTC |
| Disks | **`/dev/nvme0n1,/dev/nvme1n1`** (2× ~420 GiB DC NVMe) via `APPSYNERGY_DISKS` |
| EFI | 1 GiB EF00 on **each** disk (mirrored after bootctl) |
| Data | LUKS2 per disk (`crypt0`/`crypt1`) → **btrfs RAID1** (`-d raid1 -m raid1`) |
| Subvols | `@` `@home` `@var` `@log` `@cache` `@snapshots` `@srv` |
| Usable | ~400 GiB after mirror |

Single-disk fallback: `--disk /dev/sda` (no RAID1).  
Unlock: TPM both LUKS → SSH initrd → console. [SERVER-SECURITY.md](SERVER-SECURITY.md).

## Install

```bash
sudo appsynergy-install --variant server --yes \
  --disk /dev/sda --password-file /tmp/appsynergy-key \
  --ssh-pubkey /root/id_ed25519.pub
```

Live seeds: `packages-target-server.txt`, `machine-server.env`, `sysctl-server.conf`,
`server/*`, `server-nftables.conf`. Target notes: `/etc/appsynergy/{UNLOCK,TPM,VARIANT}.txt`.

## AppArmor

- **Kernel:** `CONFIG_SECURITY_APPARMOR=y`, LSM order includes `apparmor` (needs
  `linux-appsynergy-server` rebuild — desktop fallback kernel may lack it).
- **Userspace:** `apparmor` package; unit enabled at install.
- **Profiles:** distro stock only (no custom orchestrator profiles).
- **Check:** `aa-status` · `dmesg | grep -i apparmor` · `/sys/kernel/security/lsm`
