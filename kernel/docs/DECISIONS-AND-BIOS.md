# Locked decisions + BIOS findings (your answers)

## Your constraints (locked)

| Constraint | Decision |
|------------|----------|
| Nothing that breaks projects | **Hard no** on removing: docker, cuda, nvidia stack, rustup/clang/llvm, steam, btrfs/cryptsetup, wireguard, scx packages, beetv-worker deps, llama.cpp |
| NTFS from Linux | **Not needed** — leave Windows dual-boot intact; Linux kernel can omit `ntfs3` (Windows boot does not use it) |
| Beat CachyOS but must boot | Custom kernel as **extra** boot entry; keep stock `linux-cachyos` forever as fallback |
| Dual-boot menu | **systemd-boot** already: CachyOS + CachyOS-LTS + Windows EFI. We add a third Linux entry; Windows stays |
| scx / scheduler | Experiment **after** stable native kernel; default stays EEVDF until proven |
| Noticeable + stable | Prefer **BIOS RAM + userspace first** (you will notice), then careful custom kernel; no mitigations-off daily |

---

## Dual-boot plan (safe)

### What you have now

- Firmware: UEFI, Secure Boot **disabled** (good for custom kernels)
- Loader: **systemd-boot**, timeout **5 s**, default `linux-cachyos`
- Entries: `linux-cachyos.conf`, `linux-cachyos-lts.conf`
- Windows: EFI `Boot0000* Windows Boot Manager` (same ESP)
- `/boot` free: **~91 MiB** — **not enough** for another full kernel+initramfs (~17M + ~220M)

### What we will do

1. **Remove only LTS** (`linux-cachyos-lts` + headers + lts-nvidia-open)  
   - Frees ~222 MB initramfs + 16 MB vmlinuz on ESP  
   - You still select **CachyOS (current)** vs **Windows** vs **custom rustopt**  
   - LTS is not required for dual-boot; it is a second Linux, not Windows  

2. Build custom kernel as package name e.g. `linux-rustopt` with its own:
   - `/boot/vmlinuz-linux-rustopt`
   - `/boot/initramfs-linux-rustopt.img`
   - `/boot/loader/entries/linux-rustopt.conf`

3. **Default entry stays `linux-cachyos`** until you trust rustopt for a week.  
   At boot: arrow keys pick CachyOS / rustopt / (Windows via firmware menu or optional systemd-boot Windows entry).

4. Never replace or delete the working CachyOS entry in the same step as first boot of rustopt.

### Menu sketch after change

```
Linux Boot Manager (systemd-boot)   ← default UEFI entry
  linux-cachyos          ← stock, proven (DEFAULT)
  linux-rustopt          ← experimental, max-perf
  (optional) Windows     ← or use UEFI boot menu / Boot0000
```

Firmware boot order already has Windows; hold the board’s boot-menu key (often F8/Esc on ROG) anytime.

---

## Hard-no removals (projects)

**Keep forever for your stack:** docker/containerd, cuda, nvidia-utils (+ lib32), steam, rustup, clang/llvm, btrfs-progs, cryptsetup, wireguard-tools, qemu-base, llama.cpp, scx-scheds (for later experiments), current `linux-cachyos` + headers + nvidia-open.

**OK to drop from Linux only (not projects):**

- LTS kernel packages (frees boot; not used as daily)
- Linux `ntfs3` in *custom* config only (Windows dual-boot unchanged)
- Unused Wi-Fi vendor firmwares, jfs/f2fs tools, mdadm (after confirmed no arrays)
- Not: cuda, browsers you use, compilers, docker

---

## “I want the CPU to cache much more” — honest model

Software **cannot enlarge** L1/L2/L3. What you *can* do:

| Lever | Effect on “cache behavior” | Noticeable? |
|-------|----------------------------|-------------|
| **DDR5 4000 → 6000 MT/s (XMP)** | Real DRAM bandwidth + lower latency into L3 misses | **Yes** (compiles, LLMs CPU-side, everything) |
| Smaller/hotter kernel (native + strip) | Slightly better I-cache for *kernel* paths | Subtle under load |
| THP / hugepages for LLM | Fewer TLB misses (page-table “cache”) | **Yes** for large models |
| Userspace native + mold + tmpfs target | Less I/O, better code for rustc | **Yes** for Rust |
| GPU PCIe Gen/host max | LLM load/offload bandwidth | Yes if model loads or multi-GPU; less if fully in VRAM |
| Kernel PGO | Hot syscall layout | Small |

Your baseline already showed cargo is **~85–90% userspace**. The single biggest “feel it” win on this box is almost certainly **RAM running at JEDEC 4000 instead of kit-rated 6000**.

---

## Bad / suboptimal BIOS settings (live evidence)

Board: **ASUS ROG STRIX Z690-E GAMING WIFI**  
BIOS now: **2703 (2023-08-11)** — latest on ASUS site is **4505 (2025-12-15)** (~2+ years behind).

### 1. CRITICAL — RAM not at XMP (biggest issue)

| | |
|--|--|
| Kit | G.Skill **F5-6000** U4040 4×16 GB (64 GB) |
| Running | **4000 MT/s @ 1.1 V** (JEDEC) |
| Expected with XMP | **6000 MT/s** (with correct profile/voltage) |

This is **not** a kernel problem. At 4000 MT/s every L3 miss is slower than it should be — Rust builds, local LLMs (CPU or host→GPU copies), desktop, everything.

**BIOS (typical ASUS names):**

1. Exit → Advanced Mode (F7)  
2. **Ai Tweaker** → **Ai Overclock Tuner** → **XMP I** (or DOCP/EXPO-equivalent; on Intel ASUS it is XMP)  
3. Confirm DRAM Frequency shows **DDR5-6000**  
4. Save (F10), boot, verify:

```fish
sudo dmidecode -t memory | rg "Configured Memory Speed"
# want: 6000 MT/s
```

**Stability note:** 4×16 GB DDR5-6000 on Alder Lake can be picky. If unstable: try XMP with slight DRAM voltage bump per G.Skill QVL, or 5600/5200 as stable middle ground. Still far better than 4000.

### 2. HIGH — GPU PCIe host capped Gen3 **and** ×8

Measured on CPU PEG root port `00:01.0` (not just nvidia idle):

- `LnkCap: Speed 8GT/s, Width x8` — host max **Gen3 ×8**
- GPU: `LnkSta … (downgraded)` width ×8; idle Gen1 is normal P8 power save
- NVMe path Gen4×4 is fine (unrelated)

**BIOS check:** PCIEX16_1 speed **Auto/Gen4**, bifurcation **x16** (not x8x8), ReBAR + Above 4G on, top slot, watch M.2 lane share.

Full tables, bandwidth sketch, and verify commands: **[BIOS-CHECKLIST.md — PCIe deep dive](BIOS-CHECKLIST.md#pcie-deep-dive-why-it-looked-slow)**.

### 3. MEDIUM — BIOS age (2703 → 4505)

Update path: ASUS support → ROG STRIX Z690-E → BIOS 4505. Prefer **USB BIOS Flashback** if you update (more reliable). After update: re-enable XMP, re-check boot order, re-check Secure Boot off.

Benefits: memory training, ME firmware, security, sometimes PCIe/USB quirks.

### 4. LOW / situational

| Setting | Your state | Advice |
|---------|------------|--------|
| Secure Boot | Disabled | Keep off for custom kernels |
| VT-x | On | Keep (qemu/docker) |
| VMD / RST | **On** (NVMe under VMD) | **Do not disable** without a migration plan — boot disk depends on it |
| Hyper-Threading | On | Keep for compiles (`-j24`) |
| Multi-Core Enhancement / ASUS MultiCore Enhancement | Unknown | “Enabled – Remove All Limits” can help all-core turbo for cargo; watch thermals/power |
| C-states | C1E–C10 available | Keep for idle; for pure bench runs optional “C-states less aggressive” — daily driver keep default |
| Intel SpeedStep / Speed Shift | Working via intel_pstate | OK |
| iGPU | Not in DRM | Already effectively off — fine for 4090-only |
| Fast Boot | Unknown | Can hide USB/boot devices; if dual-boot flaky, set **Thorough** |

### 5. Not bad

- Turbo enabled (`no_turbo=0`)  
- 450 W GPU power limit available  
- THP always in OS  
- Mitigations on Alder Lake are relatively cheap; **do not** use `mitigations=off` for daily driver  

---

## Max performance + max stability strategy (phased)

### Phase 0 — Noticeable, low risk (do this week)

1. **Enable XMP → 6000** (or highest stable). Re-run combly-release bench; compare to 116.72 s baseline.  
2. Install **mold**, set native RUSTFLAGS, tmpfs `CARGO_TARGET_DIR`.  
3. For local LLMs: ensure models stay in **VRAM**; use CUDA builds of llama.cpp; after XMP, host preprocess/tokenize faster too.  
4. Optional BIOS update 2703 → 4505, then re-apply XMP.

### Phase 1 — Dual-boot free space

```fish
# ONLY after you confirm you do not need LTS recovery kernel
sudo pacman -Rns linux-cachyos-lts linux-cachyos-lts-headers linux-cachyos-lts-nvidia-open
df -h /boot   # need ~300M+ free before custom kernel
```

Stock **linux-cachyos remains**. Windows unchanged.

### Phase 2 — Custom kernel (beat Cachy, keep fallback)

- Package `linux-rustopt` via Cachy PKGBUILD + corrected fragment  
- **Default boot = linux-cachyos**  
- First week: manually pick rustopt from menu  
- NVIDIA: rebuild open module for new kernel version (same major)  
- If anything fails: reboot → choose CachyOS  

### Phase 3 — Experiment (noticeable under load)

- Try `scx_lavd` while cargo + Plasma (service off by default)  
- Optional AutoFDO after rustopt is daily-driver for a week  
- Do **not** stack scx + BORE + Full LTO + PGO all at once  

---

## What “beating CachyOS” will feel like

| Source | You notice? |
|--------|-------------|
| XMP 4000→6000 | Yes — snappier, faster big compiles/LLM host work |
| mold + native rustc flags + tmpfs target | Yes — Rust wall times |
| Custom native kernel | Mild–moderate; more under desktop+compile; stability first |
| GPU PCIe host Gen fix | If transfers were Gen-limited; check after BIOS |

We already have **quantified baseline** (`bench/20260712-baseline-cachyos`). After XMP alone, re-run the same bench before any kernel change so gains are attributed correctly.

---

## Next step (recommended)

1. You enable XMP (and optional BIOS update) in firmware.  
2. Tell me post-XMP `Configured Memory Speed` and whether boot is stable.  
3. I re-run baseline bench → “XMP-only after”.  
4. Then free LTS from `/boot` and build rustopt as a **non-default** menu entry.

No project packages removed. No default kernel swapped without your OK.
