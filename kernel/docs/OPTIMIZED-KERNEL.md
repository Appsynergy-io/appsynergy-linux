# Custom kernel: i9-12900K desktop, Rust-first, beat CachyOS

Baseline: CachyOS `linux-cachyos` **7.1.2-3**, Clang ThinLTO, O3, AutoFDO-ready, PREEMPT_DYNAMIC, HZ=1000, THP=always, `sched-ext` available.

Target: single-machine kernel for **ASUS Z690 + i9-12900K (Alder Lake hybrid) + RTX 4090 + NVMe LUKS/btrfs + KDE Plasma**, optimized for **cargo/rustc** and desktop latency.

---

## 0. System profile (scanned)

| Item | Value |
|------|--------|
| CPU | Intel Core i9-12900K (8P+8E, 24 threads), microcode 0x3e |
| Caches | L1d 48K×16, L1i 32K×16, L2 1.25M (P) / shared E-cluster, **L3 30 MiB shared** |
| RAM | 64 GiB, NUMA=1, THP=`always`, zram-zstd swap 62.5G |
| GPU | NVIDIA AD102 RTX 4090 (proprietary `nvidia` 610.x) |
| Chipset | ASUS Z690, VMD → NVMe WD_BLACK SN750 SE 500G |
| Net | Intel I225-V (`igc`) up; AX210 Wi-Fi soft-blocked; BT on |
| Storage FS | LUKS → btrfs subvols; EFI vfat `/boot` (**91% full ~91M free**); Windows NTFS present, not mounted |
| DE | KDE Plasma 6.7.2 |
| Kernel now | 7.1.2-3-cachyos, `GENERIC_CPU` + **x86-64-v3** (not native), NR_CPUS=8192 |
| Workloads | Docker+containerd, WireGuard modules, Steam, QEMU, many Rust crates under `~/projects` |
| LSMs active | capability,landlock,lockdown,yama,bpf (SELinux/AppArmor/Smack/Tomoyo **compiled but unused**) |
| scx | packages installed, service **inactive** |
| CPU governor | intel_pstate + powersave / EPP balance_performance |

**Boot constraint:** `/boot` has ~91 MiB free. A second vmlinuz+initramfs will not fit. Free space (remove LTS kernel images, old initramfs) before installing a custom kernel, or put kernels on a larger ESP.

---

## 1. What can be stripped (hardware-justified)

### Keep (required for this box)

| Area | Why |
|------|-----|
| NVMe + VMD + AHCI | Boot disk under VMD; ASM1061 + chipset SATA present |
| dm-crypt, dm-mod, encrypted_keys, trusted/tee | Root is LUKS |
| btrfs, vfat, tmpfs, fuse, overlay, loop | Root, ESP, Flatpak/Docker, temp |
| ntfs3 (module) | Dual-boot Windows partitions on same NVMe |
| igc, iwlwifi/iwlmvm, bluetooth stack | Ethernet + Wi-Fi + BT hardware present |
| xhci, hid (hid_apple, generic), snd_hda, snd_usb_audio, SOF Intel TGL path | Keyboard, mouse, ASUS USB audio, HDMI/DP audio, board HDA |
| asus_wmi, asus_armoury, asus_ec_sensors, eeepc_wmi | Board control / sensors |
| nvidia (out-of-tree) + drm/ttm helpers | Daily GPU; build with matching headers |
| kvm + kvm_intel | Optional VMs; small; keep as modules |
| nf_tables, bridge, veth, wireguard, tun | Docker + VPN |
| zram, zstd/lz4 backends | Active swap |
| intel_pstate, RAPL, coretemp, mei | Power/thermal on Alder Lake |
| ntsync | Steam/Wine path already loads it |
| BPF JIT, io_uring, landlock, yama, seccomp | Used by modern userspace / containers |

### Strip aggressively (dead weight → smaller text, better I-cache)

These ship in CachyOS generic configs and do nothing on this machine:

| Category | Examples | Justification |
|----------|----------|---------------|
| **AMD GPU/CPU paths** | `DRM_AMDGPU`, `DRM_RADEON`, full AMD pstate guests of concern, `AMD_MEM_ENCRYPT` runtime paths, Hygon/Centaur/Zhaoxin CPU_SUP | Intel-only desktop |
| **Intel iGPU** | `DRM_I915`, `DRM_XE` | No iGPU display in use; discrete 4090 only (disable if you never use iGPU/QuickSync) |
| **Nouveau** | `DRM_NOUVEAU` | Using proprietary NVIDIA |
| **Virt host clutter** | Xen PV/Dom0 full stack, Hyper-V, VirtualBox guest, VMware balloon if not used | You host QEMU/KVM only; keep `KVM`/`KVM_INTEL`, drop Xen host |
| **Filesystems unused** | JFS, XFS, GFS2, OCFS2, NILFS2, F2FS, ZoneFS, GFS, Ceph, CIFS/SMB client if unused, NFS* if unused, ISO/UDF optional→m, exFAT if unused | Root is btrfs; Windows = ntfs3 only |
| **Net drivers (~hundreds)** | Realtek, Broadcom, Atheros Wi-Fi, e1000e-era, Mellanox, Chelsio, virtually all USB ethernet, CAN, hamradio, Appletalk | Only `igc` + `iwlwifi` |
| **SCSI/RAID megadrivers** | MegaRAID, AACRAID, Fusion MPT, HPSA, FCoE, iSCSI target farms | Single consumer NVMe |
| **md RAID** | `md_mod`/`raid1` loaded but no arrays | Disable unless you plan software RAID |
| **Sound bloat** | 700+ SND options; keep HDA + USB + HDMI + SOF Intel TGL; drop obscure SoC/PCI cards | Desktop USB + board HDA |
| **Unused LSMs** | SELinux, Smack, Tomoyo, AppArmor (not in active LSM list) | Saves code + hooks; keep landlock/yama/bpf/lockdown |
| **Debug/trace tax** | Full ftrace tracers, `DEBUG_INFO` DWARF for production image, KFENCE default sample if not debugging, `DYNAMIC_DEBUG` optional | Smaller image; keep `DEBUG_INFO_BTF` if you need bpftool CO-RE |
| **ACPI laptop/PMIC** | Battery-centric, ChromeOS EC, many EXTCON phone chips | Desktop ASUS |
| **Staging + rare HID/input** | Everything not matching your USB IDs | Logitech G502 + Apple keyboard only |

### Do **not** strip for “security” without understanding

Keep: `STACKPROTECTOR_STRONG`, `FORTIFY_SOURCE`, `RANDOMIZE_BASE`/`MEMORY`, `STRICT_*`, module signing (optional force off for local builds), Spectre mitigations that apply to Alder Lake (Enhanced IBRS is cheap; do **not** set `mitigations=off` for a daily driver).

Safe-ish runtime toggle for compile farms (benchmark only): `mitigations=auto` remains default. Measuring with `mitigations=off` is fine for isolation of numbers, not for daily use.

### NR_CPUS / SMP bloat

CachyOS: `CONFIG_NR_CPUS=8192` + `MAXSMP`. You have **24** logical CPUs. Set `NR_CPUS=32` or `64`. Shrinks per-CPU data structures and some static arrays → better D-cache for scheduler/RCU.

### `CONFIG_GENERIC_CPU` + x86-64-v3 vs native

Running kernel is **not** `X86_NATIVE_CPU`. Switching to **`-march=native` / `CONFIG_X86_NATIVE_CPU=y`** is the single largest free win for *kernel* hot paths on 12900K (AVX2, AES-NI codegen, etc.). Userspace Rust already benefits if you build rustc/crates with native; the kernel still enters constantly during compile (syscalls, FS, scheduler, networking for crate downloads).

---

## 2. Config recommendations (diff vs CachyOS running)

Base: `configs/cachyos-7.1.2-3.running.config` (snapshot of `/proc/config.gz`).

### 2.1 Must-change (performance / cache)

```text
# CPU targeting — biggest kernel IPC win
# CONFIG_GENERIC_CPU is not set
CONFIG_X86_NATIVE_CPU=y

# Or if Kconfig version uses:
# CONFIG_MNATIVE_INTEL=y

CONFIG_NR_CPUS=64
# CONFIG_MAXSMP is not set

# Keep O3 + ThinLTO for first custom build; Full LTO optional later
CONFIG_CC_OPTIMIZE_FOR_PERFORMANCE_O3=y
CONFIG_LTO_CLANG_THIN=y
# Full LTO: CONFIG_LTO_CLANG_FULL=y  (much longer link; better code layout)

# PGO / AutoFDO (Clang)
CONFIG_AUTOFDO_CLANG=y
# After training profile exists:
# CONFIG_AUTOFDO_CLANG=y and pass profile via scripts (see §3)

# Optional Propeller (layout) after AutoFDO
# CONFIG_PROPELLER_CLANG=y
```

### 2.2 Scheduler / latency (desktop + compile)

Cachy already: `PREEMPT` + `PREEMPT_DYNAMIC`, `HZ_1000`, `SCHED_CLASS_EXT`, `NO_HZ_FULL`.

Recommendations:

```text
CONFIG_PREEMPT=y
CONFIG_PREEMPT_DYNAMIC=y
CONFIG_HZ_1000=y
CONFIG_SCHED_CLASS_EXT=y
CONFIG_SCHED_AUTOGROUP=y
CONFIG_SCHED_CORE=y          # SMT-aware; good on 12900K hyperthreads
CONFIG_RCU_BOOST=y
CONFIG_RCU_NOCB_CPU=y
CONFIG_RCU_LAZY=y
```

**Policy choice (pick one path):**

| Path | When | How |
|------|------|-----|
| **A. Stock EEVDF + PREEMPT_DYNAMIC** (default recommend v1) | Stable daily driver, minimal surprise | No BORE; use `preempt=full` or `voluntary` via sysfs |
| **B. sched-ext (scx_lavd / scx_bpfland / scx_rusty)** | Want best interactive under load | Keep `SCHED_CLASS_EXT`; run `scx_lavd` or `scx_bpfland` from `scx-scheds` while cargo builds |
| **C. BORE patch** | Prefer CFS latency patch over scx | Apply BORE only if you maintain a patch set; Cachy often offers bore flavor packages — prefer distro patch over hand-rolling |

For **Rust compile storms** (many short-lived rustc/LLVM threads): EEVDF is already fair; **scx_lavd** or **scx_rusty** often feels better for “desktop stays smooth while -j24 cargo”. Benchmark both.

**Hybrid CPU:** Prefer cargo on all CPUs; pin latency-sensitive UI with `uclamp` or Plasma’s own priority. Optional: `intel_pstate` EPP `balance_performance` (current) for mixed; for pure benchmark runs use `performance`.

### 2.3 Memory / cache-friendly for rustc

```text
CONFIG_TRANSPARENT_HUGEPAGE=y
CONFIG_TRANSPARENT_HUGEPAGE_ALWAYS=y
CONFIG_COMPACTION=y
CONFIG_MIGRATION=y
CONFIG_ZRAM=m
CONFIG_ZRAM_BACKEND_ZSTD=y
# Zswap: you already boot with zswap.enabled=0 (zram primary) — OK
```

Userspace (not kernel config) for Rust:

- Keep `/tmp` on tmpfs (you have 32G tmpfs — excellent for `CARGO_TARGET_DIR` / incremental).
- `export CARGO_TARGET_DIR=/tmp/cargo-target` or per-project on tmpfs for max IOPS.
- Consider `mimalloc`/`jemalloc` global allocator only inside *your* binaries; rustc itself is already tuned.
- `vm.swappiness=10` (or lower) when 64G + zram; avoid swapping rustc working sets.

### 2.4 Security trim (keep secure, drop unused)

```text
CONFIG_LSM="landlock,lockdown,yama,bpf"
# CONFIG_SECURITY_SELINUX is not set
# CONFIG_SECURITY_SMACK is not set
# CONFIG_SECURITY_TOMOYO is not set
# CONFIG_SECURITY_APPARMOR is not set

CONFIG_STACKPROTECTOR_STRONG=y
CONFIG_FORTIFY_SOURCE=y
CONFIG_RANDOMIZE_BASE=y
CONFIG_RANDOMIZE_MEMORY=y
CONFIG_HARDENED_USERCOPY=y
# INIT_ON_ALLOC: small alloc tax; keep for safety on daily driver
CONFIG_INIT_ON_ALLOC_DEFAULT_ON=y
# CONFIG_INIT_ON_FREE_DEFAULT_ON is not set
```

### 2.5 Debug strip (production image)

```text
# CONFIG_DEBUG_INFO_DWARF5 is not set   # or none — shrink vmlinux dramatically
CONFIG_DEBUG_INFO_BTF=y                 # keep if you use scx / bpf
# CONFIG_KFENCE is not set              # or leave on at low sample rate
# Disable heavy tracers for release:
# CONFIG_FUNCTION_TRACER is not set
# CONFIG_FTRACE_SYSCALLS is not set
```

Keep `IKCONFIG_PROC=y` for introspection.

### 2.6 Driver minimalism (modules for rare, =n for never)

```text
# GPU: only what NVIDIA needs + simplefb/efifb for early boot
# CONFIG_DRM_I915 is not set
# CONFIG_DRM_XE is not set
# CONFIG_DRM_AMDGPU is not set
# CONFIG_DRM_NOUVEAU is not set
CONFIG_DRM=y
CONFIG_DRM_FBDEV_EMULATION=y

# Net
CONFIG_IGC=y                 # or m
CONFIG_IWLMVM=m
CONFIG_IWLWIFI=m
# CONFIG_WLAN_VENDOR_REALTEK is not set
# (disable entire unused vendor trees)

# Block
CONFIG_NVME_CORE=y
CONFIG_BLK_DEV_NVME=y
CONFIG_VMD=y
CONFIG_SATA_AHCI=m
# CONFIG_MD is not set          # if no mdadm arrays
CONFIG_BLK_DEV_DM=y
CONFIG_DM_CRYPT=y
CONFIG_ZRAM=m

# FS
CONFIG_BTRFS_FS=y
CONFIG_VFAT_FS=y
CONFIG_FAT_FS=y
CONFIG_NTFS3_FS=m
CONFIG_FUSE_FS=y
CONFIG_OVERLAY_FS=m
# CONFIG_XFS_FS is not set
# CONFIG_F2FS_FS is not set
# CONFIG_JFS_FS is not set
# CONFIG_EXT4_FS=y             # keep as m or y for rescue USB; your root is btrfs
```

### 2.7 Localversion

```text
CONFIG_LOCALVERSION="-rustopt"
# CONFIG_LOCALVERSION_AUTO is not set
```

### 2.8 Fragment file

See `configs/rustopt.fragment` — apply with:

```fish
scripts/merge-config.sh -m .config configs/rustopt.fragment
make olddefconfig
```

---

## 3. Full build instructions

### 3.0 Prerequisites

```fish
sudo pacman -S --needed base-devel clang lld llvm bc libelf pahole cpio \
  python git pahole rust scx-scheds \
  linux-cachyos-headers  # for nvidia module match during transition

# Free /boot first (CRITICAL)
df -h /boot
# Example: remove LTS if unused
# sudo pacman -Rns linux-cachyos-lts linux-cachyos-lts-headers linux-cachyos-lts-nvidia-open
```

Disk: expect **20–40 GiB** free for full LTO+PGO trees under `/home` or `/var/tmp`.

### 3.1 Source strategy (recommended order)

**Option A — CachyOS PKGBUILD (easiest NVIDIA + hooks)**  
Use `cachyos-kernel-manager` or clone Cachy kernel PKGBUILD, inject fragment + `X86_NATIVE_CPU`, build package. Keeps mkinitcpio/nvidia-dkms integration.

**Option B — Mainline + selective Cachy patches**  
More control; more breakage risk with NVIDIA open modules.

Recommend **Option A** for daily driver; use mainline only if you need a feature not in Cachy yet.

```fish
# Example layout
mkdir -p ~/src && cd ~/src
# Prefer Cachy kernel sources matching your running series:
# https://github.com/CachyOS/linux-cachyos  or PKGBUILD from CachyOS-PKGBUILDs
git clone --depth=1 https://github.com/CachyOS/linux-cachyos.git
# Or fetch kernel.org vX.Y + Cachy patchset used by 7.1.x
```

### 3.2 Baseline config

```fish
cd ~/src/linux-cachyos   # or extracted linux-X.Y
zcat /proc/config.gz > .config
# merge fragment from this repo:
scripts/kconfig/merge_config.sh -m .config \
  /home/imma/projects/kernel/configs/rustopt.fragment
make LLVM=1 LLVM_IAS=1 olddefconfig
make LLVM=1 LLVM_IAS=1 localmodconfig   # OPTIONAL: further strip from lsmod
# After localmodconfig, re-enable anything needed for boot that was idle:
# btrfs, dm-crypt, nvme, vmd, igc, xhci, efivarfs, etc. — review carefully
```

`localmodconfig` is powerful but dangerous: modules not loaded *now* disappear. Cross-check against §1 keep-list.

### 3.3 Build (non-PGO ThinLTO native) — v1

```fish
set -x PATH /usr/lib/llvm/22/bin $PATH  # if needed
set -x KCFLAGS "-O3 -pipe"
# native is mostly from CONFIG_X86_NATIVE_CPU; optional extra:
# set -x KCFLAGS "$KCFLAGS -march=native"

make LLVM=1 LLVM_IAS=1 -j(nproc) all
make LLVM=1 LLVM_IAS=1 -j(nproc) modules

sudo make LLVM=1 LLVM_IAS=1 modules_install
sudo make LLVM=1 LLVM_IAS=1 install
# Cachy/Arch: better to package; or:
# sudo cp arch/x86/boot/bzImage /boot/vmlinuz-linux-rustopt
# rebuild initramfs + nvidia module for new kernel version string
```

NVIDIA: install `nvidia-dkms` or rebuild `linux-*-nvidia-open` against new headers. Mismatch = black screen / no modeset.

### 3.4 PGO / AutoFDO training (Rust-focused)

Clang AutoFDO on kernels typically:

1. Build instrumented or use `perf` sampling with last-branch records.
2. Convert to LLVM profile.
3. Rebuild with profile-guided optimization + ThinLTO.

#### Method: AutoFDO via `perf` (practical on desktop)

```fish
# 1) Instrumented-less sample on a *running* custom kernel (v1 native ThinLTO)
# Install: sudo pacman -S perf

# 2) Training workload — mirror real use
cd ~/projects/appsynergy-rs   # or largest multi-crate workspace
cargo clean
# Record kernel samples while compiling (needs root or perf_event_paranoid)
sudo sysctl kernel.perf_event_paranoid=1

# 30–120 min of representative work:
perf record -e br_inst_retired.near_taken:u -b -c 500009 \
  -o /tmp/kernel-autofdo.data -- \
  fish -c 'cargo build --release -j24; cargo test -j24; cargo check -j24'

# Also train desktop interactivity briefly:
# open Plasma apps, browse, while another cargo build runs

# 3) Create AutoFDO profile (tooling depends on llvm version)
# Cachy documents AutoFDO in kernel PKGBUILDs; typical flow:
create_llvm_prof --binary=/usr/lib/modules/(uname -r)/vmlinux \
  --profile=/tmp/kernel-autofdo.data --format=extbinary \
  --out=/home/imma/projects/kernel/profiles/rust-kernel.afdo

# 4) Rebuild kernel with:
#   CLANG_AUTOFDO_PROFILE=/path/to/rust-kernel.afdo
#   CONFIG_AUTOFDO_CLANG=y
```

CachyOS kernel PKGBUILDs often wrap this as `build_with_autofdo`. Prefer their scripts when building via `linux-cachyos` PKGBUILD — they already set `CONFIG_AUTOFDO_CLANG`.

#### PGO training script outline (userspace cargo suite)

```fish
# scripts/pgo-train-rust.fish — run under perf/instrumentation
set projects appsynergy-rs beetv-rs combly keel frontier
for p in $projects
  if test -d $HOME/projects/$p
    pushd $HOME/projects/$p
    cargo fetch
    cargo check -j24
    cargo build --release -j24
    cargo test --tests -j24 -- --test-threads=24
    popd
  end
end
# Kernel compile as secondary train (scheduler + FS):
# cd ~/src/linux && make LLVM=1 -j24 clean; make LLVM=1 -j24 vmlinux
```

### 3.5 Boot entry

```fish
# After modules_install + initramfs:
sudo mkinitcpio -p linux-rustopt   # if pacman package
# or mkinitcpio -k <ver> -g /boot/initramfs-linux-rustopt.img
sudo bootctl update   # if systemd-boot
# Ensure cmdline keeps LUKS + btrfs subvol:
# root=UUID=... rootflags=subvol=/@ cryptdevice=UUID=...:...
# Suggested extras:
# nowatchdog zswap.enabled=0 preempt=full
# Optional scx later via userspace, not cmdline
```

Keep previous CachyOS kernel as fallback entry until 1 week stable.

### 3.6 Post-boot sysctl / THP (Rust)

```fish
# /etc/sysctl.d/99-rustopt.conf
vm.swappiness = 10
vm.vfs_cache_pressure = 50
vm.dirty_ratio = 15
vm.dirty_background_ratio = 5
# THP already always via config; verify:
# cat /sys/kernel/mm/transparent_hugepage/enabled
```

```fish
# Optional: cargo on tmpfs
set -Ux CARGO_TARGET_DIR /tmp/cargo-target-$USER
```

---

## 4. Major changes → cache / Rust build impact

| Change | Mechanism | Helps Rust how |
|--------|-----------|----------------|
| **`X86_NATIVE_CPU` / -march=native** | Better kernel codegen (AVX2 paths, instruction selection) | Faster syscalls, memcpy/clear_page, crypto (LUKS), scheduler | 
| **Strip unused drivers/LSMs** | Smaller `vmlinux` text + fewer modules | Less I-cache / iTLB pollution when 24 rustc threads hammer kernel |
| **`NR_CPUS=64`** | Smaller static per-CPU footprints | Better D-cache locality in sched/RCU under high fork/exec |
| **ThinLTO → (+AutoFDO)** | Cross-module inlining + measured hot/cold layout | Hot kernel paths (futex, epoll, read/write, page fault) denser in I-cache |
| **THP always** | Fewer page faults / page-walks for large heaps | rustc/LLVM love large anonymous maps |
| **PREEMPT_DYNAMIC + scx optional** | Low tail latency under load | Plasma stays responsive during `-j24`; less “jank tax” |
| **zram + low swappiness** | Avoid disk thrash | Compile working sets stay in RAM |
| **tmpfs target dir** | Zero NVMe for intermediate artifacts | Wall-clock cargo wins (often larger than kernel delta!) |
| **Disable unused AMD/iGPU DRM** | Less driver code linked/loaded | Cleaner L3 during graphics+compile |
| **Keep btrfs/dm-crypt optimized (AES-NI)** | Hardware crypto | Encrypted root less of a bottleneck |

**Honest expectation:** CachyOS is already near state-of-the-art. Realistic wins:

- Kernel-only native+strip+PGO: often **1–5%** wall-clock on pure CPU compile; **noticeable latency** under load.
- Kernel + userspace (tmpfs target, mold/lld linker, `RUSTFLAGS=-C target-cpu=native`, sccache): often **10–40%** on clean builds — do these first if not already.

---

## 5. Benchmark commands

### 5.0 Baseline capture (before reboot into new kernel)

```fish
mkdir -p /home/imma/projects/kernel/bench/(date +%Y%m%d)-cachyos
set B /home/imma/projects/kernel/bench/(date +%Y%m%d)-cachyos
uname -a > $B/uname.txt
cat /proc/cmdline > $B/cmdline.txt
```

### 5.1 Cache misses during cargo

```fish
cd ~/projects/appsynergy-rs  # large workspace
cargo clean
perf stat -e cycles,instructions,cache-references,cache-misses,\
L1-dcache-load-misses,L1-icache-load-misses,LLC-loads,LLC-load-misses,\
branch-misses,context-switches,cpu-migrations,page-faults \
  -o $B/perf-cargo-build.txt -- \
  cargo build --release -j24
```

### 5.2 Kernel compile time (scheduler + FS stress)

```fish
cd ~/src/linux
make LLVM=1 LLVM_IAS=1 clean
/usr/bin/time -v make LLVM=1 LLVM_IAS=1 -j24 vmlinux 2> $B/time-kernel-build.txt
```

### 5.3 Full Rust project timings

```fish
for p in appsynergy-rs beetv-rs combly
  set dir $HOME/projects/$p
  test -d $dir; or continue
  pushd $dir
  cargo clean
  /usr/bin/time -f '%e sec wall, %U user, %S sys' -o $B/time-$p-check.txt cargo check -j24
  /usr/bin/time -f '%e sec wall, %U user, %S sys' -o $B/time-$p-release.txt cargo build --release -j24
  /usr/bin/time -f '%e sec wall, %U user, %S sys' -o $B/time-$p-test.txt cargo test -j24
  popd
end
```

### 5.4 Latency under compile load

```fish
# Terminal 1:
cd ~/projects/appsynergy-rs && cargo build --release -j24

# Terminal 2 — cyclictest (install rt-tests):
sudo cyclictest -m -S -p 80 -i 200 -h 40 -q -D 60 | tee $B/cyclictest.txt

# Or desktop feel:
# sudo pacman -S latencytop  (limited); or use `interbench`; or manual: drag windows while compile
```

### 5.5 Optional micro

```fish
# syscall/futex heavy
perf bench sched messaging -g 25 -l 1000 | tee $B/perf-bench-sched.txt
perf bench mem memcpy -s 1GB -l 20 | tee $B/perf-bench-memcpy.txt
```

Compare same commands on CachyOS vs rustopt; keep governor/EPP identical (`performance` for apples-to-apples).

```fish
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor
# intel_pstate:
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference
```

---

## 6. Iteration plan

| Stage | Goal | Accept when |
|-------|------|-------------|
| **0** | Free `/boot`; userspace wins (tmpfs `CARGO_TARGET_DIR`, `RUSTFLAGS=-C target-cpu=native`, mold/lld, sccache) | Clean build times drop without new kernel |
| **1** | Cachy config + **native** + **NR_CPUS=64** + LSM trim only | Boots KDE+NVIDIA+LUKS; bench ≥ parity |
| **2** | `localmodconfig` + manual keep-list; drop AMD/iGPU DRM | Smaller modules dir; no missing hw |
| **3** | AutoFDO train on cargo+kernel build; rebuild | Hot path perf stat improves |
| **4** | Try Full LTO (weekend build) | Link succeeds; measure |
| **5** | scx_lavd vs scx_bpfland vs EEVDF under cargo+Plasma | Pick lowest cyclictest p99 + subjective |
| **6** | Optional BORE only if scx loses | Maintainability cost OK |
| **7** | Propeller / advanced layout if AutoFDO helps | Diminishing returns |
| **8** | Rebase on each Cachy minor; re-profile quarterly | Stay secure/current |

**Stop condition:** when wall-clock cargo gains <1% after a change, invest in rustc/LLVM/userspace instead.

---

## 7. Open questions (need your input)

1. **Primary Rust repos for PGO training** — which 2–3 trees (e.g. `appsynergy-rs`, `beetv-rs`) best represent daily work?
2. **iGPU / QuickSync** — ever used, or safe to disable `i915`/`xe` completely?
3. **Windows dual-boot** — need read-write NTFS from Linux regularly, or can ntfs3 stay modular/rarely used?
4. **Wi-Fi** — keep soft-blocked as backup, or unused (still keep modules as `m`)?
5. **Package style** — prefer CachyOS PKGBUILD + kernel manager, or hand-rolled `make install`?
6. **Risk tolerance** — OK to run AutoFDO weekend builds and Full LTO, or stick ThinLTO+native only?
7. **scx** — want interactive tuning with `scx_lavd` as default, or pure EEVDF?

---

## 8. Quick wins right now (no rebuild)

```fish
# 1) Native codegen for your crates
set -Ux RUSTFLAGS "-C target-cpu=native"

# 2) Fast linker (if not already)
# rustup component / pacman clang + mold
set -Ux RUSTFLAGS "$RUSTFLAGS -C link-arg=-fuse-ld=mold"

# 3) tmpfs target
set -Ux CARGO_TARGET_DIR /tmp/cargo-target-$USER

# 4) Under heavy compile, try scx
sudo pacman -S scx-scheds
sudo scx_lavd --performance   # example; see scx_lavd --help

# 5) Match governor for builds
echo performance | sudo tee /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference
```

These often beat a week of kernel micro-optimization for cargo wall time.
