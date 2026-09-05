# AppSynergy Linux (unified installer USB)

**archiso** image + Rust `appsynergy-install` with two target variants:

| Variant | Command | Target |
|---------|---------|--------|
| **desktop** (default) | `appsynergy-install` | Z690 Plasma workstation |
| **server** | `appsynergy-install --variant server` | OVH / tunnel host (no DE) |

- Live environment: console, NetworkManager/iwd, install tools
- Desktop target: Plasma (Breeze), LUKS+btrfs, dev stack, `appsynergy-linux`
- Server target: headless, nftables + WireGuard + networkd, **k3s**, the same `appsynergy-linux` kernel (GENERIC_V3)
- Shell: **bash** default (fish optional interactive)

### Server OS (keep list)

Full documentation (keep list, disk, unlock, packages, NOT-keep apps):

**`kernel/docs/SERVER-OS.md`**

| Keep (summary) | Skip |
|----------------|------|
| nft fail-closed, key-only SSH, TPM→SSH→console unlock | agent, pets, console SPA |
| sysctl FQ/BBR, journald caps, watchdog | RAUC, verity, UKI |
| AppArmor | Plasma, NM, browsers |
| WG / nft / namespaces / cgroup v2 / XDP-ready kernel | appsynergy-linux fabric apps |
| **k3s** (no docker/containerd/nerdctl) | edgectl / custom orchestrator CLIs |
| same LUKS+btrfs layout as desktop | |

## Build ISO

```bash
sudo pacman -S --needed archiso
cd desktop
sudo ./scripts/build-iso.sh
```

Output: `out/appsynergy-linux-YYYY.MM.DD-x86_64.iso`

Needs network during build. Needs free disk under `work/` (several GB).

## Write USB

```bash
lsblk
sudo ./scripts/write-usb.sh /dev/sdX
```

## Install machine

1. Boot USB (UEFI).
2. `nmtui` if Wi-Fi; Ethernet preferred.
3. `sudo appsynergy-install` (**Rust** installer; source in `installer/`)  
   - **`--variant desktop|server`** (default desktop; env `APPSYNERGY_VARIANT`)  
   - Desktop disk default: **`/dev/nvme0n1`**; server default: **`/dev/sda`**  
   - Kernel: `appsynergy-linux` + `appsynergy-linux-headers`, both variants (CPU without x86-64-v3 is refused before any disk is touched)  
   - Non-interactive: `--yes --password-file /path/to/key`  
   - **TPM enroll** when TPM present (`--tpm` / `--no-tpm` / `--tpm-pcrs`)  
   - **Server SSH**: `--ssh-pubkey /path/to/id_ed25519.pub` → root key-only + **initrd dropbear unlock** if TPM fails  
   - Same disk as desktop: **full-disk LUKS2 + btrfs**
4. Reboot, remove USB. Desktop: Plasma. Server: TPM unlock (or `ssh root@ip` in initrd for passphrase) then SSH as root.
5. Server **does not** ship appsynergy-linux apps (agent/pets/console/RAUC).

## Layout after install

```
nvme0n1p1  2G   EFI
nvme0n1p2  rest LUKS2 → btrfs @ @home @log @cache @snapshots
```

## Files

| Path | Role |
|------|------|
| `iso/` | archiso profile |
| `iso/packages.x86_64` | **live** packages |
| `iso/airootfs/etc/appsynergy/packages-target.txt` | **desktop** pacstrap list |
| `iso/airootfs/etc/appsynergy/packages-target-server.txt` | **server** pacstrap list |
| `iso/airootfs/etc/appsynergy/machine.env` | desktop disk/hostname defaults |
| `iso/airootfs/etc/appsynergy/machine-server.env` | server defaults (UTC, /dev/sda) |
| `iso/airootfs/etc/appsynergy/sysctl-server.conf` | tunnel/WG sysctl (server) |
| `installer/` | **Rust** `appsynergy-install` (clap); built by `build-iso.sh` |
| `iso/airootfs/usr/local/bin/appsynergy-install` | staged release binary |
| `iso/airootfs/opt/appsynergy/pkgs/` | local kernel/branding/browser `.pkg.tar.zst` |
| `scripts/stage-rescue-clis.sh` | Copies `grok` + `claude` into live `/usr/local/bin` at build time |

## Not included (on purpose)

NVIDIA stack, extra kernels, Cachy themes, CUDA, mingw, Firefox, yay/octopi.
Browsers: Brave (+ Thorium when packaged); custom browser later.

## Target extras (packages-target)

Networking: `wireguard-tools`, `iwd` + NM `wifi.backend=iwd`, `wireless-regdb`, `jq`, `bind`, `fwupd`.
BT / passkeys / wallet: `bluez` + `bluez-utils` + `bluez-obex` + `bluedevil`, `libfido2`,
`kwallet-pam` + `kwalletmanager` + `signon-kwallet-extension`, `plasma-browser-integration`,
`pipewire-audio` (BT codecs). Installer sets bluez `Experimental=true` + `AutoEnable=true`
for Chromium/Brave hybrid passkeys; user groups include `lp`/`rfkill`/`audio`/`input`.
