# Audit remediation — status of record

External 18-agent audit (2026-08) → three read-only verification sweeps confirmed findings at file:line against `634e926` → remediation in small PR units + live-host runbook. Constraint held throughout: no reboot, no service restart, nothing taken offline on skylake (OVH `144.217.66.212`) or tigerlake (NUC `192.168.101.101`).

## Resolved before this remediation (verified live 2026-08-06, evidence in /root backups on both hosts)

- Client signature enforcement: both hosts run `SigLevel = Required DatabaseRequired`, keyring 1-2 populated (fpr `3B90D92D1E28E9E060D5C53D15D4351CF0D36AD1`), signed db syncing. Flipped 05:02–08:09 UTC 2026-08-06.
- Repo signing: `build-repo.sh` signs packages + db by default since `1805903`; `appsynergy-keyring` published and indexed.
- Skylake NetworkPolicy: enforcing (7.1.5-3 kernel has PHYSDEV; 93 KUBE-NWPLCY chains).
- nftables: loaded on both (oneshot unit; `is-active`=inactive is the oneshot idiom, not a gap).
- Monitoring: `monitoring` ns live on skylake (audit claim stale).

## Remediation units

| Unit | PR | Fixes |
|---|---|---|
| U0 publish-guard | #4 | publish refuses missing pkg/db sigs (`ALLOW_UNSIGNED=1` escape); every publish tail-calls verify-repo.sh; db-sig reachability checks; SIGN=0 banner |
| U8 mirrorlist-upgrade | #5 | legacy inline TrustAll sections migrated to Required on package upgrade (guarded on keyring populate; validated, backed up, idempotent) |
| U5 rescue-payload | #6 | `--disks` for multi-disk; k3s staged + gated in payload (sha256-pinned, verified against upstream `sha256sum-amd64.txt`); version-anchored branding globs incl. the destructive prune; both RESCUE-INSTALL.md copies fixed |
| U1 installer-trust | #9 | installer installs keyring+mirrorlist, asserts fingerprint, best-effort `pacman -Sy` to prove the signed db (offline installs still succeed); never writes TrustAll; keyring staged on ISO; preflight bail if keyring pkg missing (before any disk touch) |
| U2 input-validation | #12 | username/hostname/timezone/locale/keymap allowlist-validated at config load; kills `format!`→`bash -c` injection |
| U3 disk-safety | #14 | explicit disk-source precedence (typed flag wins); `--yes` refuses auto-detected disks; real block-device + partition + live-medium rejection |
| U4 robustness | #17 | server-critical `systemctl enable` hard-fails; server os-release VARIANT corrected (real file); final ESP re-sync + dual-ESP unlock verification |
| U6 gate-lints | #13 | shellcheck discovers every script (incl. extensionless, by shebang); unanchored-glob lint; write-usb removability gate |
| U7 kernel-tigerlake | #8 | fragment gains `NETFILTER_XT_MATCH_PHYSDEV=m`; `br_netfilter` in modules-load; fragment assertions in gate. Kernel `7.1.5-3` built, signed, published — **never installed**, and now **superseded**: the fragments it fixed no longer exist |
| kernel inherit | — | AppSynergy stops maintaining kernel configs. One package, `appsynergy-linux` = upstream `linux-cachyos-server` + ThinLTO, renamed, config unmodified. Five build scripts and three fragments deleted; the gate now asserts the pin and fails if a fragment returns. Two consequences handled in the same change: `GENERIC_V3` means the installer must refuse pre-Haswell CPUs, and upstream's `CONFIG_LSM` omits AppArmor so the cmdline must add it |
| repo prune | #21 | `build-repo.sh` keeps only the newest version per pkgname before `repo-add` — staging held two kernel versions and the indexed one was decided by glob order |
| U9 docs+CA | #18 | docs teach Required signatures; pre-rename paths fixed (incl. a live `backup-to-usb.sh` bug); one-secret keyfile model documented honestly; `appsynergy-ca-certificates` 1-3 makes the Root the only anchor, Intermediate neutral-trust chain filler |
| CI | #10, #11, #15 | act_runner on skylake k3s (ns `ci`, host-exec mode, capacity 1, hard limits, restricted securityContext, enforced NetworkPolicy); `check.sh` runs on every push |
| gate determinism | #16 | `--mode='a-st'` in `make-srctars.sh` — see below |

## What CI caught on its first run

The payload tarballs claimed determinism via `--sort/--mtime/--owner` but never normalized **mode**. A checkout under a Kubernetes volume inherits `g+s` on every directory it creates (`fsGroup` marks the volume root setgid), so the runner archived `2755` directories where the workstation archived `0755`: identical files, identical GNU tar 1.35, different sha256. The gate passed on the workstation and failed only in CI, first on `tarball-sums`, then `makepkg-all`. Fixed by stripping setuid/setgid/sticky from every member; the recorded sums are unchanged, so no PKGBUILD or pkgrel churn. Verified inside the runner pod, whose directories are still `drwxr-sr-x`.

This is the class of defect a workstation-only gate cannot see, and it appeared within minutes of the runner going live.

## Live-host changes (2026-08-06; nothing restarted, nothing rebooted)

- **skylake `br_netfilter` persisted** — appended to `/etc/modules-load.d/appsynergy-server.conf` (not package-owned; backup `.bak-20260806-231143`). Inert: the module was already loaded by the k3s unit's `ExecStartPre`, so this only fixes boot ordering. Tigerlake already persists it via its own `netguard.conf`.
- **skylake ESP2 — no action needed.** Read-only inspection showed `/dev/nvme1n1p1` already matching `/boot`; the sole difference is `loader/random-seed`, which is per-ESP entropy and must NOT be copied. The ordering defect that caused stale mirrors is real in code and fixed by U4 for future installs.
- **os-release VARIANT corrected on both hosts** — `/etc/os-release` replaced (symlink → real file) with `VARIANT="Server"` / `VARIANT_ID=server`, matching what U4 now produces for new installs. Backups in `/root`.
- **CI runner deployed to skylake** — namespace `ci`, registered as `k3s-arch-host`, green on `main`. Existing workloads' restart counts are byte-identical to the pre-deploy baseline; node sat at 18% CPU after a full CI queue drain.
- Artifact worth knowing: skylake predates the current installer and carries `/etc/appsynergy/VARIANT.txt` (`VARIANT=Server`) where tigerlake and current code use `/etc/appsynergy/VARIANT` (`server`). Both agree with os-release; harmless, not worth a migration.

## Deferred — operator decision required

Tigerlake NetworkPolicy is fail-open (8 `KUBE-NWPLCY-` chains vs skylake's 93) until the NUC boots a kernel carrying `CONFIG_NETFILTER_XT_MATCH_PHYSDEV`.

**Superseded 2026-08-07.** The fix is no longer `linux-appsynergy-server-tigerlake 7.1.5-3`. AppSynergy stopped maintaining kernel configs: there is now one kernel, `appsynergy-linux` — CachyOS's `linux-cachyos-server` built with ThinLTO under our name, from upstream's unmodified config, which already carries physdev, br_netfilter and nf_conntrack_bridge. Both retired flavor packages are dropped from the repo by `build-repo.sh`. See `kernel/CLAUDE.md`.

**This is a kernel swap, not a kernel upgrade, and that changes the risk.** The package name changes, so `/boot` filenames change (`vmlinuz-appsynergy-linux`, `initramfs-appsynergy-linux.img`) and **new bootloader entries are required** — the old entries keep pointing at the old images. The old kernel therefore remains as the fallback entry for free, which is the one thing in our favour. The NUC has no TPM, so every boot needs a hand LUKS unlock over initrd SSH; an initramfs without the dropbear hooks means physical access, not a reboot.

Steps 4 and 5 are the dangerous ones. Both hosts, skylake included — skylake needs this swap too, since its `linux-appsynergy-server-skylake` is equally retired, though it is not urgent because it already enforces.

1. ~~Build + publish~~ — **done 2026-08-07: `appsynergy-linux 7.1.6-1`**, signed under `3B90D92D…F0D36AD1`, published, `verify-repo.sh` clean (no drift, every package fetchable, both db signatures good). Validated before publishing: `vmlinuz`'s compiled-in banner reads `7.1.6-1-appsynergy-linux`; `arch = x86_64`; `X86_64_VERSION=3` with no `X86_NATIVE_CPU`; physdev/br_netfilter/nf_conntrack_bridge/igb/igc all ship. The built config differs from upstream's own `linux-cachyos-server-lto` by **3 symbols out of 12,633**, all toolchain-derived (`RUSTC_LLVM_VERSION`, `RUST_IS_AVAILABLE`) and inert because Rust is off in both. The one module we lack against their *GCC* build, `da903x-regulator`, is one upstream themselves drop under LTO.
2. **Re-verify at window time.** `pacman -Si appsynergy-linux` resolves to the intended version under `Required DatabaseRequired`, and nothing has superseded it. Confirmed on both hosts 2026-08-07 — each resolves `7.1.6-1`, `x86_64`, `PackageRequired PackageTrustedOnly DatabaseRequired DatabaseTrustedOnly`; **neither has installed it and both still run their old kernel.** (`pacman -Sy` there logs two `404`s for `core.db.sig`/`extra.db.sig`: Arch publishes no database signatures and core/extra are `DatabaseOptional`. Pre-existing, not ours.)
3. **Confirm the CPU can boot it.** The kernel is `GENERIC_V3`; both metals were verified v3-capable 2026-08-06, but re-check on any new hardware: `/lib/ld-linux-x86-64.so.2 --help | grep x86-64-v3` must say `(supported, searched)`.
4. **Install** (rewrites `/boot`, runs `mkinitcpio`; no reboot yet):
   ```
   pacman -Sy appsynergy-linux appsynergy-linux-headers
   lsinitcpio /boot/initramfs-appsynergy-linux.img | grep -E 'usr/bin/dropbear|root/.ssh/authorized_keys|appsynergy-initrd-sshd.service'
   ```
   All three must be present. Absent → do not reboot. Do **not** remove the old kernel package yet: its `/boot` images are the fallback.
5. **Add the boot entry, and put AppArmor on the cmdline.** The new entry needs the same `rd.luks.name=`/`root=` options as the current one **plus** `lsm=landlock,lockdown,yama,integrity,apparmor,bpf` — upstream's `CONFIG_LSM` omits AppArmor, so without this `apparmor.service` starts, `aa-status` looks healthy, and nothing is enforced. Leave the old entry in place as the fallback and do not make the new one default until it has booted once.
6. **Reboot**, unlock over initrd SSH, then confirm:
   ```
   uname -r                                                    # expect *-appsynergy-linux
   zgrep CONFIG_NETFILTER_XT_MATCH_PHYSDEV /proc/config.gz     # expect =m
   lsmod | grep br_netfilter                                    # persisted via modules-load.d
   aa-status | head -3                                          # expect profiles loaded AND enforcing
   iptables-save -t filter | grep -c KUBE-NWPLCY-               # expect >> 8
   ```
   Then probe a pod path a NetworkPolicy denies, to prove enforcement rather than presence.
7. **Only after a clean boot**, remove the retired kernel package and its `/boot` images.

Skylake's `br_netfilter` is now persisted, so its next reboot loads it at boot instead of relying on the k3s unit's soft modprobe; that is the intended state, not drift.

## Deferred — accepted risk / future work

Cosign/Rekor anchoring of the keyring (Vault-blocked); SBOM; server auto-updates/snapper; installer static-IP; `LocalFileSigLevel` tightening; hermetic kernel builds beyond `kernel/upstream/PIN`.
