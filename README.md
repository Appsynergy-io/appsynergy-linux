# appsynergy-linux

AppSynergy Linux — installer ISO, kernels, and the pacman repository that serves them.

Consolidated 2026-08-04 from three repositories (`appsynergy-desktop`, `kernel`, `appsynergy-packages`), whose full history is preserved here via `git subtree`.

| Subtree | Contents |
|---------|----------|
| `desktop/` | archiso profile, the Rust installer, ISO/rescue build scripts |
| `kernel/` | CachyOS config fragments per target metal, benchmarks, BIOS notes |
| `packages/` | PKGBUILDs, repo staging and publish scripts, pacman drop-in |

## Install target

Add the repo to `/etc/pacman.conf`:

```ini
[appsynergy]
SigLevel = Optional TrustAll
Server = https://git.appsynergy.io/api/packages/imabee/generic/appsynergy-repo/x86_64
```

Or install `appsynergy-mirrorlist`, which ships that snippet. Packages are unsigned until `appsynergy-keyring` exists — scope `TrustAll` to `[appsynergy]` only.

## Build

```bash
sudo desktop/scripts/build-iso.sh    # -> desktop/out/
```

Per-area detail lives in `desktop/README.md`, `kernel/README.md`, `packages/README.md`. Working notes for agents: `CLAUDE.md`.
