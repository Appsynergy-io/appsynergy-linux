# CLAUDE.md — appsynergy-linux

AppSynergy Linux: the installer ISO, the kernels it ships, and the pacman repo that serves both. One system, three subtrees — they call into each other by path, so they version together.

## Commands

```bash
sudo desktop/scripts/build-iso.sh                      # installer ISO -> desktop/out/
sudo desktop/scripts/run-iso-build.sh                  # clean-build entrypoint (wipes dead work dirs)
desktop/scripts/stage-rescue-payload.sh                # OVH rescue tarball -> desktop/out/
packages/scripts/build-linux-appsynergy.sh             # desktop kernel
packages/scripts/build-linux-appsynergy-server.sh      # both server kernels
packages/scripts/build-linux-appsynergy-server-flavor.sh skylake|tigerlake
packages/scripts/build-repo.sh && packages/scripts/publish-repo.sh   # stage + publish pacman repo
```

## File map

| Path | Role |
|------|------|
| `desktop/iso/` | archiso profile (airootfs, `pacman.conf`) |
| `desktop/installer/` | `appsynergy-install` (Rust); `src/detect.rs` picks the kernel package from the live CPU |
| `desktop/scripts/` | ISO build, rescue payload staging, USB write |
| `kernel/configs/` | CachyOS config fragments — one per target metal |
| `kernel/bench/` | committed benchmark runs |
| `packages/pkgbuilds/` | PKGBUILDs (kernels, `appsynergy-branding`, `appsynergy-mirrorlist`) |
| `packages/repo/x86_64/` | local pacman staging, gitignored — `repo-add` output, input to publish |

## Invariants and gotchas

- **The published repo is the Gitea generic package**, `https://git.appsynergy.io/api/packages/imabee/generic/appsynergy-repo/x86_64`. Host `/etc/pacman.conf` and `/etc/pacman.d/appsynergy-mirrorlist` point there, never at a path in this tree. Moving this checkout cannot break installed systems; only ISO builds read the local repo.
- **`desktop/iso/pacman.conf` carries an absolute `file://` path** to `packages/repo/x86_64` — pacman config has no relative form. `build-iso.sh` asserts it matches `PKG_REPO` and aborts on drift. Update both together.
- **Scripts derive `MONO`** (`$(dirname "$0")/../..`) to reach a sibling subtree. Do not reintroduce absolute paths; the checkout has moved once already.
- **`desktop/iso/airootfs/opt/appsynergy/k3s/k3s.service.env` is gitignored** — mode 0600, holds runtime secrets.
- **`desktop/work-*/` are root-owned** multi-GB archiso scratch dirs; removing them needs root. `out/` is 16G of built ISOs. Both gitignored.
- Kernel invariants (`CONFIG_BRIDGE_NETFILTER` and k3s) live in `kernel/CLAUDE.md` — read it before touching any fragment.

## Conventions

Subtrees keep their own `.gitignore`; patterns anchored with a leading `/` resolve against the subtree, not the repo root.
