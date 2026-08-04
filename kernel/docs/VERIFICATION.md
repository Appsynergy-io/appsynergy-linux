# Verification report — mistakes fixed, baseline recorded, strip candidates

Date: 2026-07-12  
Host: CachyOS `7.1.2-3-cachyos`, i9-12900K, ASUS Z690, RTX 4090  

---

## 1. Mistakes found in the first plan (and fixes)

| # | Claim / setting | Reality | Fix |
|---|-----------------|---------|-----|
| 1 | `CONFIG_SND_SOC_INTEL_SOF=y` | **Invalid symbol**. Real tree uses `CONFIG_SND_SOC_SOF_*` / `CONFIG_SND_SOC_SOF_INTEL_TOPLEVEL` | Fragment rewritten with real SOF symbols |
| 2 | Keep/use `NO_HZ_FULL` for perf | Enabled in Cachy, but **cmdline has no `nohz_full=` / `isolcpus=`** → feature idle, extra complexity | Prefer `NO_HZ_IDLE` for this desktop |
| 3 | Dropping AMD DRM only | Correct that no AMD GPU; also **no iGPU in `/sys/class/drm`** (only NVIDIA `0x10de`) | Safe to disable `i915`/`xe`/`amdgpu`/`nouveau` |
| 4 | `md`/`raid1` needed? | Modules loaded, **`/proc/mdstat` empty**, no `/dev/md*` | Safe to disable `CONFIG_MD` |
| 5 | `dm_verity` on root path | Loaded with **use count 0**; root is `dm-crypt` only | Keep as `m` optional; not boot-critical |
| 6 | `NR_CPUS=8192` | Real hardware **24** threads | `NR_CPUS=64` |
| 7 | `GENERIC_CPU` + x86-64-v3 | Confirmed not native | `X86_NATIVE_CPU=y` (symbol valid in tree) |
| 8 | Active LSMs include SELinux etc. | `/sys/kernel/security/lsm` = `capability,landlock,lockdown,yama,bpf` | Drop SELinux/Smack/Tomoyo/AppArmor from build |
| 9 | Strip SOF entirely | **SOF Intel TGL stack is loaded** for board HDA even when playback is USB + NVIDIA HDMI | Keep Intel SOF; drop AMD SOF only |
| 10 | “Kernel strip will dominate cargo” | Cargo is **user-bound** (sys time ~8–15% of user time in baseline) | Expect small wall-time deltas from kernel; larger from userspace toolchain |
| 11 | `context-switches` in first perf recipe | Hybrid PMU reported `context-switches:u` = **0** (user-only; switches are kernel) | After runs: use kernel+user events / `perf stat -a` carefully; wall_sec + cache miss rates remain primary |
| 12 | `NUMA_BALANCING` on 1 node | Single socket; balancing is wasted work | Disable NUMA balancing; keep `NUMA=y`, `NODES_SHIFT=3` |
| 13 | `/boot` space | **92 MiB free**, 92% full | Must remove LTS (or other) **before** installing custom kernel |
| 14 | Over-aggressive `localmodconfig` | Would drop idle-but-needed paths (Wi-Fi soft-block, SATA empty, rescue FS) | Prefer fragment + manual review; localmod only with keep-list |

Corrected fragment: `configs/rustopt.fragment` (rewritten).

---

## 2. Component keep/strip re-check (hardware ground truth)

### Must keep (boot or daily)

| Component | Evidence |
|-----------|----------|
| VMD + NVMe | `lspci` VMD driver `vmd`; root disk `nvme` under VMD domain |
| dm-crypt + aesni | LUKS mapper root; `dm_crypt` use=1 |
| btrfs | `findmnt /` → btrfs |
| vfat | `/boot` ESP |
| igc | `enp6s0` UP |
| iwlwifi | `wlan0` present (soft-blocked); keep as module |
| BT (btusb/intel) | BT powered on |
| xHCI + hid_apple + generic HID | Apple keyboard, G502 |
| snd_usb_audio + snd_hda + Intel SOF | USB ASUS audio primary; NVIDIA HDMI; SOF modules loaded |
| nvidia (out-of-tree) | DRM card1 only GPU |
| asus_wmi / armoury / ec_sensors | Loaded |
| zram | Active swap |
| nf_tables, bridge, veth, overlay, wireguard, tun | Docker active |
| kvm_intel | Module loaded; qemu-base installed |
| ntsync | modules-load.d enabled (Steam) |

### Safe kernel strip (real code size / I-cache)

| Strip | Why safe | Benefit type |
|-------|----------|--------------|
| AMDGPU/Radeon/Nouveau/i915/Xe | No matching hardware in DRM | Kernel image + modules smaller |
| Xen + Hyper-V guest stacks | Never used | Built-in Xen was `=y` — real text savings |
| `CONFIG_MD` / raid personalities | No arrays | Avoids md_mod/raid1 autoload |
| Unused FS (XFS/F2FS/JFS/…) | Not mounted; root btrfs | Module tree / rare build-in |
| SELinux/Smack/Tomoyo/AppArmor | Not in active LSM list | Fewer security hooks |
| DWARF debug, heavy ftrace, KFENCE | Not debugging kernel daily | Much smaller vmlinux |
| `NR_CPUS` 8192→64 | 24 CPUs | Per-CPU data footprint |
| AMD CPU_SUP / amd_pstate / AMD SOF | Intel desktop | Dead code |
| Hundreds of unused NIC/Wi-Fi vendors | Only igc+iwlwifi | Module dir size (initramfs if pulled) |

### Do **not** strip “for speed” (common traps)

- **Mitigations off** — daily driver; Alder Lake already cheap on major ones  
- **dm-crypt / AES** — root is encrypted  
- **btrfs** — root  
- **Intel SOF** — loaded and tied to board audio  
- **AHCI** — controllers present; empty today, needed if you attach SATA  
- **KVM** — you have qemu  
- **Docker net stack** — docker active  
- **Wi-Fi modules** — hardware present even if soft-blocked  
- **Calculator / Plasma apps** — irrelevant to kernel; leave alone  

### Benefit honesty

| Change | Likely effect on cargo wall time | Likely effect elsewhere |
|--------|----------------------------------|-------------------------|
| `X86_NATIVE_CPU` | Small (syscalls/crypto/sched) | Kernel IPC |
| Strip dead drivers/LSMs | Very small | Latency under load, module dir, boot |
| AutoFDO of kernel | Small–moderate on hot kernel paths | Same |
| tmpfs `CARGO_TARGET_DIR` + mold + `-C target-cpu=native` | **Large** | Userspace only |
| Free `/boot` + drop LTS | None until custom kernel fits | Enables install |

---

## 3. Baseline performance (recorded)

**Dir:** `bench/20260712-baseline-cachyos/`  
**Conditions:** `epp=performance`, `governor=performance`, `CARGO_INCREMENTAL=0`, no `RUSTC_WRAPPER`, no `RUSTFLAGS`, ananicy-cpp **active**, THP=always, rustc/cargo 1.96.1, perf 7.1.1  

### Wall clock + CPU time

| Workload | wall_sec | user_sec | sys_sec | maxrss |
|----------|----------|----------|---------|--------|
| combly `cargo build --release -j24` | **116.72** | 172.46 | 15.28 | 1.54 GiB |
| combly `cargo check -j24` | **20.54** | 27.90 | 13.93 | 0.86 GiB |
| beetv-rs `cargo build --release -j24` | **78.16** | 170.43 | 15.51 | 1.48 GiB |

Sys/user ratio ≈ **8–15%** → most time is userspace rustc/LLVM; kernel tuning alone cannot give 2×.

### Cache (hybrid PMU: sum atom + core)

Derived from `perf-*-release.txt` (userspace counters):

| Workload | cache-refs (atom+core) | cache-misses (atom+core) | miss rate |
|----------|------------------------|--------------------------|-----------|
| combly-release | 20.21e9 | 4.93e9 | **~24.4%** |
| combly-check | 2.61e9 | 0.86e9 | **~32.9%** |
| beetv-release | 19.59e9 | 4.79e9 | **~24.5%** |

(Alder Lake splits `cpu_atom/*` and `cpu_core/*`; rates are approximate because of multiplexing %.)

### Microbenches

| Bench | Result |
|-------|--------|
| `perf bench sched messaging -g20 -l5000` | **2.544 s** |
| `perf bench mem memcpy` glibc 512MB | **21.24 GB/s** |

### Kernel footprint (before)

| Item | Size |
|------|------|
| vmlinuz | 17 MiB |
| `/usr/lib/modules/7.1.2-3-cachyos` | 359 MiB |
| `.ko*` count | 6470 |

### How to quantify after

Re-run the same script with **identical**:

- EPP `performance`
- `CARGO_INCREMENTAL=0`
- empty RUSTFLAGS / no sccache wrapper (or same wrapper both times)
- same project commits
- `cargo clean` before each timed run
- 2–3 repeats; report median wall_sec

```fish
# After new kernel boots:
set -x LABEL rustopt-(uname -r)
# copy method from scripts/bench-rust.fish or re-run the baseline recipe
# Compare wall_sec and cache miss rate (sum atom+core) to this file.
```

**Primary success metrics**

1. `wall_sec` median (combly-release, beetv-release)  
2. cache miss rate (misses/refs)  
3. `perf bench sched messaging` time (scheduler path)  
4. Subjective: Plasma drag under `-j24` cargo  

**Ignore for kernel A/B:** pure disk package size, browser count.

---

## 4. Software strip candidates (userspace)

Rule: remove **unused stacks**, not everyday utilities (kcalc, dolphin, etc.).  
Split by **what it actually buys**.

### A. Enables custom kernel + frees RAM/disk (do first)

| Package | Size | Why removable | Benefit |
|---------|------|---------------|---------|
| `linux-cachyos-lts` + headers + `linux-cachyos-lts-nvidia-open` | ~308 MiB + **boot images** | You boot `linux-cachyos` only | **Frees `/boot` (~92 MiB free now)** — required before second kernel |
| pacman orphans: `electron37`, `electron39`, `llvm21-libs`, … | ~600 MiB+ | `pacman -Qtdq` | Disk; less clutter |

```fish
# Review then remove LTS if you accept single rolling kernel + this custom later
pacman -Rns linux-cachyos-lts linux-cachyos-lts-headers linux-cachyos-lts-nvidia-open
pacman -Qtdq | sudo pacman -Rns -
```

### B. Firmware you never load (disk + smaller initramfs if included)

Keep: `linux-firmware-intel`, `linux-firmware-nvidia`, iwl/AX210 pieces (under intel/iwlwifi).  
Candidates (no matching hardware):

| Package | ~Size |
|---------|-------|
| linux-firmware-amdgpu | 27 MiB |
| linux-firmware-radeon | 2 MiB |
| linux-firmware-atheros | 51 MiB |
| linux-firmware-broadcom | 13 MiB |
| linux-firmware-mediatek | 37 MiB |
| linux-firmware-realtek | 7 MiB |
| linux-firmware-cirrus | 3 MiB |
| linux-firmware-other | 30 MiB |

~**170 MiB** firmware. **No cargo speedup**; helps disk and optional initramfs size.

### C. FS / RAID tools for filesystems you do not use

| Package | Notes |
|---------|-------|
| jfsutils, f2fs-tools, nilfs-utils | No such FS mounted |
| xfsprogs | Optional keep for rescue USB |
| mdadm, dmraid | No md/dmraid arrays |
| nfs-utils | Only if you never use NFS |

**No runtime CPU win** unless something autoloads (md modules currently do — kernel config strip is the real fix).

### D. Network stack duplicates / mobile leftovers

| Package | Notes |
|---------|-------|
| modemmanager | Desktop, no modem — safe if you do not use WWAN |
| netctl | NetworkManager is active — netctl unused |
| ntp | timesyncd already enabled — pick one |
| iwd **or** wpa_supplicant | Wi-Fi soft-blocked; NM may use one — check before remove |

### E. Large optional stacks — confirm before purge

| Package | Size | Keep if… |
|---------|------|----------|
| **cuda** | **4.7 GiB** | GPU compute / llama / torch |
| **mingw-w64-gcc** | **1.2 GiB** | Windows cross builds |
| **jdk17-openjdk** | 426 MiB | Java projects |
| thorium + brave + firefox | ~1.4 GiB | Keep **one** daily browser |
| scx-scheds (inactive) | 157 MiB | Keep if you will try scx; else optional |
| qemu-base | tiny meta | VMs |
| cni-plugins / nerdctl | Docker-adjacent | Docker present — careful |
| CJK font packs (multiple) | ~400 MiB+ | Keep one set if you need CJK |

### F. Does **not** help strip for compile perf

- Plasma/KDE apps, kcalc, kate, etc.  
- ananicy-cpp: **no cargo/rustc rules**; does not deprioritize compiles. Leave for desktop games/UI.  
- Removing browsers while idle saves almost nothing on cargo wall time.

### G. Userspace changes that **do** help cargo (not removal)

```fish
# mold not installed — big link-time win for large crates
sudo pacman -S --needed mold

set -Ux RUSTFLAGS "-C target-cpu=native -C link-arg=-fuse-ld=mold"
set -Ux CARGO_TARGET_DIR /tmp/cargo-target-$USER
# sccache exists at ~/.cargo/bin/sccache but RUSTC_WRAPPER was unset in baseline
# set -Ux RUSTC_WRAPPER sccache   # only for iterative work, not A/B kernel benches
```

---

## 5. Recommended order of operations

1. **Record baseline** — done (`bench/20260712-baseline-cachyos`).  
2. **Userspace toolchain** (mold + native RUSTFLAGS + tmpfs target) → re-bench combly-release (expect largest gain).  
3. **Remove LTS kernel** → free `/boot`.  
4. **Optional:** unused firmware + orphans + confirmed-unused tools (modemmanager, netctl, mdadm, jfs/f2fs/nilfs).  
5. **Build custom kernel** with corrected `rustopt.fragment` only after step 3.  
6. **Re-run same bench recipe** → fill after columns; compute %Δ wall_sec and miss rate.

---

## 6. Before / after table (fill after rebuild)

| Metric | Baseline (Cachy 7.1.2-3) | After | Δ% |
|--------|--------------------------|-------|-----|
| combly-release wall_sec | 116.72 | | |
| combly-check wall_sec | 20.54 | | |
| beetv-release wall_sec | 78.16 | | |
| combly-release cache miss rate | ~24.4% | | |
| beetv-release cache miss rate | ~24.5% | | |
| sched messaging | 2.544 s | | |
| memcpy glibc | 21.24 GB/s | | |
| vmlinuz size | 17 MiB | | |
| modules tree | 359 MiB / 6470 ko | | |

---

## 7. Still confirm with you before destructive removals

1. Remove **LTS kernel** pack? (needed for `/boot`)  
2. Remove **cuda** (4.7G) / **mingw** / **jdk17**?  
3. Keep **one** of thorium/brave/firefox?  
4. Is **modemmanager** / **nfs-utils** ever used?  
5. Primary PGO training trees still combly + beetv-rs + appsynergy-rs?
