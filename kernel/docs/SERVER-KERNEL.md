# Server kernels: dual host-max (OVH + NUC)

OS variant: **[SERVER-OS.md](SERVER-OS.md)**. Security: **[SERVER-SECURITY.md](SERVER-SECURITY.md)**.

Same CachyOS series as desktop `linux-appsynergy` (e.g. 7.1.x), Clang ThinLTO, O3, AutoFDO-ready.
**No portable compromise package** — installer ships **both** and defaults boot by CPU.

| Package | uname suffix | Metal | ISA (`KCFLAGS`) | NIC builtin |
|---------|--------------|-------|-----------------|-------------|
| `linux-appsynergy-server-skylake` | `-appsynergy-server-skylake` | server1: Xeon **E3-1270 v6** | **skylake** | `igb` (I210) |
| `linux-appsynergy-server-tigerlake` | `-appsynergy-server-tigerlake` | lab `192.168.101.101`: **i7-1185G7** | **tigerlake** | `igc` (I225-LM) |

Desktop package stays **`linux-appsynergy`**.

## Profile (both)

| Item | Value |
|------|--------|
| Workloads | WireGuard, nftables, policy routing, systemd, cgroup v2, namespaces, Rust bins |
| Future | eBPF / XDP (BTF + AF_XDP + cls_bpf) |
| Not present | DE, GPU/DRM, Wi-Fi, BT, audio, ASUS WMI |
| Tick | HZ 250 · `preempt=voluntary` cmdline |
| THP | madvise · `NR_CPUS=16` · Intel-only · performance governor |
| Mitigations | kept on production (no `mitigations=off`) |

## Fragments

| File | Role |
|------|------|
| `configs/server-skylake.fragment` | Skylake max |
| `configs/server-tigerlake.fragment` | Tiger Lake max |
| `configs/server.fragment` | **legacy portable** reference only — do not ship |

## Build (critical)

Cachy `prepare` enables **`X86_NATIVE_CPU`** when `_processor_opt` is empty — that is the **build host** (e.g. 12900K), **wrong** for OVH/NUC. Scripts force `_processor_opt=generic_v3` + `KCFLAGS=-march=…`.

```bash
# both (installer payload)
./scripts/build-linux-appsynergy-server.sh

# or one:
./scripts/build-linux-appsynergy-server-skylake.sh
./scripts/build-linux-appsynergy-server-tigerlake.sh
# packages → repo/x86_64/ + appsynergy-desktop/iso/.../opt/appsynergy/pkgs/
```

Post-build: config must have `# CONFIG_X86_NATIVE_CPU is not set`. Script refuses otherwise.

## Installer

`appsynergy-install --variant server --kernel local` (variant is **your** choice):

1. CPU auto-maps **one** package (+ headers): Skylake/Kaby/Coffee/Comet/Xeon E3 v5–v6 → `…-server-skylake`; Tiger Lake / 11th gen / 1185G7 → `…-server-tigerlake`.
2. Writes a single matching systemd-boot entry (e.g. `appsynergy-skylake.conf`).
3. Does **not** install both host-max kernels. Unmapped CPUs (e.g. Alder Lake) fail with a clear error unless `--kernel repo`.

## Boot cmdline

```
rd.luks.name=…=cryptroot root=UUID=… rootflags=subvol=@ rw \
  zswap.enabled=0 preempt=voluntary ip=dhcp
# ip=dhcp only when SSH unlock armed
```
