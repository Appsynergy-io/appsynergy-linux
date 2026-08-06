# Changelog

Releases are git tags (`vYYYY.MM.DD`); each entry lists the packages that moved.

## Unreleased

| Package | Version | Change |
|---------|---------|--------|
| `appsynergy-mirrorlist` | 1-6 | scriptlet migrates legacy inline TrustAll `[appsynergy]` to the signed drop-in (keyring-gated, validated, backed up); depends on `appsynergy-keyring` |

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
