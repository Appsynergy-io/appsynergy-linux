#!/usr/bin/bash
# Build one host-max server kernel: skylake | tigerlake
#   skylake:   Xeon E3-1270 v6  → -march=skylake   pkg linux-appsynergy-server-skylake
#   tigerlake: i7-1185G7       → -march=tigerlake pkg linux-appsynergy-server-tigerlake
#
# Critical: never leave _processor_opt empty — Cachy prepare defaults to
# X86_NATIVE_CPU (build-host 12900K). Force generic_v3 + KCFLAGS march.
set -euo pipefail

# Monorepo root: sibling subtrees kernel/ and desktop/ live beside packages/.
MONO="$(cd "$(dirname "$0")/../.." && pwd)"

FLAVOR="${1:-}"
case "$FLAVOR" in
  skylake)
    SUFFIX=appsynergy-server-skylake
    FRAG_DEFAULT="$MONO/kernel/configs/server-skylake.fragment"
    MARCH=skylake
    DESC="E3-1270 v6 / skylake / igb"
    ;;
  tigerlake)
    SUFFIX=appsynergy-server-tigerlake
    FRAG_DEFAULT="$MONO/kernel/configs/server-tigerlake.fragment"
    MARCH=tigerlake
    DESC="i7-1185G7 / tigerlake / igc"
    ;;
  *)
    echo "usage: $0 skylake|tigerlake" >&2
    exit 2
    ;;
esac

KDIR="${KDIR:-/home/imma/src/linux-cachyos/linux-cachyos}"
OUT_REPO="${OUT_REPO:-$(cd "$(dirname "$0")/.." && pwd)/repo/x86_64}"
FRAG="${FRAG:-$FRAG_DEFAULT}"
CFG_SRC="${CFG_SRC:-}"
ISO_PKGS="${ISO_PKGS:-$MONO/desktop/iso/airootfs/opt/appsynergy/pkgs}"
LOG="/tmp/linux-${SUFFIX}-build.log"

[[ -d $KDIR ]] || { echo "KDIR missing: $KDIR"; exit 1; }
[[ -f $FRAG ]] || { echo "fragment missing: $FRAG"; exit 1; }

# Pin assert: refuse to build from a source tree that drifted from the recorded
# pin (kernel/upstream/PIN) — an unpinned build is unreproducible by definition.
# PIN_OVERRIDE=1 builds anyway and prints what would have shipped.
PIN_FILE="$MONO/kernel/upstream/PIN"
if [[ -f $PIN_FILE ]]; then
  want_commit=$(sed -n 's/^UPSTREAM_COMMIT=//p' "$PIN_FILE")
  have_commit=$(git -C "$KDIR" rev-parse --short HEAD 2>/dev/null || echo unknown)
  if [[ "$have_commit" != "$want_commit" ]]; then
    echo "PIN MISMATCH: KDIR at $have_commit, pin says $want_commit ($PIN_FILE)"
    [[ "${PIN_OVERRIDE:-0}" == "1" ]] || { echo "set PIN_OVERRIDE=1 to build anyway, then update the pin"; exit 1; }
    echo "PIN_OVERRIDE=1 — building unpinned"
  fi
fi

export _pkgsuffix="$SUFFIX"
export _use_lto_suffix=no
export _use_gcc_suffix=no
export _processor_opt=generic_v3
export _HZ_ticks=250
export _hugepage=madvise
export _per_gov=yes
export _tickrate=idle
export _preempt=full
export _use_llvm_lto=thin
export _cc_harder=yes

export KCFLAGS="-march=${MARCH} -mtune=${MARCH}"
export KCPPFLAGS="-march=${MARCH} -mtune=${MARCH}"
unset CFLAGS CXXFLAGS LDFLAGS || true

cd "$KDIR"

if [[ -z $CFG_SRC ]]; then
  for c in config-*-appsynergy-server config-*-appsynergy config-*-cachyos-igpu config; do
    # shellcheck disable=SC2086
    if compgen -G "$c" >/dev/null; then
      CFG_SRC=$(ls -1t $c 2>/dev/null | head -1)
      break
    fi
  done
fi
[[ -n ${CFG_SRC:-} && -f $CFG_SRC ]] || { echo "No base kernel config in $KDIR"; exit 1; }

echo "==> flavor=$FLAVOR ($DESC)"
echo "==> base config: $CFG_SRC"
echo "==> fragment:    $FRAG"
echo "==> pkgbase:     linux-$SUFFIX"
echo "==> KCFLAGS:     $KCFLAGS"
echo "==> _processor_opt=$_processor_opt  (must NOT be empty/native)"

cp -a "$CFG_SRC" "$KDIR/config"

if [[ -d src/cachyos-* ]]; then
  TREE=$(ls -1d src/cachyos-* 2>/dev/null | head -1)
  if [[ -x $TREE/scripts/kconfig/merge_config.sh ]]; then
    echo "==> Merging fragment via $TREE/scripts/kconfig/merge_config.sh"
    (cd "$TREE" && ./scripts/kconfig/merge_config.sh -m ../../config "$FRAG" && cp -a .config ../../config)
  else
    echo "==> merge_config unavailable in tree; appending fragment markers"
    {
      echo ""
      echo "# --- appsynergy $FLAVOR fragment ---"
      cat "$FRAG"
    } >> config
  fi
else
  echo "==> No src tree yet; appending fragment to config for prepare"
  {
    echo ""
    echo "# --- appsynergy $FLAVOR fragment ---"
    cat "$FRAG"
  } >> config
fi

if grep -q '^CONFIG_X86_NATIVE_CPU=y' config 2>/dev/null; then
  echo "WARN: config still has X86_NATIVE_CPU=y — prepare will force generic_v3 via _processor_opt"
fi

echo "==> Building linux-$SUFFIX (long ThinLTO) → $LOG"
export PATH="${HOME}/.local/bin:${PATH}"
command -v bc >/dev/null || { echo "bc missing (install bc or put it on PATH)"; exit 1; }
makepkg -f --noconfirm --skippgpcheck --skipchecksums --nodeps 2>&1 | tee -a "$LOG"

BUILT_CFG=$(ls -1t config-*-"${SUFFIX}" 2>/dev/null | head -1 || true)
if [[ -n ${BUILT_CFG:-} ]]; then
  if grep -q '^CONFIG_X86_NATIVE_CPU=y' "$BUILT_CFG"; then
    echo "REFUSE: $BUILT_CFG has X86_NATIVE_CPU=y — wrong package, delete and rebuild" >&2
    exit 1
  fi
  echo "==> post-check OK: $(basename "$BUILT_CFG") has no X86_NATIVE_CPU"
  grep -E 'CONFIG_GENERIC_CPU|CONFIG_X86_64_VERSION|CONFIG_NR_CPUS|CONFIG_IGB|CONFIG_IGC' "$BUILT_CFG" | head -20
fi

mkdir -p "$OUT_REPO"
shopt -s nullglob
staged=()
for f in linux-${SUFFIX}-[0-9]*.pkg.tar.zst linux-${SUFFIX}-headers-[0-9]*.pkg.tar.zst; do
  [[ -f $f ]] || continue
  [[ $f == *dbg* ]] && continue
  cp -a "$f" "$OUT_REPO/"
  staged+=("$(basename "$f")")
  echo "    staged $(basename "$f") → $OUT_REPO"
done
shopt -u nullglob

if [[ -d $ISO_PKGS && ${#staged[@]} -gt 0 ]]; then
  for name in "${staged[@]}"; do
    cp -a "$OUT_REPO/$name" "$ISO_PKGS/"
    echo "    ISO payload: $name"
  done
fi

echo "Done $FLAVOR. Packages:"
ls -1 "$OUT_REPO"/linux-${SUFFIX}-*.pkg.tar.zst 2>/dev/null || true
