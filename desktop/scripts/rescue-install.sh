#!/usr/bin/env bash
# Install AppSynergy Server from a NON-Arch rescue system (OVH rescue = Debian).
#
# Unpacks the shipped Arch bootstrap (pacstrap/arch-chroot, keyring + mirrors
# already baked in), wires the payload into it, and runs the real installer.
# Nothing here improvises: every input is verified before the first destructive
# step, and the installer itself refuses a server install without an unlock key.
#
#   bash rescue-install.sh --disk /dev/nvme0n1,/dev/nvme1n1 [--flavour skylake]
#
# Run rescue-preflight.sh FIRST and read its output.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PAYLOAD="${PAYLOAD:-$HERE}"          # dir holding pkgs/ etc/ bootstrap/ SHA256SUMS
CHROOT="${CHROOT:-/root/appsynergy-bootstrap}"
FLAVOUR=skylake
DISKS=""
PASSTHRU=()

die() { printf '\nERROR: %s\n' "$*" >&2; exit 1; }
say() { printf '==> %s\n' "$*"; }

while (( $# )); do
  case "$1" in
    --disk)    DISKS="$2"; shift 2 ;;
    --flavour) FLAVOUR="$2"; shift 2 ;;
    --payload) PAYLOAD="$2"; shift 2 ;;
    -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
    *)         PASSTHRU+=("$1"); shift ;;
  esac
done

[[ $EUID -eq 0 ]] || die "run as root"
[[ -n "$DISKS" ]] || die "--disk is required (comma-separated). Take the names from rescue-preflight.sh."
[[ "$FLAVOUR" =~ ^(skylake|tigerlake)$ ]] || die "--flavour must be skylake or tigerlake"

# ---------------------------------------------------------------- verify input
say "verifying payload"
[[ -f "$PAYLOAD/SHA256SUMS" ]] || die "no SHA256SUMS in $PAYLOAD"
( cd "$PAYLOAD" && sha256sum -c --quiet SHA256SUMS ) || die "payload checksum mismatch — re-copy it"
say "  checksums ok"

BOOTSTRAP=$(ls -1 "$PAYLOAD"/bootstrap/*bootstrap*.tar.zst 2>/dev/null | head -1)
[[ -n "$BOOTSTRAP" && -f "$BOOTSTRAP" ]] || die "bootstrap tarball missing from $PAYLOAD/bootstrap/"

for d in ${DISKS//,/ }; do
  [[ -b "$d" ]] || die "not a block device: $d"
done
say "  target disks: $DISKS"

KPKG=$(ls -1 "$PAYLOAD"/pkgs/linux-appsynergy-server-${FLAVOUR}-[0-9]*.pkg.tar.zst 2>/dev/null | head -1)
[[ -n "$KPKG" ]] || die "no $FLAVOUR kernel package in $PAYLOAD/pkgs/"
say "  kernel: $(basename "$KPKG")"

PUBKEY="$PAYLOAD/etc/ssh-unlock.pub"
[[ -s "$PUBKEY" ]] || die "missing $PUBKEY — a headless server cannot be unlocked without it"
say "  unlock key: $(ssh-keygen -lf "$PUBKEY" 2>/dev/null || echo present)"

# ------------------------------------------------------------------- unmount
# Always leave the chroot clean. A bind-mounted /dev IS the host's /dev, so a
# later delete over a still-mounted tree would destroy real device nodes.
cleanup() {
  local rc=$?
  say "unmounting chroot"
  local m
  while read -r m; do
    [[ -n "$m" ]] || continue
    umount -R "$m" 2>/dev/null || umount -l "$m" 2>/dev/null || true
  done < <(findmnt -rno TARGET | grep "^$CHROOT" | sort -r)
  if findmnt -rno TARGET | grep -q "^$CHROOT"; then
    printf '  WARNING: mounts remain under %s — do NOT delete that directory yet:\n' "$CHROOT"
    findmnt -rno TARGET | grep "^$CHROOT"
  else
    say "  clear"
  fi
  exit $rc
}
trap cleanup EXIT INT TERM

# ------------------------------------------------------------- unpack chroot
say "unpacking bootstrap into $CHROOT"
if findmnt -rno TARGET | grep -q "^$CHROOT"; then
  die "$CHROOT already has mounts — unmount before re-running"
fi
rm -rf "$CHROOT"; mkdir -p "$CHROOT"
# --long=31 is mandatory: the tarball is compressed with --long.
zstd -dc --long=31 "$BOOTSTRAP" | tar -C "$CHROOT" -x || die "failed to unpack bootstrap"
R="$CHROOT/root.x86_64"
[[ -d "$R" ]] || die "unexpected bootstrap layout"

for b in pacstrap arch-chroot sgdisk cryptsetup mkfs.btrfs bootctl; do
  [[ -x "$R/usr/bin/$b" || -x "$R/usr/sbin/$b" ]] || die "bootstrap is missing $b"
done
say "  tooling present"

# ----------------------------------------------------------------- wire it up
say "wiring payload into the chroot"
mkdir -p "$R/etc/appsynergy" "$R/opt/appsynergy/pkgs"
cp -a "$PAYLOAD/etc/." "$R/etc/appsynergy/"
# Only the selected flavour's kernel; the other would just slow pacman -U.
cp -a "$PAYLOAD"/pkgs/linux-appsynergy-server-${FLAVOUR}-*.pkg.tar.zst "$R/opt/appsynergy/pkgs/" 2>/dev/null || true
cp -a "$PAYLOAD"/pkgs/appsynergy-*.pkg.tar.zst "$R/opt/appsynergy/pkgs/" 2>/dev/null || true
install -Dm755 "$PAYLOAD/appsynergy-install" "$R/usr/local/bin/appsynergy-install" \
  || die "installer binary missing from payload"

mount --bind /proc "$R/proc" || die "bind /proc failed"
mount --bind /sys  "$R/sys"  || die "bind /sys failed"
mount --bind /dev  "$R/dev"  || die "bind /dev failed"
mount --bind /run  "$R/run"  2>/dev/null || true
cp /etc/resolv.conf "$R/etc/resolv.conf" 2>/dev/null || true

# --------------------------------------------------------------------- install
say "running appsynergy-install (this partitions and wipes $DISKS)"
chroot "$R" /usr/local/bin/appsynergy-install \
  --variant server \
  --disk "$DISKS" \
  --ssh-pubkey /etc/appsynergy/ssh-unlock.pub \
  "${PASSTHRU[@]}"
rc=$?

if (( rc == 0 )); then
  say "install finished"
  printf '\n  Next: set OVH boot mode back to hard disk, then reboot.\n'
  printf '  First boot: ssh into the initrd to unlock, then ssh in normally.\n'
  printf '  Verify with: uname -r  -> expect *-appsynergy-server-%s\n' "$FLAVOUR"
else
  printf '\n  install failed (rc=%s) — target disks may be partially written.\n' "$rc"
fi
exit $rc
