# AppSynergy Linux (unified installer USB)

**archiso** image + Rust `appsynergy-install` with two target variants:

| Variant | Command | Target |
|---------|---------|--------|
| **desktop** (default) | `appsynergy-install` | Z690 Plasma workstation |
| **server** | `appsynergy-install --variant server` | OVH / tunnel host (no DE) |

- Live environment: console, NetworkManager/iwd, install tools
- Desktop target: Plasma (Breeze), LUKS+btrfs, dev stack, `appsynergy-linux`
- Server target: headless, nftables + WireGuard + networkd, **k3s**, CPU-auto server kernel
- Shell: **bash** default (fish optional interactive)

### Server OS (keep list)

Full documentation (keep list, disk, unlock, packages, NOT-keep apps):

**`/home/imma/projects/appsynergy-linux/kernel/docs/SERVER-OS.md`**

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
cd /home/imma/projects/appsynergy-linux/desktop
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
   - Kernel local pkgs: `appsynergy-linux` + `appsynergy-linux-headers`, both variants  
   - Or: `--kernel repo` for stock Arch `linux`  
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

### Installer fixes (vs bash 2026-07-23)

- `os-release`: write `/usr/lib/os-release` only; symlink `/etc/os-release` (no dual-`cp` abort)
- Branding: no pre-seed of package-owned files; `pacman -U --overwrite '*'`
- `--password-file` / `APPSYNERGY_KEYFILE` for LUKS + `chpasswd`
- `/etc/vconsole.conf` before mkinitcpio
- `efibootmgr` creates AppSynergy NVRAM entry; drops stale Windows/Linux PARTUUIDs
- **TPM2 LUKS enroll** after initramfs (`systemd-cryptenroll`); crypttab gets `tpm2-device=auto`; rebuild mkinitcpio
- Every failure includes the step name

## Not included (on purpose)

NVIDIA stack, extra kernels, Cachy themes, CUDA, mingw, Firefox, Plymouth, yay/octopi.
Browsers: Brave (+ Thorium when packaged); custom browser later.

## Target extras (packages-target)

Networking: `wireguard-tools`, `iwd` + NM `wifi.backend=iwd`, `wireless-regdb`, `jq`, `bind`, `fwupd`.
BT / passkeys / wallet: `bluez` + `bluez-utils` + `bluez-obex` + `bluedevil`, `libfido2`,
`kwallet-pam` + `kwalletmanager` + `signon-kwallet-extension`, `plasma-browser-integration`,
`pipewire-audio` (BT codecs). Installer sets bluez `Experimental=true` + `AutoEnable=true`
for Chromium/Brave hybrid passkeys; user groups include `lp`/`rfkill`/`audio`/`input`.

## After first boot

- Restore backups (`~/.ssh`, `~/projects`, `~/bin/bazel` farm wrapper, …)
- `rustup default stable` (and 1.90.0 if you match appsynergy-rs pin for cargo path)
- `bazelisk` is on PATH as needed; project `.bazelversion` is **8.3.1** (bazelisk fetches it)
- Symlink if you want the name `bazel`: `sudo ln -sf /usr/bin/bazelisk /usr/local/bin/bazel`  
  Prefer restoring **`~/bin/bazel`** (farm WoL wrapper) so `check.sh` finds it first
- NativeLink: restore `/opt/nativelink` + unit from backup, then `systemctl enable --now nativelink`
- `~/.bazelrc` remote_executor lines → restore from backup (points at 127.0.0.1:50051)
- TPM later: `/etc/appsynergy/TPM.txt` on installed system

## Bazel / build host packages (on target image)

| Need | Package(s) |
|------|------------|
| Bazel launcher | `bazelisk` (honors `.bazelversion` → 8.3.1 for appsynergy-rs) |
| JVM for Bazel | `jdk17-openjdk` |
| Host / farm CC | `gcc` `binutils` `glibc` `linux-api-headers` `pkgconf` `openssl` (+ `base-devel`) |
| Clang path / lints | `clang` `llvm` `lld` `compiler-rt` |
| CMake crates | `cmake` `ninja` `nasm` `perl` |
| Frontend gates | `nodejs` `npm` `pnpm` |
| Go migrator | `go` |
| Cargo fallback | `rustup` |
| musl release | `musl` `kernel-headers-musl` |
| Containers | **server: k3s only** · desktop: none (no docker) |

**Not packaged on ISO:** NativeLink binary tree, farm `~/bin/bazel` wrapper — **restore from backup** after install.
