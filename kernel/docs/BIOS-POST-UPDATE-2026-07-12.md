# BIOS post-update review — 2026-07-12

Snapshot after reboot with BIOS update + user changes.  
Raw dumps: `docs/post-bios-20260712-1434/`

---

## Verdict

| Item | Before (2703) | After | Status |
|------|---------------|-------|--------|
| BIOS version | 2703 (2023-08) | **4505** (DMI Release Date 11/28/2025) | **OK** |
| Platform Firmware Revision | — | 45.5 | OK |
| Intel ME | 16.1.27.2176 | **16.1.38.2676** | **OK** (matches 4505 notes) |
| DRAM | **4000 MT/s** @ 1.1 V | **5600 MT/s** stable (XMP 6000 locked up; dialed back) | **OK — still large win** |
| GPU PCIe host max gen | **Gen 3** | **Gen 4** | **OK** |
| GPU PCIe under CUDA load | (not measured Gen4 before) | **Gen 4 @ 16 GT/s ×8**, P0 | **OK gen / still ×8** |
| GPU PCIe idle | Gen1 ×8 | Gen1 ×8, P8 | Normal power save |
| PEG root port max width | x8 | **still x8** | **Unchanged — still half lanes** |
| NVMe | Gen4 ×4 | Gen4 ×4 | OK |
| Secure Boot | disabled | disabled | OK for custom kernels |
| Turbo | on | on (`no_turbo=0`) | OK |
| Boot path | LUKS/btrfs/systemd-boot | unchanged | OK |
| Kernel | 7.1.2-3-cachyos | same | OK |

**Bottom line:** Flash + memory tune succeeded. PCIe **generation** fixed (Gen4 under load). PCIe **width remains ×8**. DRAM settled at **5600 MT/s** after **6000 locked the system** (normal for 4×16 GB DDR5 on Alder Lake).

---

## Confirmed details

### Firmware

```text
Vendor:  American Megatrends Inc.
Version: 4505
Release: 11/28/2025
ME:      16.1.38.2676
fwupd SPI BIOS version: 4505 (locked — normal)
```

### Memory (all four DIMMs)

```text
Part: F5-6000U4040E16G  (G.Skill)  — kit rated 6000
Size: 4 × 16 GiB = 64 GiB
Configured: 5600 MT/s @ 1.1 V   (was 4000; 6000 XMP unstable)
```

**Stability note (2026-07-12 later):** Full XMP **6000** caused system lockups. User set **5600** — confirmed via dmidecode on all four DIMMs. Keep **5600 as daily driver**.

Why this is expected:

- 4 DIMMs is harder than 2 on Alder Lake IMC
- Kit “6000” is often validated for 2-DIMM; 4×16 needs more voltage/training margin
- 5600 is still **+40%** vs old 4000 JEDEC — main win retained

Optional later (only if chasing 6000 again): small DRAM VDD bump per G.Skill QVL, loosen primary timings one step, or test with MemTest86 overnight — **not required** for daily use.

### GPU PCIe (RTX 4090 @ 01:00.0)

| State | Gen | Width | Notes |
|-------|-----|-------|--------|
| Idle (P8) | 1 | 8 | Normal; do not use idle for judgment |
| **CUDA load (P0)** | **4** | **8** | Gen fixed; width still half |

Root port `00:01.0` after update:

```text
LnkCap:  Speed 32GT/s, Width x8     # was Speed 8GT/s, Width x8
LnkCap2: Supported Link Speeds 2.5-32GT/s
LnkCtl2: Target Link Speed 16GT/s   # was 8GT/s
LnkSta under load: 16GT/s, Width x8
```

`nvidia-smi` host max: **4** (was 3).  
Resizable BAR still present (BAR1 32GB).

### Bandwidth (approx, one direction)

| Link | ~GB/s |
|------|-------|
| Ideal Gen4 ×16 | ~32 |
| **Now under load Gen4 ×8** | **~16** |
| Before max Gen3 ×8 | ~8 |
| Idle Gen1 ×8 | ~2 |

So host↔GPU bandwidth roughly **doubled** vs previous *maximum*, still **half** of a full x16 slot.

### Why still ×8?

Host root port **advertises max width x8**, so the 4090 cannot train x16. Not idle power management.

**Most likely (board design):** SSD in **M.2_1** — ASUS manual:

> When M.2_1 is occupied with SSD, PCIEX16(G5) will run x8 mode only.

Your only NVMe is the WD_BLACK SN750 SE (boot disk). If it sits in M.2_1 (top CPU Gen5 socket), GPU ×8 is **expected**, not a BIOS bug.

Full move plan: **[M2-GPU-LANE-SHARE.md](M2-GPU-LANE-SHARE.md)** — prefer **M.2_3** (chipset), leave **M.2_1 empty**.

---

## Other observations

- CPU microcode in OS still **0x3e** (package `intel-ucode`); separate from board ME/BIOS package.  
- EPP default after boot: `balance_performance` / governor `powersave` (intel_pstate) — normal; set `performance` for benches.  
- LUKS/btrfs cmdline unchanged; dual-boot infrastructure intact.  
- No regressions spotted in boot or NVIDIA driver load.

---

## Impact on goals

| Goal | Effect of this update |
|------|------------------------|
| Rust / cargo builds | **Yes** — 6000 MT/s DRAM helps L3-miss traffic and general bandwidth |
| Local LLMs (GPU) | **Yes** — Gen4 link for loads/copies; ×8 still limits peak transfer rate |
| “Feel more cached” | DRAM speed is the main felt change |
| Custom kernel later | Secure Boot still off; proceed when ready |

### Cargo bench (ran 2026-07-12 after 5600)

Same kernel, EPP performance, clean builds. Full table: `bench/20260712-post-bios-5600/COMPARE.md`

| Workload | Before (~4000) | After (5600) | Delta |
|----------|----------------|--------------|-------|
| combly release | 116.72 s | **76.50 s** | **−34.5%** |
| combly check | 20.54 s | **9.94 s** | **−51.6%** |
| beetv release | 78.16 s | **54.94 s** | **−29.7%** |

Sched messaging 2.544 → **2.309 s**. Noticeable win without a custom kernel.

---

## Checklist status

| Checklist item | Done? |
|----------------|-------|
| Flash 4505 | Yes |
| XMP / 6000 MT/s | Yes |
| PCIe Gen Auto/Gen4 (host max 4 under load) | Yes |
| PCIe full **x16** | **No** — still x8 max |
| ReBAR available | Yes (BAR visible) |
| Secure Boot off | Yes |
| VMD left alone (system booted) | Yes |

---

## Residual optional work

1. Hunt **x16** width in BIOS (bifurcation / M.2 lane map).  
2. Re-bench combly/beetv vs pre-XMP baseline.  
3. Continue plan: free `/boot` LTS only when building custom kernel; keep `linux-cachyos` default.
