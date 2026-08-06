#!/usr/bin/bash
# The release gate. Run before every commit/publish; CI runs exactly this.
# Fails fast, prints one PASS/FAIL line per stage, exit != 0 on any failure.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
fail=0
stage() { # name cmd...
  local name="$1"; shift
  if "$@" >/tmp/check-stage.log 2>&1; then
    echo "PASS  $name"
  else
    echo "FAIL  $name"; tail -20 /tmp/check-stage.log | sed 's/^/      /'; fail=1
  fi
}

# 1. Shell lint — release-critical scripts only (bench/backup host tools excluded)
stage shellcheck shellcheck -S error \
  "$ROOT"/packages/scripts/*.sh \
  "$ROOT"/scripts/check.sh \
  "$ROOT"/desktop/scripts/build-iso.sh \
  "$ROOT"/desktop/scripts/run-iso-build.sh \
  "$ROOT"/desktop/scripts/stage-rescue-payload.sh

# 2. Installer tests
stage cargo-test bash -c "cd '$ROOT/desktop/installer' && cargo test --locked"

# 3. Payload tarballs regenerate deterministically AND match the sums the
#    PKGBUILDs pin — drift means assets changed without updating the PKGBUILD.
check_sums() {
  "$ROOT"/packages/scripts/make-srctars.sh >/dev/null || return 1
  local p tar want have
  for p in appsynergy-branding appsynergy-branding-desktop appsynergy-wallpapers; do
    local d="$ROOT/packages/pkgbuilds/$p"
    tar=$(sed -n "s/^source=('\(.*\)')/\1/p" "$d/PKGBUILD")
    want=$(sed -n "s/^sha256sums=('\([0-9a-f]*\)')/\1/p" "$d/PKGBUILD")
    have=$(sha256sum "$d/$tar" | cut -d' ' -f1)
    [[ "$want" == "$have" ]] || { echo "sum drift in $p: PKGBUILD=$want actual=$have"; return 1; }
  done
}
stage tarball-sums check_sums

# 4. Every any/ package builds cleanly from its declared, checksummed sources
build_all() {
  local p
  for p in appsynergy-mirrorlist appsynergy-ca-certificates appsynergy-keyring \
           appsynergy-branding appsynergy-wallpapers appsynergy-branding-desktop; do
    (cd "$ROOT/packages/pkgbuilds/$p" && makepkg -f --noconfirm -d) || return 1
  done
}
stage makepkg-all build_all

# 5. namcap: package errors are failures, warnings informational.
#    Lint ONLY what the PKGBUILDs currently produce (makepkg --packagelist) —
#    a directory glob also lints stale rels from earlier builds and fails on
#    defects that are already fixed.
namcap_all() {
  local p f files=()
  for p in appsynergy-mirrorlist appsynergy-ca-certificates appsynergy-keyring \
           appsynergy-branding appsynergy-wallpapers appsynergy-branding-desktop; do
    while IFS= read -r f; do
      [[ -f "$f" ]] && files+=("$f")
    done < <(cd "$ROOT/packages/pkgbuilds/$p" && makepkg --packagelist 2>/dev/null)
  done
  ((${#files[@]} == 6)) || { echo "expected 6 built packages, found ${#files[@]}"; return 1; }
  local out
  out=$(namcap "${files[@]}" 2>&1)
  echo "$out"
  ! grep -q " E: " <<<"$out"
}
stage namcap namcap_all

# 6. Kernel pin still matches the build tree (skip silently if tree absent)
pin_check() {
  local kdir="${KDIR:-/home/imma/src/linux-cachyos}"
  [[ -d "$kdir/.git" ]] || return 0
  local want have
  want=$(sed -n 's/^UPSTREAM_COMMIT=//p' "$ROOT/kernel/upstream/PIN")
  have=$(git -C "$kdir" rev-parse --short HEAD)
  [[ "$want" == "$have" ]] || { echo "pin drift: PIN=$want tree=$have"; return 1; }
}
stage kernel-pin pin_check

# 7. Both server fragments keep the k3s bridge-netfilter pair. physdev is a
#    separate symbol that `depends on BRIDGE_NETFILTER`, and losing it fails
#    open: kube-router aborts every sync, so the apiserver accepts a
#    NetworkPolicy and nothing is programmed. Fragments only — a shipped
#    kernel/upstream/config-* legitimately lags until the next rebuild.
fragment_netfilter() {
  local f p
  for f in server-skylake server-tigerlake; do
    p="$ROOT/kernel/configs/$f.fragment"
    grep -qE '^CONFIG_BRIDGE_NETFILTER=[my]$' "$p" ||
      { echo "$f.fragment: missing CONFIG_BRIDGE_NETFILTER=m"; return 1; }
    grep -qE '^CONFIG_NETFILTER_XT_MATCH_PHYSDEV=m$' "$p" ||
      { echo "$f.fragment: missing CONFIG_NETFILTER_XT_MATCH_PHYSDEV=m"; return 1; }
  done
}
stage kernel-netfilter fragment_netfilter

exit $fail
