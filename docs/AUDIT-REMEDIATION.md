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
| U4 robustness | **in flight** (`fd277f7`, `dev-07f3e323`) | server-critical `systemctl enable` hard-fails; server os-release VARIANT corrected (real file); final ESP re-sync + dual-ESP unlock verification |
| U6 gate-lints | #13 | shellcheck discovers every script (incl. extensionless, by shebang); unanchored-glob lint; write-usb removability gate |
| U7 kernel-tigerlake | #8 | fragment gains `NETFILTER_XT_MATCH_PHYSDEV=m`; `br_netfilter` in modules-load; fragment assertions in gate. Kernel built+published; **NUC reboot deferred to maintenance window** |
| U9 docs+CA | **in flight** | docs teach Required signatures; pre-rename paths fixed; one-secret keyfile model documented honestly; `appsynergy-ca-certificates` 1-3 makes the Root the only anchor, Intermediate neutral-trust chain filler |
| CI | #10, #11 | act_runner on skylake k3s (ns `ci`, host-exec mode, capacity 1, hard resource limits, restricted securityContext, enforced NetworkPolicy); `check.sh` runs on every push |

## Live-host runbook (executed items)

- R3 trust flip — done before this session (see above).
- R4' skylake: `br_netfilter` persisted via `/etc/modules-load.d/netguard.conf` (file write only; module already loaded by k3s unit). Tigerlake already had it.
- R5 skylake: ESP2 (`nvme1n1p1`) diffed against `/boot` and re-synced (precondition: `/boot` matched running kernel).
- R-VARIANT: live `/etc/os-release` corrected to `VARIANT="Server"` on both (matches U4 behavior for new installs).

## Deferred — maintenance window, owner: operator

- Tigerlake boot into PHYSDEV kernel + NetworkPolicy validation (`KUBE-NWPLCY-` chains > 8, pod probes per kernel/CLAUDE.md).
- Any tigerlake initrd change.

## Deferred — accepted risk / future work

Cosign/Rekor anchoring of the keyring (Vault-blocked); SBOM; server auto-updates/snapper; installer static-IP; `LocalFileSigLevel` tightening; hermetic kernel builds beyond `kernel/upstream/PIN`.
