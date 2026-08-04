# ROG STRIX Z690-E GAMING WIFI — BIOS updates & performance notes

Source: ASUS official API `GetPDBIOS` (global), retrieved 2026-07-12.  
Your current BIOS: **2703** (2023-08).  
**Latest: 4505** (2025-12-15).

Support page: https://rog.asus.com/motherboards/rog-strix/rog-strix-z690-e-gaming-wifi-model/helpdesk_bios/

---

## Latest package (downloaded here)

| Field | Value |
|-------|--------|
| Version | **4505** |
| Date | 2025/12/15 |
| Size | 13.02 MB |
| SHA-256 | `8B61323171D455C0E97A8BA247F1C9F92786A7CFB8F293EC4A78F0382BB9DF6D` |
| Local zip | `firmware/ROG-STRIX-Z690-E-GAMING-WIFI-ASUS-4505.zip` |
| Official URL | https://dlcdnets.asus.com/pub/ASUS/mb/BIOS/ROG-STRIX-Z690-E-GAMING-WIFI-ASUS-4505.zip |

**4505 changelog (official):**

1. Enhance overall system security.  
2. Update the ME Firmware version to **16.1.38.2676**.  

ME stays updated even if you roll BIOS back later.  
Flashback CAP name: rename with **BIOSRenamer** → typically `SZ690E.CAP`.

---

## Is there performance in the updates (2703 → 4505)?

Short answer: **yes, but not a free “12th gen turbo” win in the latest file alone.**  
Most post-2703 notes target **13th/14th gen stability (Vmin Shift)** and **security/ME**. For your **12900K**, the useful bits are memory/PCIe/profile options and a few explicit performance lines.

### Explicitly performance / stability relevant (after your 2703)

| Version | Date | Perf-related? | Official notes (summary) |
|---------|------|---------------|---------------------------|
| **4101** | 2025-01-14 | **Yes — named** | **“Enhanced system performance, stability”** and allowed **C1E** to be disabled |
| **4001** | 2024-10-30 | Stability / power | Microcode **0x12B** (idle/light elevated voltage / Vmin Shift). **Removed option to disable C1E** (forces C1E on per Intel) — *then 4101 re-allows disable* |
| **4301** | 2025-05-26 | Stability (mainly 13/14) | Microcode **0x12F** Vmin Shift for 13th/14th gen |
| **4505** | 2025-12-15 | Security / ME only | Security + ME 16.1.38.2676 — **no stated perf boost** |
| **3802** | 2024-08-09 | Defaults / 13–14 | Microcode **0x129**; factory defaults → **Intel Default Settings** |
| **3701** | 2024-07-17 | Spec compliance | Microcode **0x125** eTVB within Intel specs |
| **3603** | 2024-05-31 | **Profiles** | **Performance Preferences**: Intel Default (Performance/Extreme) vs **ASUS Advanced OC Profile**; new defaults per Intel guidance |
| **3501** | 2024-04-24 | Stability profile | **Intel Baseline Profile** (lower power / stability) |
| **3401** | 2024-03-22 | **Memory** | **Improved DDR5 compatibility**; CEP when disabled optimized |
| **3302** | 2024-02-22 | Perf option (14th non-K) | Microcode **0x123**; CEP disable for perf (power/thermals up); 256GB memory compatibility |
| **3101** | 2023-12-29 | Stability + PCIe | Stability; LogoFAIL; **PCIe Bifurcation options** for GPU + M.2 |
| **2802** | 2023-10-06 | 14th gen support | Microcode/ME for 14th gen; stability |

### Older notes (before / around 2703) that mentioned perf

| Version | Notes |
|---------|--------|
| 2403 | Improve system performance; improve memory compatibility |
| 2305 | Improve system performance and security; DRAM stability |
| 2103 | OC stability + microcode for 12th/13th |
| 2004 | Improve system performance |
| 1403 | Improve performance for **12900KS** |
| 1003 / 0811 / 0803 / 0702 | Various “improve performance”, DRAM, microcode |

### What this means for *your* 12900K + 6000 kit

1. **4505 alone** will not magically raise clocks; it’s security + newer ME. Still worth installing if you update.  
2. Path **2703 → 4505** still brings: **DDR5 compatibility work (3401)**, **performance preference profiles (3603)**, **C1E control + “enhanced performance/stability” (4101)**, PCIe bifurcation options (3101), many microcode/ME refreshes.  
3. Your big win remains **XMP 6000**, not the BIOS zip text. After flash, **re-enable XMP** and re-check PCIe/ReBAR.  
4. **Intel Default Settings** (from 3603+) can *limit* multi-core turbo vs older “ASUS multi-core enhancement” style defaults. After update, for max compile/LLM host throughput pick **ASUS Advanced OC Profile** / MCE “remove limits” only if thermals allow — not the restrictive Baseline profile.

---

## How to flash (safe)

1. Prefer **USB BIOS Flashback** (board powered, USB on Flashback port, file renamed with **BIOSRenamer** to `SZ690E.CAP`).  
   Or EZ Flash 3 from within BIOS (internet or USB).  
2. Keep AC power connected the whole time.  
3. After flash: load defaults once if needed, then re-apply:
   - XMP → 6000  
   - ReBAR + Above 4G  
   - PCIe x16 Auto/Gen4  
   - Secure Boot off  
   - Boot order (Linux Boot Manager / Windows)  
   - Do **not** flip VMD  
4. Boot Linux and verify:

```fish
sudo dmidecode -t bios | rg "Version|Release"
sudo dmidecode -t memory | rg "Configured Memory Speed"
```

---

## Full changelogs (2703 → 4505)

### 4505 (2025/12/15)
1. Enhance overall system security.  
2. Update ME to 16.1.38.2676.

### 4301 (2025/05/26)
1. Intel microcode **0x12F** — Vmin Shift conditions (13th/14th gen).  
ME → 16.1.35.2557.

### 4101 (2025/01/14)
1. **Enhanced system performance, stability** and allowed **C1E** to be disabled.  
ME → 16.1.32.2473.

### 4001 (2024/10/30)
1. Microcode **0x12B** — elevated voltage at idle/light load / Vmin Shift.  
2. Option to disable C1E **removed** (C1E stays enabled per Intel).  
ME → 16.1.32.2473.

### 3802 (2024/08/09)
1. Microcode **0x129** — 13th/14th stability.  
2. Factory defaults = **Intel Default Settings** (incl. non-K).  
ME → 16.1.30.2307.

### 3701 (2024/07/17)
Microcode **0x125** — eTVB within Intel specs.  
ME → 16.1.30.2307.

### 3603 (2024/05/31)
1. **Performance Preferences**: Intel Default (Performance/Extreme) vs **ASUS Advanced OC Profile**.  
2. Redefine factory defaults from Intel guidance.  
3. F5 = Reset to Defaults.  
4. Warnings when leaving defaults.

### 3501 (2024/04/24)
**Intel Baseline Profile** — lower power limits / stability.

### 3401 (2024/03/22)
1. **Improved DDR5 compatibility**.  
2. Further optimized CEP when disabled.

### 3302 (2024/02/22)
1. Microcode **0x123** — CEP disable for perf on 14th non-K.  
2. Higher power/temp if CEP off.  
3. Enhanced 256GB memory compatibility.

### 3101 (2023/12/29)
1. Improve system stability.  
2. ME 16.1.30.2307.  
3. LogoFAIL patch.  
4. **PCIe Bifurcation** for GPU with M.2 storage.

### 2802 (2023/10/06)
14th gen microcode/ME; improve stability.

### 2703 (2023/08/17) — your baseline
Security mitigation; improve stability. ME 16.1.27.2176.

---

## Related

- [BIOS-CHECKLIST.md](BIOS-CHECKLIST.md) — full post-flash checklist, **PCIe deep dive** (Gen3×8 root cause), Linux verify commands  
- [DECISIONS-AND-BIOS.md](DECISIONS-AND-BIOS.md) — dual-boot, hard-nos, strategy  
 
