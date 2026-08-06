# Volume password + TPM unlock

## What the disk looks like after install

```text
/dev/nvme0n1
├─ p1  2G   FAT32   /boot     (ESP — unencrypted)
└─ p2  rest LUKS2   cryptroot
      └─ btrfs (label appsynergy)
           @           → /
           @home       → /home
           @log        → /var/log
           @cache      → /var/cache
           @snapshots  → /.snapshots
```

Same idea as today (LUKS → btrfs subvols), but **one LUKS on the whole disk** (Windows gone). Only the EFI system partition is outside LUKS.

---

## Passwords you set — one or three, depending on the mode

| Mode | You are in it when | Secrets |
|------|--------------------|---------|
| **Keyfile / batch** | `--password-file PATH`, `APPSYNERGY_KEYFILE`, an existing `/tmp/appsynergy-key`, or you answered the guided password prompt | **One.** The same string is the LUKS passphrase, the root password and your login password |
| **Interactive** | no keyfile, and you left the guided prompt empty | **Three**, independent: LUKS typed twice at `cryptsetup`, then root and user typed at separate `passwd` prompts in the chroot |

The guided installer states it outright: *"LUKS + root + login password (same for all). Leave empty to type passwords later during install."* Keyfile mode is the default path for unattended and server installs.

### What that one secret reaches in keyfile mode

1. `cryptsetup luksFormat --key-file=-` and `cryptsetup open` — the volume passphrase.
2. `chpasswd` for `root:` and `<user>:` — both login passwords.
3. `systemd-cryptenroll --unlock-key-file=` — it **authorizes** TPM enrollment. The TPM seals a fresh random key into its own keyslot, so your secret is not the sealed material; but whoever holds it can add a keyslot.

One secret to lose, one to protect. Choose it at **volume-passphrase** strength — it guards a LUKS header an attacker can carry away, so ordinary login-password strength is not enough. It lands in `/tmp/appsynergy-key` and `/tmp/appsynergy-tpm-unlock.key`, both mode 0600 on the live medium's tmpfs and gone at reboot.

Use interactive mode if the disk secret must differ from the login password.

### Choosing a good volume passphrase

- Long passphrase or diceware (4+ words); you type it at a text prompt with the Keychron.
- Write it down **offline** until TPM works and you have tested recovery.
- You will need it if firmware/Secure Boot/PCR policy changes or the TPM is cleared.

### At first reboot

1. Remove USB.
2. systemd-boot → **AppSynergy Linux**.
3. Prompt for disk unlock → type **volume passphrase** + Enter.
4. SDDM → **imma** + user password.

---

## TPM unlock (after install works)

Do **not** enroll TPM until you have unlocked with the passphrase at least once successfully.

### 1. Confirm TPM

```bash
ls -l /dev/tpm0 /dev/tpmrm0
systemd-cryptenroll --tpm2-device=list
```

### 2. Enroll (installed system)

```bash
sudo appsynergy-tpm-enroll
```

That will:

1. Ask for the **volume passphrase** once (authorization).
2. Add a **TPM2 token** (keeps passphrase slot).
3. Point `/etc/crypttab` at `tpm2-device=auto`.
4. Run `mkinitcpio -P`.

Then:

```bash
reboot
```

Expected: boot to SDDM **without** typing the volume passphrase.  
If the TPM path fails, you still get the passphrase prompt — type the volume password.

### Manual equivalent

```bash
# discover LUKS partition
lsblk -f

sudo systemd-cryptenroll --tpm2-device=auto --tpm2-pcrs=7 /dev/nvme0n1p2
# edit /etc/crypttab if the script did not (see /etc/appsynergy/TPM.txt)
sudo mkinitcpio -P
reboot
```

### PCR policy (this machine)

- Secure Boot is **off** today.
- Default enroll uses **PCR 7** (Secure Boot policy state).
- If enroll or unlock is flaky:

```bash
sudo APPSYNERGY_TPM_PCRS=0+7 appsynergy-tpm-enroll
# or looser (weaker binding):
sudo systemd-cryptenroll --wipe-slot=tpm2 /dev/nvme0n1p2
sudo systemd-cryptenroll --tpm2-device=auto /dev/nvme0n1p2
```

Re-enroll after: BIOS update, Secure Boot on/off, major boot-chain changes.

### Remove TPM only (keep passphrase)

```bash
sudo systemd-cryptenroll --wipe-slot=tpm2 /dev/nvme0n1p2
sudo mkinitcpio -P
```

---

## Initramfs note

Install uses **`sd-encrypt` + `rd.luks.name=`** so TPM tokens from `systemd-cryptenroll` work.  
Classic `cryptdevice=` + `encrypt` hook does **not** auto-use TPM tokens.

---

## Recovery

| Problem | Action |
|---------|--------|
| Forgot user password | Boot USB, unlock LUKS with volume pass, `arch-chroot`, `passwd imma` (in keyfile mode they are the same string, so this is also the volume passphrase) |
| Forgot volume passphrase | Data not recoverable (no escrow). Headers alone do not help. |
| TPM stops unlocking | Type volume passphrase; re-run `appsynergy-tpm-enroll` |
| BIOS cleared TPM | Same as above |
