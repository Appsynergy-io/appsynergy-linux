# appsynergy-linux

AppSynergy Linux — installer ISO, kernels, and the pacman repository that serves them.

The live `[appsynergy]` origin is the GitHub Release tag `repo-x86_64`. The ISO is built from this tree and is not hosted.

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
packages/scripts/fetch-repo.sh       # staging from the GitHub Release (includes the kernel)
sudo desktop/scripts/build-iso.sh    # -> desktop/out/  (not uploaded)
```

Per-area detail lives in `desktop/README.md`, `kernel/README.md`, `packages/README.md`. Working notes for agents: `CLAUDE.md`.
