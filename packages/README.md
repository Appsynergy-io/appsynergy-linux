# AppSynergy Linux packages

Public pacman repository for AppSynergy Linux (desktop workstation image).

## Pacman repo (HTTP)

| Item | Value |
|------|--------|
| Owner | `imabee` |
| Generic package name | `appsynergy-repo` |
| Version / arch path | `x86_64` |
| **Server URL** | `https://git.appsynergy.io/api/packages/imabee/generic/appsynergy-repo/x86_64` |

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
| `appsynergy-linux` / headers | The kernel — upstream CachyOS `linux-cachyos-server` + ThinLTO, renamed. One package, every machine. |
| `appsynergy-keyring` | The GPG key every other package is verified against |
| `appsynergy-ca-certificates` | AppSynergy Root CA trust anchor (+ Intermediate as chain filler) |
| `appsynergy-mirrorlist` | Registers the `[appsynergy]` repo |
| `appsynergy-branding` | os-release + motd + greeting — every machine |
| `appsynergy-branding-desktop` | icons, start entry, Plymouth — graphical installs only |
| `appsynergy-wallpapers` | desktop + lock wallpapers — graphical installs only |

Kernel build: `./scripts/build-appsynergy-linux.sh`. There are no config fragments —
the contract is `kernel/upstream/PIN`.

Brave/Thorium stay as local USB payload or AUR for now (large / AUR).

## Layout

```text
pkgbuilds/     # PKGBUILDs
scripts/       # build-repo.sh publish-repo.sh
repo/x86_64/   # local staging (repo-add output); not committed (gitignored)
pacman/        # appsynergy.conf drop-in
```

## Publish (maintainer)

```bash
# stage .pkg.tar.zst into repo/x86_64/, then:
./scripts/build-repo.sh
./scripts/publish-repo.sh   # needs GITEA_TOKEN or tea config
```

Public GET (no auth) for package files once published.
