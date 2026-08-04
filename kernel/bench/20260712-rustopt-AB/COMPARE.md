# rustopt kernel A/B — 2026-07-12

Same script/protocol both sides (`scripts/bench-rust.fish`: cargo clean, no sccache,
config rustflags incl. native, EPP performance, -j24). Stock = 7.1.2-3-cachyos
(`20260712-1717/-1719-stock-prereboot-*`); rustopt = 7.1.3-2-cachyos-rustopt
(`20260712-1724/-1725-rustopt-*`, `-1729-rustopt-combly-run2`). First rustopt combly
run (81.35) discarded — first compile after reboot, cold page cache; run2 is warm.

## Wall seconds

| Workload | stock | rustopt | Δ |
|----------|-------|---------|---|
| combly release | 79.98 | 78.87 | −1.4% |
| combly check | 10.06 | 9.91 | −1.5% |
| beetv release | 56.54 | 56.23 | −0.5% |
| beetv check | 7.00 | 7.02 | ~0 |
| sched messaging | 2.447 / 2.608 | 2.486 / 2.575 / 2.513 | parity |
| memcpy single-thread | 23.20 GB/s | 24.83 GB/s | **+7.0%** |

## Conclusions

- Builds ~1% faster — real but marginal; compile time is userspace-bound (sys_sec is
  only ~12 s of ~80 s wall, so even a much better kernel moves little).
- memcpy +7% is the clearest native-codegen win (kernel copy paths).
- No regressions: checks flat, sched parity, NVIDIA/Docker/LUKS all fine on rustopt.
- Remaining kernel-side lever: AutoFDO (stage 2) — profile under cargo load, rebuild;
  targets the ~12 s sys time + page-fault/IRQ paths. dbg package already built.
