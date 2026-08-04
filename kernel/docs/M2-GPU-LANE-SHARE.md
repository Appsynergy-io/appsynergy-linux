# M.2 vs GPU lanes — ROG STRIX Z690-E + your SSD

## Short answer

**Yes — if the boot SSD is in M.2_1, that alone forces the GPU slot to ×8.**  
Your measured PEG root port (`LnkCap Width x8`) matches that rule exactly.

Official ASUS wording (board manual / B&H-hosted manual text for this model):

> **When M.2_1 is occupied with SSD, PCIEX16(G5) will run x8 mode only.**

Product expansion slot line also says the top GPU slot is **PCIe 5.0 x16 (x16 or x8) [CPU]**.

---

## What you have now

| Piece | Evidence |
|-------|----------|
| GPU | `00:01.0` → `01:00.0` RTX 4090 — **max width x8**, Gen4 under load |
| Boot SSD | **WD_BLACK SN750 SE 500G** (`nvme0n1`) — only NVMe |
| SSD path | Under **VMD**: `00:0e.0` → `10000:e1:00.0` |
| Physical slot | **Not reported by Linux** — must look inside the case |

VMD only remaps how firmware exposes NVMe; it does **not** change the mechanical rule that **M.2_1 steals half of the CPU PEG lanes**.

---

## Slot map (this board)

Onboard M.2 (simplified):

| Slot | Typical location | Link | Shares with GPU PCIEX16(G5)? |
|------|------------------|------|--------------------------------|
| **M.2_1** | Top, near CPU / under large heatsink (PCIe **5.0** x4) | CPU | **Yes — GPU becomes ×8** |
| **M.2_2** | Mid (PCIe **4.0** x4) | CPU-side per ASUS 12th-gen notes | Usually **no** GPU x16 steal (confirm silk-screen / manual diagram) |
| **M.2_3** | Lower (PCIe **4.0** x4 **or SATA**) | **Chipset (Z690)** | **No** GPU lane steal |

Also:

- Bundled **ROG Hyper M.2** card in a PCIe slot has its own rules; **do not** put it in **PCIEX16(G5)** if you want a full-width GPU there.
- Second long slots are chipset **x4**, not full GPU x16.

Your SN750 SE is **PCIe 4.0** — it does **not** need M.2_1’s Gen5 socket. Putting it in M.2_1 only costs GPU lanes.

---

## Recommended layout for Gen4 ×16 GPU

1. **Boot SSD → M.2_3** (chipset) or **M.2_2** if free and manual confirms no share.  
   Prefer **M.2_3** for “never steal GPU lanes.”  
2. **Leave M.2_1 empty** (or use only if you accept GPU ×8 forever).  
3. GPU stays in **top PCIEX16(G5)** only.

After move + reboot, under CUDA load expect:

```text
nvidia-smi: gen 4, width 16
lspci 00:01.0 LnkCap: Width x16
```

---

## How to identify the physical slot (case open)

Power off, open side panel:

| Clue | Likely slot |
|------|-------------|
| SSD under the **top** M.2 armor / closest to CPU socket & DIMMs | **M.2_1** ← move this |
| Between GPU and bottom of board, smaller heatsink | Often **M.2_2** or **M.2_3** |
| Silk-screen on PCB | Look for **M.2_1** / **M.2_2** / **M.2_3** printed near the connector |

Photo of the manual layout (memory of typical Z690-E): M.2_1 is the vertical/top CPU Gen5 socket under the large heatsink near the CPU.

---

## Move procedure (safe for your dual-boot + LUKS)

This is the **same disk** (Windows + Linux partitions). You only change **which socket** it plugs into — no reinstall if firmware still finds NVMe under VMD.

1. Confirm backups of anything critical (always).  
2. Power off, PSU switch off, ground yourself.  
3. Note current screw/heatsink orientation.  
4. Move SSD **from M.2_1 → M.2_3** (or M.2_2).  
5. Leave M.2_1 empty.  
6. Boot Linux. If it fails to find root, enter BIOS and confirm VMD still enabled and the drive is listed.  
7. Verify:

```fish
# under GPU load (small CUDA app or llama)
nvidia-smi --query-gpu=pcie.link.gen.current,pcie.link.gen.max,pcie.link.width.current,pcie.link.width.max,pstate --format=csv
sudo lspci -vv -s 00:01.0 | rg "LnkCap:|LnkSta:"
```

**Success:** `width.current` / `LnkCap Width` → **16**.  
**Unchanged x8:** wrong slot still populated, bifurcation still on, or riser — recheck silk-screen.

---

## Tradeoffs

| Choice | GPU | SSD |
|--------|-----|-----|
| SSD in **M.2_1** (likely now) | **×8** Gen4 | Gen4 x4 on Gen5 socket (wasted for this drive) |
| SSD in **M.2_3** (chipset) | **×16** Gen4 | Gen4 x4 via chipset — still far above SATA; fine for OS + cargo |
| SSD in M.2_1 + accept ×8 | Half GPU host bandwidth | Slightly lower latency path to CPU (minor for OS disk) |

For **Rust** on this box, DRAM 6000 already dominates. For **LLM load/offload**, **GPU ×16** is the clearer win.

---

## Related

- [BIOS-POST-UPDATE-2026-07-12.md](BIOS-POST-UPDATE-2026-07-12.md) — Gen4×8 after BIOS update  
- [BIOS-CHECKLIST.md](BIOS-CHECKLIST.md) — PCIe deep dive  
