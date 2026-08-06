#!/usr/bin/bash
# Stage official k3s binary + systemd unit into live airootfs for server installs.
# Binary is NOT committed (see .gitignore). Run from build-iso.sh.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/iso/airootfs/opt/appsynergy/k3s"
CFG="$ROOT/iso/airootfs/etc/appsynergy/server/k3s-config.yaml"
# Pin: bump deliberately when refreshing the ISO payload.
K3S_VER="${K3S_VER:-v1.36.2+k3s1}"
URL="https://github.com/k3s-io/k3s/releases/download/${K3S_VER}/k3s"

mkdir -p "$DEST" "$(dirname "$CFG")"

need_dl=1
if [[ -x "$DEST/k3s" && -s "$DEST/VERSION" ]]; then
  if grep -qF "$K3S_VER" "$DEST/VERSION" 2>/dev/null || "$DEST/k3s" --version 2>/dev/null | grep -qF "${K3S_VER#v}"; then
    need_dl=0
    echo "  keep staged k3s ($(du -h "$DEST/k3s" | awk '{print $1}')) $K3S_VER"
  fi
fi

if (( need_dl )); then
  echo "  downloading k3s $K3S_VER …"
  tmp=$(mktemp)
  curl -fL --retry 3 -o "$tmp" "$URL"
  chmod 755 "$tmp"
  mv -f "$tmp" "$DEST/k3s"
  "$DEST/k3s" --version | tee "$DEST/VERSION" >/dev/null
  echo "  staged k3s ($(du -h "$DEST/k3s" | awk '{print $1}'))"
fi

# systemd unit (ExecStart=/usr/local/bin/k3s server — installer places binary there)
if [[ ! -s "$DEST/k3s.service" ]]; then
  cat >"$DEST/k3s.service" <<'UNIT'
[Unit]
Description=Lightweight Kubernetes
Documentation=https://k3s.io
Wants=network-online.target
After=network-online.target

[Service]
Type=notify
EnvironmentFile=-/etc/default/%N
EnvironmentFile=-/etc/sysconfig/%N
EnvironmentFile=-/etc/systemd/system/k3s.service.env
KillMode=process
Delegate=yes
LimitNOFILE=1048576
LimitNPROC=infinity
LimitCORE=infinity
TasksMax=infinity
TimeoutStartSec=0
Restart=always
RestartSec=5s
ExecStartPre=-/sbin/modprobe br_netfilter
ExecStartPre=-/sbin/modprobe overlay
ExecStart=/usr/local/bin/k3s server
ExecReload=/bin/kill -s HUP $MAINPID

[Install]
WantedBy=multi-user.target
UNIT
fi
[[ -f "$DEST/k3s.service.env" ]] || { : >"$DEST/k3s.service.env"; chmod 400 "$DEST/k3s.service.env"; }

# Default server config (no Traefik/ServiceLB) if missing.
# K3S_CONFIG_SRC is an EXPLICIT opt-in to pull config from another checkout —
# the old silent /home/imma/projects/appsynergy-rs fallback made two ISO builds
# from the same commit differ depending on an unrelated repo's presence.
if [[ ! -f "$CFG" ]]; then
  if [[ -n "${K3S_CONFIG_SRC:-}" && -f "$K3S_CONFIG_SRC" ]]; then
    echo "stage-k3s: using external K3S_CONFIG_SRC=$K3S_CONFIG_SRC"
    cp -a "$K3S_CONFIG_SRC" "$CFG"
  else
    cat >"$CFG" <<'CFG'
disable:
  - traefik
  - servicelb
secrets-encryption: true
write-kubeconfig-mode: "0600"
CFG
  fi
fi

[[ -x "$DEST/k3s" ]] || { echo "ERROR: k3s binary missing at $DEST/k3s"; exit 1; }
