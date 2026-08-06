# appsynergy-linux

AppSynergy Linux — installer ISO, kernels, and the pacman repository that serves them.

Consolidated 2026-08-04 from three repositories (`appsynergy-desktop`, `kernel`, `appsynergy-packages`), whose full history is preserved here via `git subtree`.

| Subtree | Contents |
|---------|----------|
| `desktop/` | archiso profile, the Rust installer, ISO/rescue build scripts |
| `kernel/` | CachyOS config fragments per target metal, benchmarks, BIOS notes |
| `packages/` | PKGBUILDs, repo staging and publish scripts, pacman drop-in |

## Install target

Install `appsynergy-mirrorlist`. It owns the repo section — `/etc/pacman.conf.d/appsynergy.conf` plus the `Include` its `post_install` appends to `/etc/pacman.conf`, because pacman has no `pacman.conf.d` convention:

```ini
[appsynergy]
SigLevel = Required DatabaseRequired
Include = /etc/pacman.d/appsynergy-mirrorlist
```

Packages and the database are GPG-signed by `3B90D92D1E28E9E060D5C53D15D4351CF0D36AD1`, shipped in `appsynergy-keyring`. On a machine that has never seen the repo, add that key with `pacman-key` **before** any pacman transaction — a signed database with an unknown key wedges every transaction, including `-U`. `CHANGELOG.md` has the rollout order.

## Build

```bash
sudo desktop/scripts/build-iso.sh    # -> desktop/out/
```

Per-area detail lives in `desktop/README.md`, `kernel/README.md`, `packages/README.md`. Working notes for agents: `CLAUDE.md`.
