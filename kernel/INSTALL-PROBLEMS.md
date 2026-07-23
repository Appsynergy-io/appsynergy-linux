# AppSynergy reimage — install problems (2026-07-23)

Logged during live-USB install + restore session. Fix these later on a rebuild
or next ISO spin of appsynergy-install / branding.

## Critical / install-blocking

### 1. `appsynergy-install` aborted mid-run on os-release copy
- **Where:** after locale/hostname, step `==> Branding os-release`
- **Error:** `cp: '/mnt/appsynergy/etc/os-release' and '/mnt/appsynergy/usr/lib/os-release' are the same file`
- **Cause:** Arch ships `/etc/os-release` as a symlink to `../usr/lib/os-release`. Installer does:
  ```bash
  cp -a "$MNT/etc/appsynergy/os-release" "$MNT/etc/os-release"
  cp -a "$MNT/etc/appsynergy/os-release" "$MNT/usr/lib/os-release"
  ```
  First write through the symlink updates `/usr/lib/os-release`; second `cp` fails under `set -e`.
- **Impact:** users, systemd-boot, final initramfs hooks, services never ran from the script.
- **Workaround used:** continued install manually from branding → users → bootctl → mkinitcpio → enable services.
- **Fix:** write only to `/usr/lib/os-release` (or `cp -a --remove-destination` / unlink first). Prefer:
  ```bash
  cp -a "$SRC" "$MNT/usr/lib/os-release"
  ln -sfn ../usr/lib/os-release "$MNT/etc/os-release"
  ```

### 2. Branding/mirrorlist from [appsynergy] repo failed; local pkgs hit file conflicts
- **Repo pull:** `Errors occurred, no packages were upgraded` (repo version 2-11 vs local 2-12; integrity/keyring or partial state).
- **Local pacman -U:**  
  `appsynergy-mirrorlist: /etc/pacman.conf.d/appsynergy.conf exists in filesystem`  
  `appsynergy-mirrorlist: /etc/pacman.d/appsynergy-mirrorlist exists in filesystem`
- **Cause:** installer copies those files onto the target *before* installing the package.
- **Workaround:** `pacman -U --noconfirm --overwrite "*" ...`
- **Fix:** either don't pre-copy files owned by the package, or always use `--overwrite` for local branding path.

### 3. Non-interactive passwords not supported by installer
- Installer always prompts for LUKS (`cryptsetup luksFormat` interactive) and `passwd` for root/imma.
- Needed batch install from `/root/passwd` for LUKS + user passwords.
- **Workaround:** patched live `/usr/local/bin/appsynergy-install` to honor `APPSYNERGY_KEYFILE` (default `/tmp/appsynergy-key`).
- **Fix:** ship keyfile / env support officially (`APPSYNERGY_KEYFILE` or `--password-file`).

## Warnings (non-fatal)

### 4. mkinitcpio sd-vconsole
- `WARNING: sd-vconsole: "/etc/vconsole.conf" not found, will use default values`  
  (seen during early kernel-package initramfs on first pacstrap/kernel install path).
- Later manual `mkinitcpio -P` after writing `KEYMAP=us` / vconsole ran clean (still possible firmware warnings).

### 5. Possibly missing firmware
- `WARNING: Possibly missing firmware for module: 'qat_6xxx'` during sd-encrypt hook rebuild.
- Unrelated to this workstation (Intel QAT); ignore unless QAT hardware present.

### 6. `/etc/motd` missing after failed branding install
- Logged as `WARN: /etc/motd missing after branding install` when branding package install failed first pass.
- Fixed when branding was force-installed and motd rewritten in finish script.

## Environment notes (not bugs)

- Live ISO runs **copytoram** — USB stick can be removed during install.
- Default user in `machine.env` is already `imma`.
- LUKS UUID after this install: `fa46991f-6620-41b8-a060-eb74fd6f9ae2` (see `/etc/appsynergy/TPM.txt` on target).
- Password source used: `/root/passwd` on live (same for LUKS + root + imma). Do **not** leave that file on media long-term.

## Restore session notes

- BACKUP USB: `/dev/sda1` label `BACKUP` (btrfs).
- Target root still mounted at `/mnt/appsynergy` during restore-from-live.
- Agent CLIs on live: `/usr/local/bin/grok` 0.2.111, `/usr/local/bin/claude` 2.1.199 (rescue only).
- Historical host paths (pre-wipe): `~/.local/bin/grok` → `~/.grok/bin/grok`, claude via bun.

## Suggested installer PR checklist

Implemented in **Rust** (`appsynergy-desktop/installer/`, binary `appsynergy-install` 0.2.0):

- [x] Fix os-release dual-copy / symlink → write `/usr/lib/os-release` + `ln -sfn`
- [x] Branding: no pre-seed of package-owned files; `pacman -U --overwrite '*'`
- [x] `--password-file` / `APPSYNERGY_KEYFILE` for LUKS + chpasswd
- [x] `/etc/vconsole.conf` written in locale step before mkinitcpio
- [x] Step-named failures (`ERROR: step \`name\` failed: …`)
- [x] `efibootmgr` AppSynergy NVRAM entry + drop stale Windows/Linux PARTUUIDs


## Restore + CLI notes (same session)

### Restore
- Full restore from BACKUP → `/home/imma` completed while still chrooted from live USB.
- Projects ~9.6G, Brave profile, agents auth, secrets, personal dirs restored.
- Also copied this file to `~/INSTALL-PROBLEMS.md` on the installed system.

### Grok CLI
- Installed latest via `curl -fsSL https://x.ai/cli/install.sh | bash` as imma.
- Version at install time: **0.2.111** → `~/.grok/bin/grok` (+ `~/.local/bin/grok` link).
- Reused existing OIDC from restored `~/.grok/auth.json`.

### Claude Code CLI
- First install failed: conflicting launcher at `~/.local/bin/claude` (not owned by native installer; root-owned link from earlier step).
- Fix: remove launcher, re-run install; got **2.1.218** at `~/.local/bin/claude` → `~/.local/share/claude/versions/2.1.218`.
- Restored `~/.claude/` credentials/settings kept.

### Still manual after first boot
- TPM enroll: `sudo appsynergy-tpm-enroll` after 1–2 good passphrase boots.
- Termius app package not installed (config restored only).
- NativeLink /opt not on stick.
- rustup default toolchain if needed: `rustup default stable`.
- Verify Wi‑Fi/BT, Brave logins, SSH to remotes.


---

## Boot readiness audit (2026-07-23, pre-first-reboot)

Audited from live USB with target still mounted at `/mnt/appsynergy`.
Goal: confirm you can boot → type LUKS password → login as imma with no blockers.

### Verdict (after fix)

**Should boot.** One **critical** firmware issue was found and **fixed in this session**.
Passwords for LUKS + root + imma verify OK against the install keyfile (same content as `/root/passwd` without trailing newline: 11 bytes).

| Check | Result |
|-------|--------|
| ESP has systemd-boot + BOOTX64.EFI | OK |
| loader entry `appsynergy.conf` | OK (default) |
| vmlinuz + initramfs + intel-ucode on ESP | OK |
| `rd.luks.name=` UUID matches LUKS partition | OK (`fa46991f-6620-41b8-a060-eb74fd6f9ae2`) |
| `/etc/crypttab` + copy inside initramfs | OK (`cryptroot` + `x-initrd.attach`) |
| initramfs has `systemd-cryptsetup`, plymouth, sd-encrypt path | OK |
| nvme / btrfs / usbhid / xhci available for unlock | OK (built-in or in image) |
| `hid-apple` explicit in initramfs (Keychron Mac mode) | OK |
| fstab tabs/subvols (`@` `@home` …) | OK (`findmnt --verify` clean) |
| LUKS passphrase test (`cryptsetup luksOpen --test-passphrase`) | **OK** |
| `imma` login hash (libcrypt yescrypt) | **OK** |
| `root` login hash | **OK** |
| SDDM enabled + Plasma wayland session | OK |
| wheel sudoers | OK |
| **UEFI NVRAM boot entries** | **WAS BROKEN → FIXED** |

---

### CRITICAL (fixed): stale UEFI NVRAM → would not find new ESP

After full-disk wipe, firmware still had boot entries for the **old** ESP:

- Old PARTUUID: `ee8c63c3-c133-4ff7-b1f9-dc63193393c6`
- New ESP PARTUUID: `526b14a4-77c9-451a-ab7d-a72749975109`

Stale entries present before fix:

- `Boot0000` Windows Boot Manager → dead
- `Boot0001` Linux Boot Manager → `\EFI\SYSTEMD\SYSTEMD-BOOTX64.EFI` on **old** PARTUUID
- `Boot0005` Fallback Linux Boot Manager → same dead disk path
- BootOrder preferred those dead entries first

`bootctl install` had written files to the new ESP (`/EFI/systemd/systemd-bootx64.efi`, `/EFI/BOOT/BOOTX64.EFI`) but **did not replace NVRAM** with a working entry. A cold boot could skip the new install or fall through to USB only.

**Fix applied:**

```text
efibootmgr -c -d /dev/nvme0n1 -p 1 -L "AppSynergy Linux" \
  -l '\EFI\systemd\systemd-bootx64.efi'
# removed Boot0000, Boot0001, Boot0005
# BootOrder now: 0002 (AppSynergy), 0006 (USB live if still plugged)
```

Verified: `Boot0002* AppSynergy Linux` → `HD(...,526b14a4-77c9-451a-ab7d-a72749975109,...)\\EFI\\systemd\\systemd-bootx64.efi`

**Installer fix for next ISO:** after `bootctl install`, assert `efibootmgr` has an entry whose PARTUUID equals the new ESP; delete stale Windows/Linux entries pointing at wiped PARTUUIDs; set BootOrder.

---

### Passwords (what to type)

| Prompt | Account / volume | Source used at install | Verified |
|--------|------------------|------------------------|----------|
| LUKS / Plymouth disk unlock | `cryptroot` | keyfile = `/root/passwd` **without** trailing `\n` | yes |
| SDDM / sudo | `imma` | same | yes |
| root | `root` | same | yes |

Notes:

- File on live was 12 bytes (`Kimberlly1!\n`); LUKS/user hashes were set from the **11-byte** stripped keyfile. Type the password as you know it (no extra space/newline).
- LUKS and login passwords are the **same** (by request).
- Do **not** leave `/root/passwd` on install media long-term.

---

### Boot path expected on first reboot

1. Unplug install USB (or leave it; BootOrder is AppSynergy first, USB second).
2. Firmware loads **AppSynergy Linux** NVRAM entry → systemd-boot.
3. Default entry `appsynergy.conf` (timeout 3s).
4. Plymouth splash → **disk unlock** → type volume password + Enter.
5. systemd mounts btrfs `subvol=@`, then fstab mounts `@home` etc. + `/boot`.
6. SDDM → login **imma** + same password → Plasma (wayland).

---

### Non-blocking / residual notes

1. **`bootctl status` noise from live session**  
   While booted from archiso USB, bootctl reports “wrong PART_ENTRY_TYPE for XBOOTLDR” / ESP UUID mismatch vs *current* (USB) loader. That is **live-environment confusion**, not a target-disk defect. Trust `efibootmgr` + files on `/dev/nvme0n1p1`.

2. **`quiet splash`**  
   Kernel cmdline hides text; unlock UI is Plymouth. If splash fails, you may get a less obvious prompt — wait a few seconds, type password, Enter. Recover: boot with `e` in systemd-boot and remove `quiet splash` once.

3. **Secure Boot**  
   Disabled on this board. Fine for current unsigned kernel/bootloader.

4. **TPM**  
   Not enrolled yet (by design). Always passphrase until `sudo appsynergy-tpm-enroll` after a good boot.

5. **No X11 session package**  
   Only `plasma.desktop` wayland session found. Normal for this Plasma stack; if SDDM misbehaves, check wayland/GPU (intel).

6. **Stale Windows entry removed**  
   Windows was wiped with the disk; NVRAM Windows entry was deleted so firmware does not hang on a missing Microsoft path.

---

### If it still does not boot

| Symptom | Action |
|---------|--------|
| Firmware says no bootable device | BIOS boot menu → select `AppSynergy Linux` or the NVMe disk; re-run `efibootmgr -c ...` from live USB |
| Drops to USB/live only | Confirm BootOrder; unplug USB |
| Stuck before password | Remove `quiet splash` from entry; check keyboard (try another port) |
| Wrong password at LUKS | Re-check passphrase; from live: `cryptsetup open /dev/nvme0n1p2 cryptroot` |
| Unlocks then emergency shell | Check `journalctl -b` / fstab; remount and inspect |
| SDDM no login | `systemctl status sddm`; `passwd imma` from chroot |

