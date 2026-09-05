# CLAUDE.md — appsynergy-linux

AppSynergy Linux: the installer ISO, the kernels it ships, and the pacman repo that serves both. One system, three subtrees — they call into each other by path, so they version together.

## Commands

```bash
scripts/check.sh                                       # release gate — run before every commit; CI runs exactly this
sudo desktop/scripts/build-iso.sh                      # installer ISO -> desktop/out/ (not uploaded)
sudo desktop/scripts/run-iso-build.sh                  # clean-build entrypoint (wipes dead work dirs)
desktop/scripts/stage-rescue-payload.sh                # OVH rescue tarball -> desktop/out/
packages/scripts/build-appsynergy-linux.sh             # the kernel (sandbox; one, for every metal)
packages/scripts/fetch-repo.sh                         # staging from the GitHub Release (ISO / local)
packages/scripts/build-repo.sh && packages/scripts/publish-repo.sh   # stage + publish pacman repo
packages/scripts/verify-repo.sh                        # assert published == staged; exit 1 on drift
```

## File map

| Path | Role |
|------|------|
| `desktop/iso/` | archiso profile (airootfs, `pacman.conf`) |
| `desktop/installer/` | `appsynergy-install` (Rust); `src/detect.rs` refuses CPUs the one v3 kernel cannot boot |
| `desktop/scripts/` | ISO build, rescue payload staging, USB write |
| `kernel/upstream/PIN` | the whole kernel contract — AppSynergy ships no Kconfig of its own |
| `packages/pkgbuilds/` | PKGBUILDs — `appsynergy-mirrorlist` and the identity split below; **no kernel** |
| `desktop/brand-review/` | wallpaper/icon **candidates** for review only; never read at runtime |
| `packages/pacman/SERVER` | the published pacman Server URL — one line, the contract |
| `packages/repo/x86_64/` | local pacman staging, gitignored — `repo-add` output, input to publish |

## Invariants and gotchas

- **Nothing a running machine reads may live in this checkout.** If a booted system needs it, it ships in a package. Violating this is invisible until the checkout moves: the desktop and lock wallpapers were set to absolute paths under `brand-review/` and died the moment `appsynergy-desktop/` became `appsynergy-linux/desktop/`. Wallpapers now come from `appsynergy-wallpapers`; keep it that way.
- **Identity is split by audience, not by topic.** `appsynergy-branding` = os-release, motd, greeting, ASCII — every machine. `appsynergy-branding-desktop` (icons, start entry, Plymouth) + `appsynergy-wallpapers` = graphical installs only. Servers must never pull either: it keeps cosmetic churn off production and stops a branding upgrade triggering `mkinitcpio -P` on a headless box. `install_branding` in `desktop/installer/src/main.rs` gates this; `adversarial_tests.rs` locks it.
- **Never glob `appsynergy-branding-*`** — it also matches `appsynergy-branding-desktop-*`. Anchor the version: `appsynergy-branding-[0-9]*`. Both `build-iso.sh` and the installer depend on this; an unanchored glob ships Plasma assets to servers and makes the ISO prune delete the identity package.
- **The published repo is the GitHub Release** `repo-x86_64`. The Server URL lives in `packages/pacman/SERVER`; `check.sh` stage `repo-url` fails if the mirrorlist, ISO remote fallback, or publish/verify/fetch scripts drift from it. Hosts read that URL via `appsynergy-mirrorlist`, never a path in this tree; only ISO builds read the local staging dir. Run `verify-repo.sh` after every publish.
- **pacman has no `pacman.conf.d` convention.** It reads only what `/etc/pacman.conf` explicitly `Include`s, so the drop-in `appsynergy-mirrorlist` ships is inert on its own. `>=1-2` appends the `Include` in `post_install`, gated on `pacman-conf --repo=appsynergy` so it no-ops where a section already exists. Appended **last**: repo order is priority order, so it can never shadow core/extra. `>=1-8` registers `[appsynergy]` only.
- **A published `pkgver-pkgrel` is immutable.** `build-repo.sh` pulls any version the published db already names from the Release instead of rebuilding it (makepkg output is never byte-identical), and `publish-repo.sh` exits early when the published db already describes staging. Bump `pkgrel` to ship new bytes; `ci.yml` publishes on every push to `main` and this is what makes that a no-op.
- **`publish-repo.sh` uploads packages before the database** (`gh release upload --clobber`). A db naming an unuploaded package 404s mid-transaction on every client. GitHub asset URLs 302; `verify-repo.sh` follows redirects.
- **`desktop/iso/pacman.conf` carries `Server = file://@PKG_REPO@`, never a real path.** pacman config has no relative form, so `build-iso.sh` renders the file into its work dir with `packages/repo/x86_64` substituted and passes that copy via `mkarchiso -C`. It aborts if the placeholder is gone. A pasted absolute path broke every ISO build the last two times the checkout moved.
- **Never put `-C -` in an `XferCommand`.** curl's resume appends to whatever bytes already sit at `%o`, so one stale partial in the cache yields a package *longer* than the real one and pacstrap dies with "invalid or corrupted package (checksum)" — cached `branding-desktop-1-3` was 239940 bytes against the repo's 239781. The same line is written into every installed system by `register_appsynergy_repo` in `desktop/installer/src/main.rs`, where it would instead fail a `pacman -Syu` on a committed disk. Fetch whole; these downloads are one-shot.
- **`appsynergy-keyring` errors during mkarchiso's pkglist step are expected.** `pacman -Q --sysroot` reads the airootfs `pacman.conf`, whose `[appsynergy]` is `Required DatabaseRequired`, while `/etc/pacman.d/gnupg` does not exist until `pacman-init.service` runs `pacman-key --init && --populate` at live boot. The image is correct: `appsynergy-mirrorlist` depends on `appsynergy-keyring`, so the key ships wherever the repo is configured. Do not silence this by baking a trustdb into the squashfs.
- **Scripts derive `MONO`** (`$(dirname "$0")/../..`) to reach a sibling subtree. Do not reintroduce absolute paths; the checkout has moved once already.
- **`desktop/iso/airootfs/opt/appsynergy/k3s/k3s.service.env` is gitignored** — mode 0600, holds runtime secrets.
- **An ISO-profile file's mode on disk means nothing.** mkarchiso copies `airootfs/` with `cp -af --no-preserve=ownership,mode`, so every mode comes from the `file_permissions` array in `desktop/iso/profiledef.sh` and anything undeclared ships `0644`. k3s and both LUKS-unlock scripts shipped non-executable for releases, invisible because the installer `chmod`s them on the target — but unusable in the live environment, which is where a rescue operator meets them. `check.sh` stage `profile-modes` discovers every executable under `airootfs/` and fails on an undeclared one; it also asserts `k3s.service.env` is declared `0600`, since `0644` in a squashfs is readable by anyone holding the USB.
- **`desktop/work-*/` are root-owned** multi-GB archiso scratch dirs; removing them needs root. `out/` is 16G of built ISOs. Both gitignored. On a KDE workstation exclude both from Baloo — the indexer holds `airootfs/proc` open and an aborted build then fails `umount` ("target is busy"), which `run-iso-build.sh` refuses to start against; clear it with `umount -l`.
- **The kernel is upstream's, unmodified.** `appsynergy-linux` is CachyOS `linux-cachyos-server` + ThinLTO, renamed and nothing else; there are no config fragments and `check.sh` fails if one reappears. Two consequences bite elsewhere: it is `GENERIC_V3`, so the installer must refuse pre-Haswell CPUs, and its `CONFIG_LSM` omits AppArmor, so the cmdline must add it. Read `kernel/CLAUDE.md` before touching any of it.

## Standards exceptions

- **The CI container is a rolling Arch userland.** `ci.yml` pins `archlinux:base-devel` by digest (mirrored in `ci/gha/Containerfile`, Dependabot-bumped monthly, `check.sh` stage `ci-image` keeps them equal), but every job still runs `pacman -Syu`: makepkg and namcap must match the rolling target the packages are built for.
- **No version tags, no release job.** `[appsynergy]` is a rolling GitHub Release; `publish` runs on every push to `main` and is a no-op unless a `pkgrel` moved. Each real publish tags `published/<stamp>` on its commit, matching the `appsynergy.db-<stamp>` snapshot it uploaded.

## Conventions

Subtrees keep their own `.gitignore`; patterns anchored with a leading `/` resolve against the subtree, not the repo root.
