# Userspace quick wins — 2026-07-12 (vs post-BIOS-5600)

Kernel 7.1.2-3-cachyos unchanged. EPP `performance`, `CARGO_INCREMENTAL=0`, `-j24`, `cargo clean` per run. sccache + lld active in **all** runs (`~/.cargo/config.toml`, pre-dates today). New in these runs: `-C target-cpu=native` added to config rustflags. Warm = second run, sccache populated under the new flags — comparable to post-BIOS numbers.

## Wall seconds

| Workload | post-BIOS-5600 | quickwins warm | scx_lavd warm |
|----------|----------------|----------------|---------------|
| combly release | 76.50 | 80.68 | 81.35 |
| combly check | 9.94 | 10.13 | 10.88 |
| beetv release | 54.94 | 57.40 | — |
| beetv check | — | 6.95 | — |
| sched messaging | 2.309 | 2.365 | **6.907** |

Dirs: `20260712-1619-quickwins-cold`, `-1622-quickwins-warm`, `-1623/-1625-quickwins-beetv-*`, `-1628-quickwins-scx-lavd`.

## Conclusions

- `target-cpu=native`: +2–5% compile time (more LLVM work). Its win is **runtime of produced binaries** (AVX2 codegen for services/LLM crates), not build speed. Kept.
- sccache (79% hit rate) + lld were already configured before today — Grok's projected "10–40% userspace win" was already banked; no further build-time headroom in userspace flags.
- scx_lavd: build parity, sched-messaging 3× worse. Keep EEVDF default; start `scx_lavd` manually only for desktop feel during long builds (`sudo systemd-run --unit=scx-trial /usr/bin/scx_lavd`; stop with `systemctl stop scx-trial`).
- tmpfs `CARGO_TARGET_DIR` not measured (bench script unsets it by design); affects daily builds only.
- Next meaningful lever is the custom kernel (native + strip + NR_CPUS=64), stage 1 of docs/OPTIMIZED-KERNEL.md §6.

## State changes applied 2026-07-12

| Change | Where |
|--------|-------|
| `-C target-cpu=native` | `~/.cargo/config.toml` rustflags |
| `CARGO_TARGET_DIR=/tmp/cargo-target-imma` (tmpfs) | fish universal var; `cargo clean` in any project wipes the shared dir |
| swappiness 10, dirty_ratio 15/5, vfs_cache_pressure 50 | `/etc/sysctl.d/99-rustopt.conf` (overrides CachyOS 70-cachyos-settings) |
| LTS kernel removed (−308 MiB) | `/boot` now 329 MiB free; stale loader entry deleted; main cachyos kernel = fallback |
| `scripts/bench-rust.fish` fixed | was never functional: fish `set -l` invisible in functions + relative `$B` broke under `pushd`; earlier bench dirs were made with direct commands |
