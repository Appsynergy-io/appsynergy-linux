# Custom Linux kernel — AppSynergy desktop + server

CachyOS-based, **same version line** (e.g. 7.1.x):

| Variant | Package | Role |
|---------|---------|------|
| **Desktop** | `linux-appsynergy` | i9-12900K / Z690, Rust compile + Plasma |
| **Server skylake** | `linux-appsynergy-server-skylake` | E3-1270 v6 max: skylake, igb, WG/nft/XDP |
| **Server tigerlake** | `linux-appsynergy-server-tigerlake` | i7-1185G7 max: tigerlake, igc, WG/nft/XDP |

Installer: you pick **desktop|server**; kernel package is **CPU-auto** (server: skylake **or** tigerlake only, not both).  
Server OS keep list: **[docs/SERVER-OS.md](docs/SERVER-OS.md)**.

## Docs

| Path | Content |
|------|---------|
| **[docs/SERVER-OS.md](docs/SERVER-OS.md)** | **Server OS variant + keep list** (install, services, NOT-keep) |
| **[docs/SERVER-KERNEL.md](docs/SERVER-KERNEL.md)** | `linux-appsynergy-server` + `server.fragment` |
| **[docs/SERVER-SECURITY.md](docs/SERVER-SECURITY.md)** | TPM / SSH unlock + hardening files |
| **[docs/BIOS-POST-UPDATE-2026-07-12.md](docs/BIOS-POST-UPDATE-2026-07-12.md)** | BIOS 4505, RAM **5600** stable, PCIe Gen4×8 |
| [docs/BIOS-CHECKLIST.md](docs/BIOS-CHECKLIST.md) | BIOS settings + PCIe deep dive |
| **[docs/VERIFICATION.md](docs/VERIFICATION.md)** | Baseline benches, strip candidates |
| [docs/OPTIMIZED-KERNEL.md](docs/OPTIMIZED-KERNEL.md) | Desktop custom kernel plan |
| [configs/rustopt.fragment](configs/rustopt.fragment) | Desktop Kconfig (`i915=m`) |
| [configs/igpu.fragment](configs/igpu.fragment) | Desktop iGPU (`i915=y`) |
| [configs/server-skylake.fragment](configs/server-skylake.fragment) | OVH E3-1270 v6 max Kconfig |
| [configs/server-tigerlake.fragment](configs/server-tigerlake.fragment) | NUC i7-1185G7 max Kconfig |
| [configs/server.fragment](configs/server.fragment) | Legacy portable (do not ship) |
| [scripts/sysctl-server.conf](scripts/sysctl-server.conf) | Server sysctl (FQ/BBR/forwarding) |

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
| `scripts/sysctl-rustopt.conf` | Desktop `/etc/sysctl.d/` snippet |
| `scripts/sysctl-server.conf` | Server tunnel/WG sysctl |
| `scripts/modules-load-server.conf` | `nf_conntrack` preload for sysctl |

## Order of work

1. ✅ 2026-07-12 userspace: `target-cpu=native` + tmpfs `CARGO_TARGET_DIR` + sysctl applied (sccache/lld pre-existed). No build-time win; native buys binary runtime. See `bench/20260712-1622-quickwins-warm/COMPARE.md`.
2. ✅ 2026-07-12 `/boot` freed: LTS removed → 329 MiB free; main cachyos kernel is the fallback.
3. Optional unused packages (firmware/orphans/mdadm/jfs/…) per VERIFICATION.md.
4. ✅ 2026-07-12 `linux-cachyos-rustopt 7.1.3-2` built (ThinLTO+O3, native, NR_CPUS=64, LSM/AMD/iGPU strip, AutoFDO-ready), installed alongside stock (entry "Linux Cachyos Rustopt", default stays stock), booted and A/B'd: builds −1%, memcpy +7%, no regressions — `bench/20260712-rustopt-AB/COMPARE.md`. Packages + dbg in `~/src/linux-cachyos/linux-cachyos/`.
5. Scheduler: EEVDF stays default — scx_lavd measured at build parity, 3× worse sched micro; use manually for desktop feel only.
6. ✅ 2026-07-12 Stage 2 AutoFDO: profiled rustopt under cargo load (`kptr_restrict=0` required), 1.77MB `kernel.afdo` via `llvm-profgen --kernel`, rebuilt with AUTOFDO_CLANG+PROPELLER_CLANG, installed 19:33 — **reboot pending**; stock boot verified intact.
7. Stage 3 Propeller (after reboot into AutoFDO kernel): longer profile run, `create_llvm_prof --format=propeller`, rebuild with `_propeller_profiles=yes`, final bench vs `bench/20260712-rustopt-AB`; then cmdline levers (`init_on_alloc=0`, opt-in `mitigations=off` entry, `preempt=voluntary`).

Do not remove everyday Plasma utilities (calculator, etc.).
