# kernel

`appsynergy-linux` — CachyOS's `linux-cachyos-server` built with ThinLTO under the AppSynergy name. One package for every machine, desktop and server alike.

**AppSynergy ships no kernel configuration.** The config is upstream's, committed in their repo, used unmodified. The only AppSynergy input is the name, and the name is not a Kconfig symbol — `prepare()` derives it from `pkgbase`.

## Build

```bash
packages/scripts/build-appsynergy-linux.sh
```

Extracts the pinned upstream commit into a scratch tree, verifies the release tarball against upstream's own signature and b2sum, builds, then refuses to stage unless the artifact matches `upstream/PIN`: expected `uname -r`, `CONFIG_X86_64_VERSION=3` with no `X86_NATIVE_CPU`, and the netfilter/NIC modules present. Staging goes to `packages/repo/x86_64`; `build-repo.sh` signs and indexes it.

## What upstream decides, and what that costs

`linux-cachyos-server` + ThinLTO is upstream's published `linux-cachyos-server-lto` recipe. Against the desktop flavor it is GCC→clang identical except for tick rate and LTO; against the plain server package it differs only in toolchain.

| | value | consequence |
|---|---|---|
| ISA | `GENERIC_V3` | pre-Haswell CPUs cannot boot it — the installer refuses them by CPU *feature*, not model name |
| `CONFIG_LSM` | `landlock,lockdown,yama,integrity,bpf` | AppArmor is compiled in but inactive; the installer adds it on the cmdline |
| HZ / preempt | 300 / `PREEMPT_LAZY` | both `PREEMPT_DYNAMIC`, so `preempt=full` on the cmdline changes it without a rebuild |
| `CONFIG_RUST` | off under clang | no in-tree Rust drivers |
| sched-ext | `SCHED_CLASS_EXT=y` | scx schedulers work; install `scx-scheds` |

## Retired

Per-metal `-march=skylake` / `-march=tigerlake` packages, `kernel/configs/*.fragment`, and the five build scripts that drove them. The split is what let the two servers drift onto different netfilter capabilities without anyone noticing — see `CLAUDE.md` in this directory.

## Upgrading to a new upstream release

1. `git -C ~/src/linux-cachyos fetch` and find the commit that bumps the flavor's `pkgver`/`pkgrel`.
2. Update `UPSTREAM_COMMIT`, `PKGVER`, `PKGREL` and `KERNEL_UNAME` in `upstream/PIN` together — `check.sh` fails if they stop composing.
3. Re-hash any `SRCSUM` entry whose upstream source moved.
4. Build, then `build-repo.sh` and `publish-repo.sh`.

Rolling a new kernel onto a running host rewrites `/boot` and needs new bootloader entries; on the TPM-less NUC verify `lsinitcpio` for the SSH-unlock components **before** rebooting.
