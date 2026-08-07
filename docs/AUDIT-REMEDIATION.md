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
| U7 kernel-tigerlake | #8 | fragment gains `NETFILTER_XT_MATCH_PHYSDEV=m`; `br_netfilter` in modules-load; fragment assertions in gate. Kernel `7.1.5-3` built, signed, published — **not installed, NUC not rebooted**; see deferred |
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

Tigerlake NetworkPolicy is fail-open (8 `KUBE-NWPLCY-` chains vs skylake's 93) until the NUC boots a kernel carrying `CONFIG_NETFILTER_XT_MATCH_PHYSDEV`. `linux-appsynergy-server-tigerlake 7.1.5-3` is built and published for exactly that; steps 1–2 below are done.

**The dangerous step is 4, not 5.** The NUC has no TPM, so every boot needs a hand LUKS unlock over initrd SSH. If `mkinitcpio` produces an image without the dropbear/ssh-unlock hooks, the box does not come back and needs physical access — verify before rebooting, not after.

1. ~~Build~~ — done 2026-08-06: `linux-appsynergy-server-tigerlake 7.1.5-3`, built config carries `CONFIG_NETFILTER_XT_MATCH_PHYSDEV=m` and the package ships `xt_physdev.ko.zst`.
2. ~~Sign + publish~~ — done: signed under the pinned key, published, `verify-repo.sh` clean. Both hosts resolve `7.1.5-3` under `Required DatabaseRequired`; neither has installed it.
3. **Re-verify at window time.** A published package is not a fresh one: confirm `pacman -Si linux-appsynergy-server-tigerlake` still resolves to the intended version, and that nothing has superseded it since.
4. **Install on the NUC** (this rewrites `/boot` and runs `mkinitcpio`; no reboot yet):
   ```
   pacman -Sy linux-appsynergy-server-tigerlake linux-appsynergy-server-tigerlake-headers
   lsinitcpio /boot/initramfs-linux-appsynergy-server-tigerlake.img | grep -E 'usr/bin/dropbear|root/.ssh/authorized_keys|appsynergy-initrd-sshd.service'
   ```
   All three must be present. Absent → do not reboot; the installer's `verify_initrd_unlock` enforces this contract for new installs, and this is the manual equivalent.
   Keep the previous kernel and its initramfs in `/boot` as the fallback boot entry.
5. **Reboot**, unlock over initrd SSH, then confirm the fix landed:
   ```
   zgrep CONFIG_NETFILTER_XT_MATCH_PHYSDEV /proc/config.gz    # expect =m
   lsmod | grep br_netfilter                                   # persisted via modules-load.d
   iptables-save -t filter | grep -c KUBE-NWPLCY-              # expect >> 8
   ```
   Then probe a pod path a NetworkPolicy denies, to prove enforcement rather than presence.

Skylake needs no reboot — it already runs a PHYSDEV kernel and enforces. Note its `br_netfilter` is now persisted, so a future reboot loads it at boot instead of relying on the k3s unit's soft modprobe; that is the intended state, not drift.

## Deferred — accepted risk / future work

Cosign/Rekor anchoring of the keyring (Vault-blocked); SBOM; server auto-updates/snapper; installer static-IP; `LocalFileSigLevel` tightening; hermetic kernel builds beyond `kernel/upstream/PIN`.
