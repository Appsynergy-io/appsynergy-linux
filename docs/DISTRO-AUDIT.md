# Distribution practice audit — 2026-08-06

Audit of appsynergy-linux against what a Linux distribution has to do to be
trustworthy and rebuildable. Findings are ranked by what breaks if ignored, each
with the evidence that produced it and the fix. Severity is about *distribution*
risk, not code style.

The through-line: **no published artifact can currently be traced to, or rebuilt
from, a commit.** Signing, reproducible builds and CI are the three legs of that,
and all three are missing. Everything else below is secondary.

## S1 — Packages are unsigned, and clients are told to trust anything

`SigLevel = Optional TrustAll` in `appsynergy-mirrorlist/appsynergy.conf`, live on
all three machines. No `appsynergy-keyring` package exists. No `.sig` file exists
anywhere in the tree or the published repo.

Anyone who compromises the Gitea account, or MITMs `git.appsynergy.io`, can serve
a package that runs arbitrary code as root on every AppSynergy machine via
`post_install`. `TrustAll` means pacman does not merely lack a key — it is
instructed not to care. The `404`s on `.sig` fetches during every install are
this gap being audible.

**Fix:** create `appsynergy-keyring` shipping the public key to
`/usr/share/pacman/keyrings/`; sign packages and the database with
`makepkg --sign` / `repo-add --sign`; move to `SigLevel = Required DatabaseRequired`.
Signing key lives in sdx, never on the build host's disk unencrypted. Roll out
keyring first, flip `SigLevel` second — the reverse order locks every machine out.

## S2 — No published package can be rebuilt from this repository

`KDIR="${KDIR:-/home/imma/src/linux-cachyos/linux-cachyos}"` in every kernel
build script. That path is an untracked checkout on one workstation, and no
CachyOS version or commit is pinned anywhere in-tree.

If that machine is lost, no shipped kernel can be reproduced — including the two
currently running in production. There is no way to answer "what source produced
`linux-appsynergy-server-skylake 7.1.5-3`?"

**Fix:** pin the upstream `pkgver`/commit in-tree; fetch it in the PKGBUILD's
`source=()` with a checksum rather than assuming a sibling directory.

## S3 — PKGBUILDs read from `$startdir`, which defeats verification

`appsynergy-branding`, `appsynergy-branding-desktop` and `appsynergy-wallpapers`
all declare `source=()` and `sha256sums=()`, then read files out of `${startdir}`
in `package()`.

makepkg therefore verifies nothing: there are no declared inputs to checksum. The
build depends on the state of the working tree rather than on anything recorded.
It also breaks `makepkg --source`, clean-chroot builds (`extra-x86_64-build`), and
any CI that does not run inside a git checkout at exactly the right path.

**Fix:** declare the real files in `source=()` with `sha256sums`, and use
`$srcdir` in `package()` — the pattern `appsynergy-mirrorlist` already follows.

## S4 — No CI, no lint, no gate

No `.github/`, `.gitea/`, `Makefile` or `justfile`. `shellcheck` is not installed.
Only the installer has tests (63); no package has one.

Every release is "run a script on the daily-driver workstation". All three defects
found on 2026-08-06 — the dead `pacman.conf.d` drop-in, the
`appsynergy-branding-*` glob also matching `-desktop-`, and publishing the
database before its packages — are mechanically detectable and would have been
caught by a modest pipeline.

**Fix:** a pipeline that runs `shellcheck`, `namcap` on every built package,
`cargo test`, builds all `any/` packages in a clean chroot, and finishes with
`verify-repo.sh`. Nothing publishes unless it passes.

## S5 — Build host, release host and daily driver are one machine

`build-repo.sh` scrapes `/home/imma/src/linux-cachyos/...` for kernels; the same
workstation stages, signs (once signing exists) and publishes.

A compromise of the desktop is a compromise of the distribution. There is also no
clean-room guarantee: builds inherit whatever is installed that day.

**Fix:** builds in a container/chroot; ideally a dedicated builder. This is what
makes S2 and S3 actually hold.

## S6 — No release identity

Zero git tags. No changelog. `pkgrel` is the only version signal, and nothing ties
a published package to a commit. "What shipped last Tuesday" is unanswerable.

**Fix:** tag releases; embed the git SHA into each package (a
`/usr/share/appsynergy/BUILDINFO`, or in `pkgdesc`); keep a changelog that names
the packages a release moved.

## S7 — Developer paths still hardcoded in ~10 places

`CLAUDE.md` says not to reintroduce absolute paths; they are still present:

| Path | Site |
|------|------|
| `/home/imma/src/linux-cachyos/...` | `build-repo.sh`, both kernel build scripts, `install-linux-appsynergy*.sh`, `build-iso.sh` |
| `/home/imma/projects/appsynergy-rs/ops/k3s/config.yaml` | `stage-k3s.sh` — a silent cross-repo dependency |
| `/home/imma/projects/combly`, `/beetv-rs` | `run-post-bios-bench.sh` |
| `file:///home/imma/projects/appsynergy-linux/...` | `desktop/iso/pacman.conf` (known; asserted by `build-iso.sh`) |

`stage-k3s.sh` is the worst: an ISO build silently depends on another checkout
being present, and produces a different image if it is not.

**Fix:** env vars with no default, failing loudly when unset, or vendor the input.

## S8 — Publishing is destructive; there is no rollback

`publish-repo.sh` replaces the database in place. There is no way to serve
yesterday's repository. If a bad package ships, every machine that syncs gets it
and recovery is manual on each host.

**Fix:** date-stamped repo snapshots with `latest` as a pointer, so rollback is
repointing rather than rebuilding. Keep the previous N package versions published
so `pacman -U` of a known-good version always works.

## S9 — 111 MB of git history is mostly rejected artwork

`.git` is 111 MB. The largest committed blobs are wallpaper candidates that were
never chosen — 16 MB, 11.9 MB, 6.2 MB, and so on, roughly 50 MB of
`brand-review/` in history permanently.

Review scratch is not product. Only the chosen master belongs in the tree, and it
now lives correctly in `appsynergy-wallpapers`.

**Fix:** keep future candidates out of git (separate assets store); accept the
existing history unless a rewrite is worth the disruption.

## S10 — One flat repo serves two products

`[appsynergy]` carries desktop kernels, server kernels and graphical packages in
one namespace. Package split and the installer's variant gate make it impossible
for a server to *acquire* a desktop package by dependency, but not impossible for
a human to install one by hand.

**Fix:** only if the hard wall is wanted — separate `appsynergy`/`appsynergy-desktop`
repos. Not obviously worth doubling the publish surface at three machines.

## Order of work

1. **S1 keyring + signing** — the only finding with a live security consequence.
2. **S2/S3 pinned, declared sources** — without these, signing certifies something unrebuildable.
3. **S4 CI** — locks 1–3 in and stops regressions.
4. **S8 repo snapshots** — cheap, and the difference between a bad publish being an inconvenience or an outage.
5. S5, S6, S7, S9, S10 as capacity allows.
