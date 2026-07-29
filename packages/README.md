# AppSynergy Linux packages

Public pacman repository for AppSynergy Linux (desktop workstation image).

## Pacman repo (HTTP)

| Item | Value |
|------|--------|
| Owner | `imabee` |
| Generic package name | `appsynergy-repo` |
| Version / arch path | `x86_64` |
| **Server URL** | `https://git.appsynergy.io/api/packages/imabee/generic/appsynergy-repo/x86_64` |

Add to `/etc/pacman.conf`:

```ini
[appsynergy]
SigLevel = Optional TrustAll
Server = https://git.appsynergy.io/api/packages/imabee/generic/appsynergy-repo/x86_64
```

Or install the `appsynergy-mirrorlist` package (ships that snippet).

Packages are **unsigned** until `appsynergy-keyring` ships; use `SigLevel = Optional TrustAll` only for `[appsynergy]`.

## What is published

| Package | Role |
|---------|------|
| `linux-appsynergy` / headers | Desktop workstation kernel (iGPU-tuned) |
| `linux-appsynergy-server` / headers | OVH / tunnel server kernel (same series; `server.fragment`) |
| `appsynergy-mirrorlist` | Registers the `[appsynergy]` repo |
| `appsynergy-branding` | os-release + motd + shell policy |

Server kernel build: `./scripts/build-linux-appsynergy-server.sh`  
(fragment: `/home/imma/projects/kernel/configs/server.fragment`).

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
