# CLAUDE.md — kernel

`appsynergy-linux` is CachyOS's `linux-cachyos-server` built with ThinLTO under our name. **AppSynergy contributes no kernel configuration.** One package, both variants, every metal.

## Commands

```bash
packages/scripts/build-appsynergy-linux.sh    # the only kernel build; stages into packages/repo/x86_64
```

## File map

| Path | Role |
|------|------|
| `upstream/PIN` | the entire contract — commit, flavor, options, expected uname, required modules |
| `upstream/cachyos-source-keys.asc` | the two keys upstream signs release tarballs with |
| `upstream/config-<uname>` | the config that actually shipped, written by the build as evidence |
| `bench/` | committed benchmark runs |

## Invariants and gotchas

- **No config fragments.** `kernel/configs/` is gone and `scripts/check.sh` stage `kernel-nofork` fails if a `*.fragment` reappears. A fragment is how the two metals silently drifted apart: skylake shipped without `CONFIG_NETFILTER_XT_MATCH_PHYSDEV` for months while tigerlake had it, and no one could see it. Upstream's config carries `BRIDGE_NETFILTER`, `NETFILTER_XT_MATCH_PHYSDEV` and `NF_CONNTRACK_BRIDGE` already; the build asserts the modules ship rather than trusting the config text.
- **`_processor_opt` empty means native autodetection**, and upstream's default *is* empty. Built on this 12900K workstation that yields an Alder Lake kernel which does not boot the Skylake Xeon — it has happened. The pin sets `GENERIC_V3` explicitly and the build refuses to stage a package whose config has `CONFIG_X86_NATIVE_CPU=y` or lacks `CONFIG_X86_64_VERSION=3`.
- **One `GENERIC_V3` kernel makes an unbootable pairing representable.** Per-`-march=` packages could not be installed on the wrong CPU; one package can. `detect::supports_x86_64_v3` gates on `/proc/cpuinfo` **features**, not model names — a VM reports the host's model while masking features — and the installer bails before touching a disk.
- **AppArmor is built in but not enabled.** Upstream sets `CONFIG_LSM="landlock,lockdown,yama,integrity,bpf"` with `CONFIG_DEFAULT_SECURITY_DAC=y`. Profiles load, the unit starts, `aa-status` looks healthy, nothing is enforced. `disk::APPARMOR_LSM_CMDLINE` puts `apparmor` on the cmdline; the test also asserts none of upstream's five entries were dropped to make room.
- **Upstream leaves one source unchecksummed.** The ThinLTO branch appends `misc/dkms-clang.patch` from a raw `master` URL and never extends `b2sums`, so makepkg aborts on the length mismatch; their CI regenerates sums, which verifies nothing. `SRCSUM` in the pin holds our hash and the build appends it positionally.
- **Never build in `$SRC_CLONE`.** It is a working copy with local modifications, and a pin asserting only the commit says nothing about what sits on top. The build extracts `git archive <commit>:<flavor>` into a scratch dir.
- **The rename is not Kconfig.** `prepare()` writes `localversion.20-pkgname` from `pkgbase`. Two guarded substitutions in the build script, each required to match exactly once so upstream drift fails the build instead of renaming the kernel silently.
- Renaming changes `/boot` filenames, so an existing host swapping to it needs new bootloader entries — a reboot-window action. See `docs/AUDIT-REMEDIATION.md`.
- The lab NUC has **no TPM** — every boot needs a hand LUKS unlock over initrd SSH, so a bad initramfs costs physical access, not a reboot. Verify `lsinitcpio` before rebooting, never after.
