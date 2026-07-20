# BIOS checklist — ROG STRIX Z690-E + i9-12900K

Enter BIOS: **Del** (or **F2**) at power-on. Use **Advanced Mode** (**F7**).

---

## Machine context (scanned 2026-07-12)

| Item | Value |
|------|--------|
| Board | ASUS ROG STRIX Z690-E GAMING WIFI |
| BIOS | **4505** as of 2026-07-12 (was 2703) — see [BIOS-POST-UPDATE-2026-07-12.md](BIOS-POST-UPDATE-2026-07-12.md) |
| CPU | i9-12900K (Alder Lake hybrid, 8P+8E / 24 threads) |
| RAM | G.Skill **F5-6000** 4×16 GB — **stable at 5600 MT/s** (6000 locked up; was 4000) |
| GPU | RTX 4090 — under load **Gen4 ×8** (gen fixed; **width still ×8**) |
| Boot | UEFI, Secure Boot **off**, systemd-boot + Windows Boot Manager |
| Storage | WD_BLACK SN750 SE NVMe under **Intel VMD** — do not disable VMD casually |
| Flash media | Prepared stick: FAT32 label **BIOS4505**, root file **`SZ690E.CAP`** |

Related:

- **[BIOS-POST-UPDATE-2026-07-12.md](BIOS-POST-UPDATE-2026-07-12.md)** — review after flash + XMP/PCIe changes  
- [BIOS-UPDATES.md](BIOS-UPDATES.md) — 4505 download, SHA-256, changelogs  
- [DECISIONS-AND-BIOS.md](DECISIONS-AND-BIOS.md) — dual-boot, project hard-nos, strategy  
- Firmware files: `firmware/ROG-STRIX-Z690-E-4505/`

---

## Do these (in order)

### 1. Update BIOS (recommended)

| From | To |
|------|-----|
| 2703 | **4505** |

- Prefer **USB BIOS Flashback** (rear Flashback port + button).  
- Stick already prepared: **`SZ690E.CAP`** on FAT32 partition labeled **BIOS4505**.  
- EZ Flash 3 from BIOS also works (pick either CAP name on the stick).  
- After update: **re-apply every setting below** (XMP and PCI options often reset).  
- ME is updated with the CAP and **stays** even if you roll BIOS back later.

Flashback steps:

1. PSU **on**, PC can be **off**.  
2. USB in **BIOS Flashback** port only (I/O shield label).  
3. Press Flashback until LED blinks; wait until LED stops (several minutes).  
4. Boot → Del → Advanced Mode → reconfigure.

**Cannot flash this board safely from Linux** (`fwupd` SPI **locked**; no LVFS CAP). Prepare USB from Linux only.

### 2. Memory speed (highest priority)

- **Ai Tweaker → Ai Overclock Tuner → XMP I** (or manual DRAM frequency)  
- **This machine:** full **6000 locked up** with 4×16 GB → daily driver is **DDR5-5600**  
- Still far better than JEDEC **4000** — keep 5600 unless you invest time in manual tuning  
- If unstable at 5600: try **5200**; if solid at 5600: leave it  

After boot:

```fish
sudo dmidecode -t memory | rg "Configured Memory Speed|Part Number"
# current daily target: 5600 MT/s (not 4000)
```

### 3. PCIe / GPU (RTX 4090) — critical detail below

Menu (names vary): **Advanced → PCI Subsystem Settings** / **Onboard Devices** / **PCIEX16_1**

| Setting | Target |
|---------|--------|
| PCIEX16_1 / PEG link **speed** | **Auto** or **Gen4** (not Gen1/2/3) |
| **Bifurcation** / multi-GPU lane split | **Disabled / Auto / x16** — **not x8x8** |
| **Resizable BAR** | **Enabled** |
| **Above 4G Decoding** | **Enabled** |
| CSM | **Disabled** (UEFI) |

Physical:

- GPU in **top** full-length CPU slot (you are already on `01:00.0` — correct).  
- Prefer direct slot; risers often force Gen3 and/or x8.  
- **M.2_1 occupied → GPU forced ×8** (ASUS manual). Prefer boot SSD on **M.2_3**, leave M.2_1 empty. See [M2-GPU-LANE-SHARE.md](M2-GPU-LANE-SHARE.md).

See **[PCIe deep dive](#pcie-deep-dive-why-it-looked-slow)** for measured root-cause.

### 4. CPU performance (stable “fast” profile)

- **Intel Turbo Boost → Enabled**  
- **Hyper-Threading → Enabled**  
- **Intel SpeedStep / Speed Shift → Enabled** (OS still sets EPP)  
- **ASUS MultiCore Enhancement** (if present):  
  - Max all-core cargo: **Enabled – Remove All Limits** (watch power/temps)  
  - Safer: **Enabled – Enforce All Limits** or Auto  
- After 3603+ BIOS family: **Performance Preferences** may offer Intel Default vs **ASUS Advanced OC Profile** — for max compile throughput prefer ASUS OC / MCE only if thermals OK; avoid **Intel Baseline** for daily max perf  
- **C-states**: leave enabled for daily use  

### 5. Virtualization (Docker / QEMU)

- **VT-x → Enabled**  
- **VT-d / IOMMU → Enabled**  

### 6. Boot / dual-boot safety

- **Secure Boot → Disabled** (custom kernels)  
- **Fast Boot → Disabled** / Thorough if dual-boot is flaky  
- Boot order: **Linux Boot Manager** (systemd-boot) primary; **Windows Boot Manager** still listed  
- **CSM → Disabled**  

### 7. Storage (do not brick boot)

- **VMD / Intel RST → leave ON** (root NVMe is under VMD today)  
- Do **not** switch VMD off or “RAID → AHCI” without a full reinstall plan  
- No experimental RAID modes  

### 8. iGPU (optional)

- Primary display **PEG / PCIe**; iGPU multi-monitor off if unused  
- Already no iGPU in Linux DRM — fine  

### 9. Save

- **F10 → Save & Reset**

---

## PCIe deep dive (why it looked slow)

Measured on this machine (BIOS 2703, idle desktop). Two separate issues:

### A. Idle Gen1 — usually normal

| Field | Idle value | Meaning |
|-------|------------|---------|
| GPU pstate | P8 ~13 W | Card asleep |
| Current link | **Gen1 (2.5 GT/s) ×8** | NVIDIA power management often drops **speed** when idle |
| ASPM on GPU link | Disabled | Not the cause |

**Do not judge PCIe by idle `nvidia-smi`.** Put a CUDA/llama/game load on the GPU, then re-check gen/width.

### B. Hard caps on the CPU root port — real problem

`lspci` on **CPU PEG** `00:01.0` (not the card):

```text
LnkCap:  Speed 8GT/s, Width x8
LnkCap2: Supported Link Speeds: 2.5-8GT/s    # host will not train above Gen3
LnkCtl2: Target Link Speed: 8GT/s
LnkSta:  Speed 2.5GT/s, Width x8
```

GPU `01:00.0`:

```text
LnkSta: Speed 2.5GT/s (downgraded), Width x8 (downgraded)
LnkCtl2: Target Link Speed: 8GT/s
nvidia-smi: pcie.link.gen.max = 3, width.max = 16
```

| Cap | Expected on 12900K + Z690-E + 4090 | What we saw |
|-----|-------------------------------------|-------------|
| Max generation | Gen4+ on PEG | **Pre-update: Gen3 only** → **Post-4505: Gen4 under load (fixed)** |
| Max width | **×16** | Root port **LnkCap Width x8** (still after update) |
| Idle current speed | Gen1 OK | Gen1 (expected idle) |

**Update 2026-07-12:** After BIOS 4505 + user PCI changes, host max is **Gen4** and CUDA load shows **16 GT/s ×8**. Width remains **x8** — still hunt bifurcation / M.2 lane share for full x16.

### Likely BIOS causes

| Symptom | Likely setting / cause |
|---------|-------------------------|
| Host max Gen3 | PCIEX16_1 speed forced to **Gen3**; or conservative Auto on old BIOS |
| Max width x8 | **Bifurcation x8x8**, M.2 lane steal, second device, riser, damaged contact |
| Idle Gen1 | Normal P-state power save |

### Bandwidth sketch (theoretical, one direction)

| Link | ~GB/s |
|------|-------|
| Gen4 ×16 (healthy 4090 desktop) | ~32 |
| **Gen3 ×8 (your configured max)** | **~8** |
| Gen1 ×8 (idle) | ~2 |

Full-VRAM local LLMs care less; **model load / host↔GPU copies** care more. NVMe path was fine (**Gen4 ×4** under VMD).

### Linux cannot raise these caps

- `pcie_aspm` was not the limiter (ASPM disabled on this link).  
- Kernel cmdline will not turn Gen3×8 into Gen4×16.  
- Fix in **BIOS** (and hardware seating / M.2 layout).  

### After-fix verification (use under GPU load)

```fish
# Under load (llama/cuda/game), not idle:
nvidia-smi --query-gpu=pcie.link.gen.current,pcie.link.gen.max,pcie.link.width.current,pcie.link.width.max,pstate --format=csv

# Root port capabilities (max should move toward Gen4+ and x16)
sudo lspci -vv -s 00:01.0 | rg "LnkCap:|LnkCap2:|LnkSta:|LnkCtl2:"

# GPU side
sudo lspci -vv -s 01:00.0 | rg "LnkCap:|LnkSta:|LnkCtl2:"
```

**Success criteria (under load):**

- `pcie.link.gen.max` ≥ **4** (or at least gen current = 3–4, not stuck at host max 3 forever after BIOS fix)  
- `pcie.link.width.current` / max → **16**  
- Root port `LnkCap` shows **Width x16** and speed support **beyond 8GT/s** if Auto/Gen4 worked  

If still x8 after BIOS x16: reseat GPU, try another top-slot seat, remove riser, free M.2 that shares PEG lanes.

---

## After reboot (full Linux checks)

```fish
# BIOS version
sudo dmidecode -t bios | rg "Vendor|Version|Release"

# RAM — want 6000 MT/s
sudo dmidecode -t memory | rg "Configured Memory Speed|Part Number|Size:"

# PCIe GPU — run again under load
nvidia-smi --query-gpu=pcie.link.gen.current,pcie.link.gen.max,pcie.link.width.current,pcie.link.width.max,pstate --format=csv

# Boot / Secure Boot
bootctl status | head -20
efibootmgr
```

Optional re-baseline cargo after XMP only (compare to `bench/20260712-baseline-cachyos/`):

```fish
# same conditions as baseline: EPP performance, CARGO_INCREMENTAL=0, no RUSTFLAGS
/home/imma/projects/kernel/scripts/bench-rust.fish /home/imma/projects/combly xmp-after
```

---

## Do not change (unless you know why)

| Setting | Why |
|---------|-----|
| VMD off / RAID→AHCI surprise | Root disk can disappear |
| Secure Boot on | Breaks unsigned custom kernels |
| Extreme manual BCLK/CPU OC for daily | Stability; XMP + MCE first |
| Disable HT | Hurts `cargo -j24` |
| Disable all C-states daily | Power/thermals; short benches only |
| Random PCIe bifurcation experiments | Can lock GPU to x8 |

---

## Minimum path (biggest wins only)

1. Flash **4505** (done)  
2. Memory **≥5600** stable (done — 6000 not stable on 4 DIMMs)  
3. **PCIEX16 Auto/Gen4**, leave M.2_1 empty for GPU ×16 if desired  
4. Verify RAM + PCIe under load  

## Memory stability (this kit)

| Setting | Result on 4×16 GB F5-6000 |
|---------|---------------------------|
| 4000 JEDEC | Stable, slow |
| **5600** | **Stable daily driver (current)** |
| 6000 XMP | Lockups — do not use until manually tuned | 

---

## Linux vs BIOS responsibility

| Goal | Where |
|------|--------|
| DRAM 6000 (XMP) | BIOS |
| PCIe Gen/width / ReBAR | BIOS |
| Secure Boot, VMD, boot devices | BIOS |
| Flash CAP | Flashback / EZ Flash (not fwupd on this board) |
| CPU governor / EPP | Linux |
| Boot order among Linux entries | systemd-boot / `efibootmgr` |
| Custom kernel | Later — keep `linux-cachyos` as menu fallback |

---

## Related docs

- [BIOS-UPDATES.md](BIOS-UPDATES.md) — 4505 package, SHA-256, version changelogs, what is actually “perf”  
- [DECISIONS-AND-BIOS.md](DECISIONS-AND-BIOS.md) — dual-boot plan, hard-nos, strategy  
- [VERIFICATION.md](VERIFICATION.md) — baseline benches  
- [OPTIMIZED-KERNEL.md](OPTIMIZED-KERNEL.md) — custom kernel plan  
