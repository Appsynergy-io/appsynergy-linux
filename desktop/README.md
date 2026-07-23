# AppSynergy Linux (desktop installer USB)

Basic **archiso** image for reimaging the Z690 workstation.

- Live environment: console, NetworkManager/iwd, install tools
- Target install: Plasma (Breeze), LUKS+btrfs, dev stack, **one** kernel path
- Locale: `en_US.UTF-8`, timezone `America/Sao_Paulo`, keymap `us`
- Branding: `os-release` + ASCII banner (not Cachy/Arch chrome)
- Shell: **bash** default (fish installed on target for optional interactive use)

## Build ISO

```bash
sudo pacman -S --needed archiso
cd /home/imma/projects/appsynergy-desktop
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
   - Default disk: **`/dev/nvme0n1` (FULL WIPE)**  
   - Default kernel: **local** packages from `/opt/appsynergy/pkgs`  
   - Or: `sudo appsynergy-install --kernel repo` for stock `linux`  
   - Non-interactive passwords:  
     `sudo appsynergy-install --yes --password-file /path/to/key`  
     (same passphrase for LUKS + root + user; trailing newline stripped)
4. Reboot, remove USB, unlock LUKS, login as `imma`.

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
| `iso/airootfs/etc/appsynergy/packages-target.txt` | **target** pacstrap list |
| `iso/airootfs/etc/appsynergy/machine.env` | disk/hostname/user defaults |
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
| Containers | `docker` `docker-compose` `fuse-overlayfs` |

**Not packaged on ISO:** NativeLink binary tree, farm `~/bin/bazel` wrapper — **restore from backup** after install.
