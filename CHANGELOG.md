# Changelog

Each real publish tags `published/<stamp>` on its commit; the same stamp names the `appsynergy.db-<stamp>` snapshot on the Release. Entries list the packages that moved.

## Unreleased

| Package | Version | Change |
|---------|---------|--------|
| `appsynergy-mirrorlist` | 1-8 | `[appsynergy]` Server is the GitHub Release `repo-x86_64`; drops `[sdx]`. Existing hosts cannot discover this from the old origin — one-shot `pacman -U` of this package, then `pacman -Sy` |
| `appsynergy-branding` | 3-4 | os-release URLs point at `github.com/Appsynergy-io/appsynergy-linux` |
| `appsynergy-branding-desktop`, `appsynergy-wallpapers`, `appsynergy-ca-certificates` | 1-4 | `url=` points at the GitHub repo; no payload change |
| `appsynergy-keyring` | 1-3 | `url=` points at the GitHub repo; no payload change |
| gate | — | `make-srctars.sh` forces every mode bit (`--mode='u=rwX,go=rX,a-st'`): a GitHub Actions checkout under umask 002 wrote 664/775 and the payload sha256 drifted in CI only, the same class as the earlier setgid drift. Recorded sums unchanged |
| `build-repo.sh` | — | A version the published db already names is pulled from the Release, not rebuilt: makepkg output is never byte-identical, so every push to `main` was re-signing and re-uploading the same six versions. New bytes need a `pkgrel` bump; `REBUILD_PUBLISHED=1` forces a local build |
| CI | — | Container pinned to an `archlinux:base-devel` digest (Dependabot docker on `ci/gha`, gate stage `ci-image` keeps `ci.yml` equal); `dependency-review` job on PRs; `publish` runs in environment `repo-x86_64` and tags `published/<stamp>` after a real publish |
| gate | — | `check.sh` stage `cargo` runs `fmt --check`, `clippy -D warnings`, `test`, `cargo audit`, `cargo deny` (policy in `desktop/installer/deny.toml`); new stages `gitleaks` (whole history) and `dependabot` (every ecosystem covered). Installer formatted; dead `parse_raid1_choice` removed |
| CI | — | `dependabot-automerge` job arms `gh pr merge --auto --squash` on every Dependabot PR; it merges once `check` passes under the main ruleset |
| CI | — | `ci.yml` caches the pacman package cache (weekly key), the rustup toolchain + cargo registry + installer `target` (keyed on `rust-toolchain.toml` + `Cargo.lock`), and the kernel assets (keyed on the Release's asset list; `pull-kernel.sh` skips a file whose size matches). Restore everywhere, save on `main` only |
| `publish-repo.sh` | — | Refuses a staging db that drops a package the published db names (`ALLOW_DROP=1` to retire one); `FORCE_PUBLISH=1` bypasses the nothing-to-publish exit for a wrong or CDN-stale published db |
| CI | — | One workflow, `ci.yml`: `publish` now `needs: check` and runs on every push to `main`; `publish-repo.sh` exits early when the published db already describes the staged packages, so a docs-only merge uploads nothing. `check.sh` stage `workflows` fails on a second workflow file |
| ISO profile | — | `pacman.conf` carries `Server = file://@PKG_REPO@`; `build-iso.sh` renders it into the work dir and passes it via `mkarchiso -C`. The committed absolute path broke the build each time the checkout moved. `build-iso.sh` also drops the `KDIR` kernel-tree source and the `imma` fallbacks for `SUDO_USER`; the kernel comes from `packages/repo/x86_64` only |
| `appsynergy-mirrorlist` | 1-6 | scriptlet migrates legacy inline TrustAll `[appsynergy]` to the signed drop-in (keyring-gated, validated, backed up); depends on `appsynergy-keyring` |
| `appsynergy-linux` | 7.1.6-1 | **New, and replaces every kernel package.** Upstream CachyOS `linux-cachyos-server` built with ThinLTO (their published `-lto` recipe) under the AppSynergy name — `uname -r` = `7.1.6-1-appsynergy-linux`. Config is upstream's, unmodified: AppSynergy no longer maintains kernel Kconfig. One package for desktop and server, every metal. |
| `linux-appsynergy`, `linux-appsynergy-server-skylake`, `linux-appsynergy-server-tigerlake` | — | **Retired.** `build-repo.sh` drops them from staging. Hosts still running them keep working; swapping is a maintenance-window action because the package rename moves `/boot` filenames and needs new bootloader entries — procedure in `docs/AUDIT-REMEDIATION.md`. |
| `appsynergy-ca-certificates` | 1-3 | Root CA is the **only** trust anchor. Intermediate moves out of `anchors/` to the `trust-source/` top level, where `update-ca-trust(8)` gives it neutral trust — still completes chains for services that present only their leaf (vault-k2), no longer an independent trust root |
| ISO profile | — | `profiledef.sh` declares modes for `k3s`, `initrd-unlock` and `initcpio-install-ssh-unlock`, which shipped **non-executable** — mkarchiso discards on-disk modes (`cp -af --no-preserve=mode`) and anything undeclared lands `0644`. Installed systems were unaffected (the installer `chmod`s all three on the target); the live environment was not. `k3s.service.env` now declares `0600` instead of shipping runtime secrets world-readable. New `check.sh` stage `profile-modes` discovers every executable under `airootfs/` and fails on an undeclared one |
| `appsynergy-install` | — | Drops `-C -` from the `XferCommand` it writes into every target's `pacman.conf`. curl's resume appends to whatever bytes already sit at the output path, so an interrupted download left a package longer than the real one and failed its checksum on an already-committed disk |
| ISO `2026.08.08` | — | Rebuilt on `appsynergy-linux` 7.1.6-1 + headers; retired `linux-appsynergy{,-server-skylake,-server-tigerlake}` pruned from the offline payload. Picks up branding 3-3 (was 2-15), mirrorlist 1-6 (was 1-1), and ships `appsynergy-ca-certificates` 1-3 + `appsynergy-keyring` 1-2 for the first time. 2.0G, `sha256 85fcd586…47bfe`. Same `-C -` fix in the build-time `pacman.conf`, which is what caused the failure that surfaced it |

## v2026.08.06

First release with a chain of trust. Everything below published to
`[appsynergy]`; desktop + both production servers verified on it.

| Package | Version | Change |
|---------|---------|--------|
| `appsynergy-keyring` | 1-1 | NEW — GPG key `3B90…6AD1` (ed25519, expires 2029-08-05) |
| `appsynergy-ca-certificates` | 1-1 | NEW — Root + Intermediate CA as trust anchors |
| `appsynergy-mirrorlist` | 1-4 | registers repo via post_install (pacman has no conf.d); `SigLevel = Required DatabaseRequired` |
| `appsynergy-branding` | 3-2 | identity-only after split; verified sources |
| `appsynergy-branding-desktop` | 1-2 | NEW — icons/start/Plymouth, desktop-only |
| `appsynergy-wallpapers` | 1-2 | NEW — v3-H desktop + lock as KDE wallpaper packages |
| `linux-appsynergy-server-skylake` | 7.1.5-3 | first published (was install-only on OVH) |
| `linux-appsynergy-server-tigerlake` | 7.1.5-2 | first published (was install-only on NUC) |

All packages and the database are GPG-signed; clients enforce
`Required DatabaseRequired`. Kernel source pinned at CachyOS `74d5bae`
(`kernel/upstream/PIN`). Payload tarballs are deterministic and
checksum-verified by makepkg. Publish uploads packages before the database,
skips identical files, and leaves a dated db snapshot for rollback.

Rollout order for a machine that has never seen the signed repo:
`pacman-key --add` the key FIRST (no pacman transaction — a signed db with an
unknown key wedges every transaction, including `-U`), then `-U` keyring,
then sync.
