# CLAUDE.md — kernel

`appsynergy-linux` is CachyOS's `linux-cachyos-server` built with ThinLTO under our name. **AppSynergy contributes no kernel configuration.** One package, both variants, every metal.

## Commands

```bash
packages/scripts/build-appsynergy-linux.sh    # the only kernel build; stages into packages/repo/x86_64
```

## File map

| Path | Role |
|------|------|
| `upstream/PIN` | the entire contract — commit, flavor, options, expected uname, required modules and config (`REQUIRE_MODULE` / `REQUIRE_CONFIG`) |
| `upstream/cachyos-source-keys.asc` | the two keys upstream signs release tarballs with |
| `upstream/config-<uname>` | the config that actually shipped, written by the build as evidence |

## Invariants and gotchas

- **No config fragments.** `kernel/configs/` is gone and `scripts/check.sh` stage `kernel-nofork` fails if a `*.fragment` reappears. A fragment is how the two metals silently drifted apart: skylake shipped without `CONFIG_NETFILTER_XT_MATCH_PHYSDEV` for months while tigerlake had it, and no one could see it. Upstream's config carries `BRIDGE_NETFILTER`, `NETFILTER_XT_MATCH_PHYSDEV` and `NF_CONNTRACK_BRIDGE` already; the build asserts the modules ship rather than trusting the config text.
- **`_processor_opt` empty means native autodetection**, and upstream's default *is* empty. Built on this 12900K workstation that yields an Alder Lake kernel which does not boot the Skylake Xeon — it has happened. The pin sets `GENERIC_V3` explicitly and the build refuses to stage a package whose config has `CONFIG_X86_NATIVE_CPU=y` or lacks `CONFIG_X86_64_VERSION=3`.
- **One `GENERIC_V3` kernel makes an unbootable pairing representable.** Per-`-march=` packages could not be installed on the wrong CPU; one package can. `detect::supports_x86_64_v3` gates on `/proc/cpuinfo` **features**, not model names — a VM reports the host's model while masking features — and the installer bails before touching a disk.
- **AppArmor is built in but not enabled.** Upstream sets `CONFIG_LSM="landlock,lockdown,yama,integrity,bpf"` with `CONFIG_DEFAULT_SECURITY_DAC=y`. Profiles load, the unit starts, `aa-status` looks healthy, nothing is enforced. `disk::APPARMOR_LSM_CMDLINE` puts `apparmor` on the cmdline; the test also asserts none of upstream's five entries were dropped to make room.
- **`preempt=` accepts only `full` and `lazy`.** x86 has lazy preemption, and on such architectures the kernel removed `none` and `voluntary`; an unsupported value logs "Dynamic Preempt: unsupported mode" and is ignored. The server cmdline carried `preempt=voluntary` as that silent no-op. `disk::DESKTOP_CMDLINE` / `SERVER_CMDLINE` hold the per-variant choices and a test rejects any other mode.
- **Upstream leaves one source unchecksummed.** The ThinLTO branch appends `misc/dkms-clang.patch` from a raw `master` URL and never extends `b2sums`, so makepkg aborts on the length mismatch; their CI regenerates sums, which verifies nothing. `SRCSUM` in the pin holds our hash and the build appends it positionally.
- **Never build in `$SRC_CLONE`.** It is a working copy with local modifications, and a pin asserting only the commit says nothing about what sits on top. The build extracts `git archive <commit>:<flavor>` into a scratch dir.
- **`/tmp` is a quota'd tmpfs and clang writes scratch files there.** The build tree lives under `~/.cache/appsynergy-kernel-build`, and the script also points `TMPDIR` into it — without that the 7.3.4 build died at the final vmlinux link with "Disk quota exceeded". A build that fails for a reason outside the tree is kept; `RESUME_WORK=<kept tree>` reruns `make` there incrementally and still applies every assertion.
- **The rename is not Kconfig.** `prepare()` writes `localversion.20-pkgname` from `pkgbase`. Guarded substitutions in the build script (name, localversion, and `pkgrel` from the pin), each required to match exactly once so upstream drift fails the build instead of renaming the kernel silently.
- **A missing toolchain drops config silently.** makepkg runs `--nodeps`, and Kconfig removes options whose toolchain test fails instead of erroring. 7.3.4-1 shipped without `CONFIG_RUST` — so without binder, which is Rust-only since 7.3 — because `bindgen` was not installed. The build now refuses without `bindgen`, `rust-src`, and a rustc built on clang's LLVM major (ThinLTO links both in one LLVM pass; pick one with `RUSTUP_TOOLCHAIN`). A stale `rust-src` from an older rustc fails in `prepare()` with hundreds of `core` errors. To check a bump without a full build, compare the built config against upstream's published `linux-cachyos-server-lto` headers package (`usr/lib/modules/*/build/.config`): anything but toolchain version strings is a finding.
- Renaming changes `/boot` filenames, so an existing host swapping to it needs new bootloader entries — a reboot-window action. Runbook: `README.md` in this directory, "Rolling onto a host".
- The lab NUC (NUC11TNKv7) unlocks by **TPM2 bound to PCR 7** only, Secure Boot off, so a kernel change does not break unlock. Its initrd SSH fallback is not usable as installed: the initramfs carries no `igc` (the hook adds no NIC driver and `MODULES=()`), so if the TPM refuses, unlocking costs physical access. Verify `lsinitcpio` before rebooting, never after.
