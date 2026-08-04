#!/usr/bin/bash
# Stage/install both host-max server kernels (ovh + nuc) into repo + ISO payload.
set -euo pipefail

# Monorepo root: sibling subtree desktop/ lives beside packages/.
MONO="$(cd "$(dirname "$0")/../.." && pwd)"
KDIR="${KDIR:-/home/imma/src/linux-cachyos/linux-cachyos}"
REPO="${REPO:-$(cd "$(dirname "$0")/.." && pwd)/repo/x86_64}"
ISO_PKGS="${ISO_PKGS:-$MONO/desktop/iso/airootfs/opt/appsynergy/pkgs}"

pick() {
  local dir=$1 prefix=$2
  shopt -s nullglob
  local out=()
  for p in "$dir"/${prefix}-[0-9]*.pkg.tar.zst "$dir"/${prefix}-headers-*.pkg.tar.zst; do
    [[ -f $p ]] || continue
    [[ $p == *dbg* ]] && continue
    out+=("$p")
  done
  shopt -u nullglob
  printf '%s\n' "${out[@]}"
}

filtered=()
for prefix in linux-appsynergy-server-skylake linux-appsynergy-server-tigerlake; do
  mapfile -t got < <(pick "$KDIR" "$prefix")
  if ((${#got[@]} < 2)); then
    mapfile -t got < <(pick "$REPO" "$prefix")
  fi
  if ((${#got[@]} < 2)); then
    echo "WARN: missing $prefix (+ headers) in $KDIR or $REPO" >&2
    continue
  fi
  filtered+=("${got[@]}")
done

((${#filtered[@]} >= 2)) || {
  echo "missing host-max server kernels (need ovh and/or nuc packages)" >&2
  exit 1
}

mkdir -p "$REPO"
cp -a "${filtered[@]}" "$REPO/"
echo "Packages: ${filtered[*]}"

if [[ -d $ISO_PKGS ]]; then
  cp -a "${filtered[@]}" "$ISO_PKGS/"
  echo "Staged to ISO: $ISO_PKGS"
fi

if [[ ${INSTALL:-0} == 1 ]]; then
  sudo pacman -U --noconfirm "${filtered[@]}"
  command -v mkinitcpio >/dev/null && sudo mkinitcpio -P || true
  command -v bootctl >/dev/null && sudo bootctl update || true
  echo "Installed on this host. Prefer matching flavor in loader default."
fi
