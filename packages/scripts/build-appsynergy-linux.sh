#!/usr/bin/bash
# Build `appsynergy-linux` — CachyOS's linux-cachyos-server, ThinLTO, renamed.
#
# AppSynergy contributes no kernel configuration. This script checks out the
# pinned upstream commit, builds that flavor's PKGBUILD against its own committed
# config, and changes exactly one thing: the name. Everything else — scheduler,
# tick rate, preemption, toolchain, driver surface — is upstream's decision.
#
# Everything the build depends on is set here explicitly rather than inherited
# from a default, because the defaults are where this goes wrong: upstream's
# `_processor_opt` is empty, meaning native autodetection, and a kernel
# autodetected on this 12900K workstation does not boot the Skylake Xeon.
#
# Contract: kernel/upstream/PIN. Nothing is staged unless the built artifact
# matches it.
set -euo pipefail

MONO="$(cd "$(dirname "$0")/../.." && pwd)"
PIN="$MONO/kernel/upstream/PIN"
SRCKEYS="$MONO/kernel/upstream/cachyos-source-keys.asc"
OUT_REPO="${OUT_REPO:-$MONO/packages/repo/x86_64}"
ISO_PKGS="${ISO_PKGS:-$MONO/desktop/iso/airootfs/opt/appsynergy/pkgs}"
# Local mirror of the upstream repo. Only ever read from, and only for committed
# objects — the build tree is a fresh extract, never this working copy.
SRC_CLONE="${SRC_CLONE:-$HOME/src/linux-cachyos}"

[[ -f $PIN ]] || { echo "missing pin: $PIN" >&2; exit 1; }
[[ -f $SRCKEYS ]] || { echo "missing source keys: $SRCKEYS" >&2; exit 1; }

pin() { sed -n "s/^$1=//p" "$PIN" | head -1; }
UPSTREAM_REPO=$(pin UPSTREAM_REPO)
COMMIT=$(pin UPSTREAM_COMMIT)
FLAVOR=$(pin UPSTREAM_FLAVOR)
PKGBASE=$(pin PKGBASE)
WANT_UNAME=$(pin KERNEL_UNAME)
for v in UPSTREAM_REPO COMMIT FLAVOR PKGBASE WANT_UNAME; do
  [[ -n ${!v} ]] || { echo "pin lacks $v" >&2; exit 1; }
done

# --- upstream source, at the pinned commit, from a clean tree -----------------
# Never build in $SRC_CLONE. It is a working copy with local modifications, and
# a pin that asserts only the commit says nothing about what is on top of it —
# which is precisely how the retired build scripts drifted.
if ! git -C "$SRC_CLONE" cat-file -e "$COMMIT^{commit}" 2>/dev/null; then
  echo "==> $COMMIT not in $SRC_CLONE; fetching $UPSTREAM_REPO"
  git -C "$SRC_CLONE" fetch --quiet origin || {
    echo "cannot fetch $UPSTREAM_REPO into $SRC_CLONE" >&2; exit 1; }
  git -C "$SRC_CLONE" cat-file -e "$COMMIT^{commit}" 2>/dev/null || {
    echo "pinned commit $COMMIT does not exist upstream" >&2; exit 1; }
fi

# Deliberately NOT mktemp's default. `/tmp` here is a 32G tmpfs with usrquota,
# and a ThinLTO kernel tree does not fit: the first attempt died twelve minutes
# in with "Disk quota exceeded" from clang's backend, which reads as a compiler
# bug rather than a full disk. Build on real disk, and check the space up front
# so the failure costs a second instead of a coffee break.
BUILD_ROOT="${BUILD_ROOT:-${XDG_CACHE_HOME:-$HOME/.cache}/appsynergy-kernel-build}"
mkdir -p "$BUILD_ROOT"
need_gib=60
avail_gib=$(df -BG --output=avail "$BUILD_ROOT" | tail -1 | tr -dc '0-9')
[[ ${avail_gib:-0} -ge $need_gib ]] || {
  echo "REFUSE: need ${need_gib}G free for a ThinLTO kernel build, $BUILD_ROOT has ${avail_gib:-0}G" >&2
  echo "        set BUILD_ROOT=/path/with/space to build elsewhere" >&2
  exit 1; }
WORK=$(mktemp -d "$BUILD_ROOT/build.XXXXXX")
cleanup() { [[ ${KEEP_WORK:-0} == 1 ]] && { echo "work tree kept: $WORK"; return; }; rm -rf "$WORK"; }
trap cleanup EXIT
echo "==> build root: $WORK (${avail_gib}G free)"

echo "==> Extracting $FLAVOR at $COMMIT (clean tree: $WORK)"
git -C "$SRC_CLONE" archive "$COMMIT:$FLAVOR" | tar -x -C "$WORK"
[[ -f $WORK/PKGBUILD && -f $WORK/config ]] || {
  echo "extract lacks PKGBUILD or config" >&2; exit 1; }

# --- the rename ---------------------------------------------------------------
# Two substitutions, both guarded on matching exactly once so upstream context
# drift fails the build instead of silently producing a differently-named kernel.
#
# The name is not a Kconfig symbol: prepare() writes it to localversion.20-pkgname,
# derived from pkgbase. Upstream computes `${pkgbase#linux}`, which only strips
# correctly while pkgbase starts with "linux" — ours does not, so the second edit
# derives it from $_pkgsuffix instead. That is a no-op for upstream's own values.
subst() { # file literal-from literal-to
  local n; n=$(grep -cFx -- "$2" "$1" || true)
  [[ $n == 1 ]] || { echo "REFUSE: expected exactly 1 match for the rename anchor, found $n:" >&2
                     echo "  $2" >&2; exit 1; }
  awk -v from="$2" -v to="$3" '$0 == from && !d { print to; d=1; next } { print }' \
    "$1" > "$1.new" && mv "$1.new" "$1"
}
subst "$WORK/PKGBUILD" 'pkgbase="linux-$_pkgsuffix"' \
                       "_pkgsuffix=\"${PKGBASE}\"
pkgbase=\"${PKGBASE}\""
subst "$WORK/PKGBUILD" '    echo "${pkgbase#linux}" > localversion.20-pkgname' \
                       '    echo "-${_pkgsuffix}" > localversion.20-pkgname'

# --- pin the sources upstream leaves unchecksummed -----------------------------
# _is_lto_kernel appends misc/dkms-clang.patch to source= without extending
# b2sums=, so makepkg aborts on the length mismatch. The URL is a raw `master`
# reference — mutable — and upstream's CI papers over it by regenerating sums,
# which is not verification. Append our pinned hash instead; it lands positionally
# because the LTO patch is the only conditional source these options enable.
while read -r name sum; do
  [[ -n $name && -n $sum ]] || continue
  echo "b2sums+=('$sum')" >> "$WORK/PKGBUILD"
  echo "==> pinned unchecksummed upstream source: $name"
done < <(sed -n 's/^SRCSUM //p' "$PIN")

# --- build options, all explicit ----------------------------------------------
while read -r k v; do
  [[ $k == OPT__* ]] || continue
  export "${k#OPT_}=$v"
done < <(sed -n 's/^\(OPT__[a-z_]*\)=\(.*\)/\1 \2/p' "$PIN")
echo "==> options: _processor_opt=${_processor_opt:-} _use_llvm_lto=${_use_llvm_lto:-}" \
     "_use_auto_optimization=${_use_auto_optimization:-}" \
     "_use_lto_suffix=${_use_lto_suffix:-} _use_gcc_suffix=${_use_gcc_suffix:-}"
[[ -n ${_processor_opt:-} ]] || { echo "REFUSE: _processor_opt is empty (native autodetect)" >&2; exit 1; }

# No KCFLAGS/KCPPFLAGS march injection: the ISA level is _processor_opt's job,
# and per-CPU marches are what made this two packages instead of one.
unset KCFLAGS KCPPFLAGS CFLAGS CXXFLAGS LDFLAGS || true

# Source signatures verified against the pinned keys in a build-local keyring,
# so makepkg needs neither --skippgpcheck nor a mutated operator keyring.
GNUPGHOME="$WORK/.gnupg"
export GNUPGHOME
mkdir -p "$GNUPGHOME"; chmod 700 "$GNUPGHOME"
gpg --quiet --batch --import "$SRCKEYS"
while read -r fpr; do
  gpg --batch --list-keys "$fpr" >/dev/null 2>&1 || {
    echo "REFUSE: $SRCKEYS lacks pinned source key $fpr" >&2; exit 1; }
done < <(sed -n 's/^SRCKEY=//p' "$PIN")

echo "==> Building $PKGBASE (ThinLTO; this is long)"
cd "$WORK"
makepkg -f --noconfirm --nodeps

# --- assert before staging ----------------------------------------------------
PKG=""
shopt -s nullglob
for f in "$WORK/$PKGBASE"-[0-9]*.pkg.tar.zst; do
  [[ $f == *-debug-* ]] && continue
  PKG="$f"; break
done
shopt -u nullglob
[[ -n $PKG ]] || { echo "REFUSE: no $PKGBASE package produced" >&2; exit 1; }

moddir=$(bsdtar -tf "$PKG" | sed -n 's|^usr/lib/modules/\([^/]*\)/$|\1|p' | head -1)
[[ "$moddir" == "$WANT_UNAME" ]] || {
  echo "REFUSE: built kernel is '$moddir', pin says '$WANT_UNAME'" >&2; exit 1; }
echo "    uname OK: $moddir"

cfg=$(ls -1t "$WORK"/config-*-"$PKGBASE" 2>/dev/null | head -1 || true)
[[ -n $cfg ]] || cfg="$WORK/config"
grep -q '^CONFIG_X86_NATIVE_CPU=y' "$cfg" && {
  echo "REFUSE: built config has X86_NATIVE_CPU=y — this kernel is tied to the build host" >&2
  exit 1; }
grep -q '^CONFIG_X86_64_VERSION=3' "$cfg" || {
  echo "REFUSE: built config is not x86-64-v3 (CONFIG_X86_64_VERSION=3 absent)" >&2; exit 1; }
echo "    ISA OK: x86-64-v3, no native-CPU pinning"

listing=$(bsdtar -tf "$PKG")
while read -r frag; do
  grep -qF -- "$frag" <<<"$listing" || {
    echo "REFUSE: package does not ship $frag" >&2; exit 1; }
  echo "    ships $frag"
done < <(sed -n 's/^REQUIRE_MODULE //p' "$PIN")

# --- stage --------------------------------------------------------------------
mkdir -p "$OUT_REPO"
shopt -s nullglob
staged=()
for f in "$WORK/$PKGBASE"-[0-9]*.pkg.tar.zst "$WORK/$PKGBASE"-headers-[0-9]*.pkg.tar.zst; do
  [[ $f == *-debug-* ]] && continue
  cp -a "$f" "$OUT_REPO/"
  # Never carry a stale signature alongside a fresh package: build-repo.sh signs
  # only where the .sig is missing or older, so a leftover would be published.
  rm -f "$OUT_REPO/$(basename "$f").sig"
  staged+=("$(basename "$f")")
  echo "    staged $(basename "$f")"
done
shopt -u nullglob
[[ ${#staged[@]} -gt 0 ]] || { echo "REFUSE: nothing staged" >&2; exit 1; }

if [[ -d $ISO_PKGS ]]; then
  for n in "${staged[@]}"; do cp -a "$OUT_REPO/$n" "$ISO_PKGS/"; echo "    ISO payload: $n"; done
fi

# Keep the built config as evidence of what shipped, next to the pin it satisfies.
cp -a "$cfg" "$MONO/kernel/upstream/config-$WANT_UNAME"
echo "Done. $WANT_UNAME staged in $OUT_REPO; run build-repo.sh to sign and index."
