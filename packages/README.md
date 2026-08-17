# AppSynergy Linux packages

Public pacman repository for AppSynergy Linux.

## Pacman repo (HTTP)

| Item | Value |
|------|--------|
| Origin | GitHub Release tag `repo-x86_64` |
| **Server URL** | `https://github.com/Appsynergy-io/appsynergy-linux/releases/download/repo-x86_64` |
| Source of truth | `packages/pacman/SERVER` |

Install `appsynergy-mirrorlist`; it owns the repo section and appends the `Include` to `/etc/pacman.conf` in `post_install` (pacman has no `pacman.conf.d` convention). What it ships as `/etc/pacman.conf.d/appsynergy.conf`:

```ini
[appsynergy]
SigLevel = Required DatabaseRequired
Include = /etc/pacman.d/appsynergy-mirrorlist
```

Packages **and** the database are GPG-signed by `3B90D92D1E28E9E060D5C53D15D4351CF0D36AD1` (`appsynergy-keyring`); clients enforce that signature. `build-repo.sh` signs by default and `publish-repo.sh` refuses to publish a missing `.sig`. Add the key with `pacman-key` before the first sync — a signed database with an unknown key wedges every transaction, including `-U`.

## What is published

| Package | Role |
|---------|------|
| `appsynergy-linux` / headers | The kernel — built in a sandbox, hosted on the same Release. CI does not compile it. |
| `appsynergy-keyring` | The GPG key every other package is verified against |
| `appsynergy-ca-certificates` | AppSynergy Root CA trust anchor (+ Intermediate as chain filler) |
| `appsynergy-mirrorlist` | Registers the `[appsynergy]` repo |
| `appsynergy-branding` | os-release + motd + greeting — every machine |
| `appsynergy-branding-desktop` | icons, start entry, Plymouth — graphical installs only |
| `appsynergy-wallpapers` | desktop + lock wallpapers — graphical installs only |

Kernel build: `./scripts/build-appsynergy-linux.sh` (sandbox). There are no config fragments — the contract is `kernel/upstream/PIN`.

## Layout

```text
pkgbuilds/     # PKGBUILDs
scripts/       # build-repo.sh publish-repo.sh fetch-repo.sh pull-kernel.sh
repo/x86_64/   # local staging (repo-add output); not committed (gitignored)
pacman/        # SERVER + appsynergy.conf drop-in
```

## Publish

```bash
./scripts/build-repo.sh
./scripts/publish-repo.sh   # needs `gh` (GITHUB_TOKEN or gh auth)
./scripts/verify-repo.sh
```

`packages.yml` on `main` is the publisher after bootstrap. Public GET, no auth.
