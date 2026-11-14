#!/usr/bin/bash
# Root-only integration test, isolated from host mounts, routes and firewall.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
if [[ ${1:-} != --isolated ]]; then
  [[ $EUID == 0 ]] || { echo 'run with sudo' >&2; exit 1; }
  exec unshare --mount --net "$0" --isolated
fi
mount --make-rprivate /
mount -t tmpfs tmpfs /run
mkdir -p /run/netns
ip link set lo up
for ns in pod wan; do ip netns add "$ns"; ip -n "$ns" link set lo up; done
ip link add cni0 type veth peer name pod0
ip link set pod0 netns pod
ip addr add 10.42.0.1/24 dev cni0
ip link set cni0 up
ip -n pod addr add 10.42.0.2/24 dev pod0
ip -n pod link set pod0 up
ip -n pod route add default via 10.42.0.1
ip link add eth0 type veth peer name wan0
ip link set wan0 netns wan
ip addr add 192.0.2.1/24 dev eth0
ip link set eth0 up
ip -n wan addr add 192.0.2.2/24 dev wan0
ip -n wan link set wan0 up
ip -n wan route add default via 192.0.2.1
sysctl -qw net.ipv4.ip_forward=1
pids=()
cleanup() {
  kill "${pids[@]}" 2>/dev/null || true
  for ns in pod wan; do ip netns del "$ns"; done
}
trap cleanup EXIT
serve() {
  python3 -c 'import socket,sys; s=socket.socket(); s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1); s.bind(("0.0.0.0",int(sys.argv[1]))); s.listen();
while True:
 c,a=s.accept(); c.close()' "$1" &
  pids+=("$!")
}
connect() {
  ip netns exec "$1" python3 -c 'import socket,sys; socket.create_connection((sys.argv[1],int(sys.argv[2])),timeout=1).close()' "$2" "$3" 2>/dev/null
}
serve 22
serve 6443
serve 10250
serve 8080
ip netns exec pod python3 -m http.server 8080 >/dev/null 2>&1 & pids+=("$!")
sleep 0.3
server="$ROOT/iso/airootfs/etc/appsynergy/server-nftables.conf"
nft -f "$server"
nft -f "$server" # Reload must not duplicate chains/rules or erase K3s tables.
ip netns exec pod ping -c1 -W1 192.0.2.2 >/dev/null
if ip netns exec wan ping -c1 -W1 10.42.0.2 >/dev/null; then echo 'Unpublished inbound pod traffic passed' >&2; exit 1; fi
connect pod 10.42.0.1 6443
connect pod 10.42.0.1 10250
connect wan 192.0.2.1 22
if connect wan 192.0.2.1 6443; then echo 'Public API exposure' >&2; exit 1; fi
if connect wan 192.0.2.1 10250; then echo 'Public kubelet exposure' >&2; exit 1; fi
if connect pod 10.42.0.1 8080; then echo 'Unrelated host service exposed to pods' >&2; exit 1; fi
nft -f - <<'RULES'
table ip test_service {
 chain prerouting { type nat hook prerouting priority dstnat; policy accept;
  iifname "eth0" tcp dport 30080 dnat to 10.42.0.2:8080
 }
}
RULES
connect wan 192.0.2.1 30080
nft -f "$server"
connect wan 192.0.2.1 30080
# A separate NetworkPolicy drop must still win over the host table's accept.
nft -f - <<'RULES'
table inet test_policy {
 chain forward { type filter hook forward priority 10; policy accept;
  ip daddr 10.42.0.2 tcp dport 8080 drop
 }
}
RULES
if connect wan 192.0.2.1 30080; then echo 'NetworkPolicy drop bypassed' >&2; exit 1; fi
nft delete table inet test_policy
nft delete table ip test_service
nft -f "$ROOT/iso/airootfs/etc/appsynergy/desktop-nftables.conf"
connect wan 192.0.2.1 22
if connect wan 192.0.2.1 8080; then echo 'Workstation service exposed' >&2; exit 1; fi
echo 'PASS: pod egress, API access, SSH, published services, policy drops, reload, workstation isolation'
