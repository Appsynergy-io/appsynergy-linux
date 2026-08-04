# Bench compare: baseline vs post-BIOS (DRAM 5600)

| | Before | After |
|--|--------|-------|
| When | 2026-07-12 earlier | 2026-07-12 ~15:57 |
| Kernel | 7.1.2-3-cachyos | same |
| BIOS | 2703 | **4505** |
| DRAM | ~4000 MT/s | **5600 MT/s** |
| EPP | performance | performance |
| cargo | clean, `-j24`, `CARGO_INCREMENTAL=0` | same |

**Negative delta = faster after.**

## Wall clock

| Workload | Before (s) | After 5600 (s) | Delta |
|----------|------------|----------------|-------|
| combly `build --release` | 116.72 | **76.50** | **−34.5%** |
| combly `check` | 20.54 | **9.94** | **−51.6%** |
| beetv-rs `build --release` | 78.16 | **54.94** | **−29.7%** |

## CPU time (user + sys)

| Workload | Before user/sys | After user/sys |
|----------|-----------------|----------------|
| combly-release | 172.46 / 15.28 | 158.86 / 11.09 |
| combly-check | 27.90 / 13.93 | 25.28 / 10.30 |
| beetv-release | 170.43 / 15.51 | 164.56 / 11.51 |

Wall time improved more than user CPU time → less wait on memory/I/O (DRAM 5600 + still-cold disk effects). Sys time also down.

## Cache miss rate (approx, sum atom+core)

| Workload | Before | After |
|----------|--------|-------|
| combly-release | ~24.4% | ~24.8% |
| combly-check | ~32.9% | (see perf file) |
| beetv-release | ~24.5% | (see perf file) |

Miss *rate* similar; absolute wall time still much better (higher effective bandwidth / lower latency per miss at 5600).

## Microbenches

| Bench | Before | After |
|-------|--------|-------|
| `perf bench sched messaging` | 2.544 s | **2.309 s** (−9.2%) |
| memcpy glibc 512MB | 21.24 GB/s | **21.67 GB/s** |
| memcpy x86-64-unrolled | 10.93 GB/s | **12.50 GB/s** |
| memcpy movsq | 11.65 GB/s | **13.30 GB/s** |

## Caveats

- Single run each side (no multi-run median). Variance exists for cargo (crate compile order, thermal).
- combly-check after clean can vary with residual caches; −50% is large — treat as directional.
- Same CachyOS kernel; change is **BIOS 4505 + RAM 5600** (and any other BIOS tweaks you set), not a custom kernel yet.

## Artifacts

- After: `bench/20260712-post-bios-5600/`
- Before: `bench/20260712-baseline-cachyos/`
