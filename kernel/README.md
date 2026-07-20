# Custom Linux kernel — Intel desktop, Rust-first

Optimized kernel design for **i9-12900K + ASUS Z690 + RTX 4090** on CachyOS, aimed at cargo/rustc throughput and KDE latency.

## Docs

| Path | Content |
|------|---------|
| **[docs/BIOS-POST-UPDATE-2026-07-12.md](docs/BIOS-POST-UPDATE-2026-07-12.md)** | BIOS 4505, RAM **5600** stable, PCIe Gen4×8 |
| [docs/BIOS-CHECKLIST.md](docs/BIOS-CHECKLIST.md) | BIOS settings + PCIe deep dive |
| **[docs/VERIFICATION.md](docs/VERIFICATION.md)** | Baseline benches, strip candidates |
| [docs/OPTIMIZED-KERNEL.md](docs/OPTIMIZED-KERNEL.md) | Custom kernel plan |
| [configs/rustopt.fragment](configs/rustopt.fragment) | Kconfig fragment |

## Baseline (recorded 2026-07-12)

`bench/20260712-baseline-cachyos/` on stock CachyOS 7.1.2-3:

| Workload | wall_sec |
|----------|----------|
| combly release -j24 | 116.72 |
| combly check -j24 | 20.54 |
| beetv-rs release -j24 | 78.16 |

## Scripts

| Script | Purpose |
|--------|---------|
| `scripts/bench-rust.fish` | Timed cargo + perf (A/B comparable) |
| `scripts/pgo-train-rust.fish` | Multi-project cargo load for AutoFDO |
| `scripts/sysctl-rustopt.conf` | `/etc/sysctl.d/` snippet |

## Order of work

1. ✅ 2026-07-12 userspace: `target-cpu=native` + tmpfs `CARGO_TARGET_DIR` + sysctl applied (sccache/lld pre-existed). No build-time win; native buys binary runtime. See `bench/20260712-1622-quickwins-warm/COMPARE.md`.
2. ✅ 2026-07-12 `/boot` freed: LTS removed → 329 MiB free; main cachyos kernel is the fallback.
3. Optional unused packages (firmware/orphans/mdadm/jfs/…) per VERIFICATION.md.
4. ✅ 2026-07-12 `linux-cachyos-rustopt 7.1.3-2` built (ThinLTO+O3, native, NR_CPUS=64, LSM/AMD/iGPU strip, AutoFDO-ready), installed alongside stock (entry "Linux Cachyos Rustopt", default stays stock), booted and A/B'd: builds −1%, memcpy +7%, no regressions — `bench/20260712-rustopt-AB/COMPARE.md`. Packages + dbg in `~/src/linux-cachyos/linux-cachyos/`.
5. Scheduler: EEVDF stays default — scx_lavd measured at build parity, 3× worse sched micro; use manually for desktop feel only.
6. ✅ 2026-07-12 Stage 2 AutoFDO: profiled rustopt under cargo load (`kptr_restrict=0` required), 1.77MB `kernel.afdo` via `llvm-profgen --kernel`, rebuilt with AUTOFDO_CLANG+PROPELLER_CLANG, installed 19:33 — **reboot pending**; stock boot verified intact.
7. Stage 3 Propeller (after reboot into AutoFDO kernel): longer profile run, `create_llvm_prof --format=propeller`, rebuild with `_propeller_profiles=yes`, final bench vs `bench/20260712-rustopt-AB`; then cmdline levers (`init_on_alloc=0`, opt-in `mitigations=off` entry, `preempt=voluntary`).

Do not remove everyday Plasma utilities (calculator, etc.).
