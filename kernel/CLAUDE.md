# CLAUDE.md — kernel

CachyOS-based kernels for AppSynergy desktop and the two server metals. Build/profile detail lives in `README.md` and `docs/SERVER-KERNEL.md`; this file carries only what changes a decision.

## File map

| Path | Role |
|------|------|
| `configs/server-skylake.fragment` | `linux-appsynergy-server-skylake` — **the OVH appliance `144.217.66.212` runs this** |
| `configs/server-tigerlake.fragment` | `linux-appsynergy-server-tigerlake` — lab NUC `192.168.101.101` |
| `configs/server.fragment` | legacy portable profile, reference only — not shipped |
| `configs/rustopt.fragment`, `igpu.fragment` | desktop |
| `configs/cachyos-*.running.config` | upstream baseline the fragments modify |

## Invariants and gotchas

### Bridge netfilter and physdev are k3s prerequisites, not per-metal taste

`server-skylake.fragment` and `server.fragment` carried `# CONFIG_BRIDGE_NETFILTER is not set`, justified as "host is a router, not an L2 switch fabric". That was true before either server ran k3s. It costs two things on any host that does. `server-tigerlake.fragment` always set the symbol; its gap was `NETFILTER_XT_MATCH_PHYSDEV`, a *separate* symbol that gates on bridge netfilter but is never implied by it — which is exactly why the fragments must name both. `server.fragment` still needs both if it is ever shipped again.

1. **NetworkPolicy is silently unenforced.** `NETFILTER_XT_MATCH_PHYSDEV` is `depends on BRIDGE && BRIDGE_NETFILTER`, so switching off bridge netfilter drops the physdev match with it, whatever the baseline config says. k3s's kube-router controller needs it and aborts every sync:

   ```
   network_policy_controller.go:334] Aborting sync. Failed to run iptables-restore:
   exit status 4 (Warning: Extension physdev revision 0 not supported, missing kernel module?
   ```

   The failure mode is the dangerous one: the API accepts a NetworkPolicy, `kubectl get netpol` lists it, and nothing is programmed. Verified on the live box 2026-08-02 — with five `default-deny` policies applied, a workload Pod still reached the kube-apiserver on `:6443` and the kubelet on `:10250`. Check enforcement by counting policy chains, never by listing objects: `iptables-save -t filter | grep -c 'KUBE-NWPLCY-[A-Z0-9]\{16\}'` must be non-zero — and compare metals, since a partial count still looks alive: 2026-08-06 the NUC (no physdev) programmed 8 chains where the appliance programmed 93.

2. **Pod-to-Service traffic needs `masquerade-all`.** Bridged frames never reach conntrack, so a reply from a Pod endpoint is never un-DNATed and the client discards it. `appsynergy-rs` works around this in `ops/k3s/config.yaml` with `kube-proxy-arg: masquerade-all=true`, which costs the client's real address on every cluster service. That workaround is documented as standing "until CONFIG_BRIDGE_NETFILTER lands in the appsynergy-linux kernel".

**Fix:** both server fragments now declare `CONFIG_BRIDGE_NETFILTER=m` (the CachyOS baseline; `=y` is unreachable while `CONFIG_BRIDGE=m`) *and* `CONFIG_NETFILTER_XT_MATCH_PHYSDEV=m`, and `scripts/check.sh` stage `kernel-netfilter` asserts both in both. The gate deliberately ignores `upstream/config-*`: the shipped `config-7.1.5-2-appsynergy-server-tigerlake` predates the physdev fix (`# CONFIG_NETFILTER_XT_MATCH_PHYSDEV is not set`) and stays stale until that metal is rebuilt. Because the symbols are modular, building is not enough — `br_netfilter` must be loaded, so `modules-load-server.conf` lists it for new installs; the k3s unit's `ExecStartPre=-/sbin/modprobe` is `-`-prefixed and ignores failure, so it is not a guarantee. Leave `net.bridge.bridge-nf-call-iptables` at the k3s default rather than forcing it host-wide. After the appliance boots the new kernel, both `appsynergy-rs` workarounds can be retired — drop `masquerade-all` from `ops/k3s/config.yaml`, and `ops/k3s/11-networkpolicy.yaml` starts enforcing on its own.

### Ordinary kernel-config gotchas

- A fragment change is a no-op unless the kernel is reconfigured; a plain rebuild reuses the cached configured-stamp. The `.config` mtime is the tell.
- The appliance has **no TPM** — every boot needs a hand LUKS unlock over initrd SSH, so a bad kernel costs a manual recovery, not a reboot. Boot-test before shipping.
