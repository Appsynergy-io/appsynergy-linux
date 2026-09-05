#!/usr/bin/bash
# The release gate. Run before every commit/publish; CI runs exactly this.
# Fails fast, prints one PASS/FAIL line per stage, exit != 0 on any failure.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
fail=0
LOG="$(mktemp)"; trap 'rm -f "$LOG"' EXIT
stage() { # name cmd...
  local name="$1"; shift
  if "$@" >"$LOG" 2>&1; then
    echo "PASS  $name"
  else
    echo "FAIL  $name"; tail -20 "$LOG" | sed 's/^/      /'; fail=1
  fi
}

# 1. Shell lint — every shell script under the four script trees, discovered not
#    listed: a hand-maintained list silently omitted rescue-install.sh and
#    write-usb.sh, the two most destructive scripts in the repo. Discovery means
#    a new script is linted the day it lands. Extensionless PATH shims count.
shell_lint() {
  local f
  local -a scripts=()
  while IFS= read -r f; do
    [[ "$f" == *.sh ]] || head -c 64 "$f" | grep -qE '^#!.*\b(ba)?sh\b' || continue
    scripts+=("$f")
  done < <(find "$ROOT/desktop/scripts" "$ROOT/packages/scripts" "$ROOT/scripts" \
                "$ROOT/ci" -type f | sort)
  ((${#scripts[@]})) || { echo "no shell scripts found — bad discovery paths"; return 1; }
  shellcheck -S error "${scripts[@]}"
}
stage shellcheck shell_lint

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

# 6. The kernel pin is self-consistent. It is now the whole contract: the build
#    checks out UPSTREAM_COMMIT into a clean tree and refuses to stage anything
#    whose uname, ISA or module list disagrees with what is written here, so a
#    half-edited pin is a silent mis-build. Runs anywhere — no build tree needed.
pin_check() {
  local P="$ROOT/kernel/upstream/PIN" v
  [[ -f $P ]] || { echo "missing kernel/upstream/PIN"; return 1; }
  for v in UPSTREAM_REPO UPSTREAM_COMMIT UPSTREAM_FLAVOR PKGVER PKGREL PKGBASE KERNEL_UNAME; do
    grep -qE "^$v=." "$P" || { echo "PIN lacks $v"; return 1; }
  done
  local pkgver pkgrel pkgbase kuname
  pkgver=$(sed -n 's/^PKGVER=//p' "$P");   pkgrel=$(sed -n 's/^PKGREL=//p' "$P")
  pkgbase=$(sed -n 's/^PKGBASE=//p' "$P"); kuname=$(sed -n 's/^KERNEL_UNAME=//p' "$P")
  [[ "$kuname" == "$pkgver-$pkgrel-$pkgbase" ]] ||
    { echo "PIN: KERNEL_UNAME=$kuname but PKGVER/PKGREL/PKGBASE compose $pkgver-$pkgrel-$pkgbase"; return 1; }
  # Empty _processor_opt is upstream's default and means native autodetection —
  # a kernel welded to whichever CPU built it. It has already produced an Alder
  # Lake kernel for a Skylake Xeon once.
  grep -qE '^OPT__processor_opt=.+' "$P" ||
    { echo "PIN: OPT__processor_opt must be set explicitly, never empty"; return 1; }
  [[ -s "$ROOT/kernel/upstream/cachyos-source-keys.asc" ]] &&
    grep -q 'BEGIN PGP PUBLIC KEY BLOCK' "$ROOT/kernel/upstream/cachyos-source-keys.asc" ||
    { echo "PIN: cachyos-source-keys.asc missing or not a PGP key block"; return 1; }
  grep -qE '^SRCKEY=[0-9A-F]{40}$' "$P" || { echo "PIN lacks a 40-hex SRCKEY"; return 1; }
  grep -qE '^SRCSUM [^ ]+ [0-9a-f]{128}$' "$P" ||
    { echo "PIN: SRCSUM lines must carry a 128-hex b2sum"; return 1; }
  grep -q '^REQUIRE_MODULE ' "$P" || { echo "PIN lists no REQUIRE_MODULE"; return 1; }
}
stage kernel-pin pin_check

# 7. AppSynergy ships no kernel configuration. The kernel is upstream's, built
#    from upstream's committed config; the moment a fragment reappears we are
#    maintaining a fork again and the per-metal drift comes back with it. The
#    k3s invariants that fragments used to assert are now asserted against the
#    built artifact by the build script, and against its recorded config here.
kernel_no_fork() {
  local frags rc=0
  frags=$(find "$ROOT/kernel/configs" -name '*.fragment' 2>/dev/null || true)
  [[ -z "$frags" ]] || { echo "AppSynergy kernel config fragment(s) reappeared:"; echo "$frags"; rc=1; }
  # physdev `depends on BRIDGE_NETFILTER` but does not follow from it — separate
  # symbol. Losing it fails open: kube-router aborts every sync, so the apiserver
  # accepts a NetworkPolicy and programs nothing.
  local kuname cfg
  kuname=$(sed -n 's/^KERNEL_UNAME=//p' "$ROOT/kernel/upstream/PIN")
  cfg="$ROOT/kernel/upstream/config-$kuname"
  if [[ -f $cfg ]]; then
    local s
    for s in CONFIG_BRIDGE_NETFILTER CONFIG_NETFILTER_XT_MATCH_PHYSDEV CONFIG_NF_CONNTRACK_BRIDGE; do
      grep -qE "^$s=[my]$" "$cfg" || { echo "$(basename "$cfg"): $s not enabled"; rc=1; }
    done
  fi
  return $rc
}
stage kernel-nofork kernel_no_fork

# 8. Branding globs stay version-anchored. "appsynergy-branding-*" also matches
#    "appsynergy-branding-desktop-*": unanchored, it ships Plasma assets to
#    servers and makes the ISO prune delete the identity package. Code only —
#    comment lines quote the forbidden form to explain it.
branding_glob_anchor() {
  local hits
  hits=$(grep -rnE 'appsynergy-branding-(desktop-)?\*' \
           "$ROOT/desktop/scripts" "$ROOT/packages/scripts" |
         grep -vE '^[^:]+:[0-9]+:[[:space:]]*#')
  [[ -z "$hits" ]] || {
    echo "unanchored branding glob — use appsynergy-branding[-desktop]-[0-9]*:"
    echo "$hits"; return 1; }
}
stage branding-glob branding_glob_anchor

# 9. Every executable in the ISO profile is declared in profiledef.sh.
#    mkarchiso copies airootfs with `cp -af --no-preserve=ownership,mode`, so the
#    mode on disk is discarded and anything undeclared ships 0644. This hid for
#    releases: k3s and both LUKS-unlock scripts were non-executable in the live
#    image, and only worked on installed systems because the installer chmods
#    them on the target. Discovered, not listed — a new executable is caught the
#    day it lands. Secrets are asserted the other way: never world-readable.
profile_modes() {
  local prof="$ROOT/desktop/iso" rc=0 f p mode
  declare -A decl
  while IFS= read -r p; do decl["$p"]=1; done \
    < <(sed -n 's/.*\["\([^"]*\)"\]=.*/\1/p' "$prof/profiledef.sh")
  while IFS= read -r f; do
    p="${f#"$prof/airootfs"}"
    [[ -n "${decl[$p]:-}" ]] || { echo "undeclared executable ships 0644: $p"; rc=1; }
  done < <(find "$prof/airootfs" -type f -perm -u+x | sort)
  # A declared path that no longer exists is a stale entry, not a failure to
  # ship — but it means the array and the tree have drifted.
  for p in "${!decl[@]}"; do
    [[ -e "$prof/airootfs$p" ]] || echo "note: declared but absent from airootfs: $p"
  done
  # k3s.service.env carries runtime secrets; 0644 in a squashfs is world-readable
  # to anyone holding the USB.
  mode=$(sed -n 's|.*\["/opt/appsynergy/k3s/k3s.service.env"\]="0:0:\([0-7]*\)".*|\1|p' \
           "$prof/profiledef.sh")
  [[ "$mode" == "600" ]] || {
    echo "k3s.service.env must be declared 0:0:600 in profiledef.sh (got '${mode:-unset}')"; rc=1; }
  return $rc
}
stage profile-modes profile_modes

# 10. One Server URL. The published Release is the contract; a drifted
#     mirrorlist, iso fallback, or publish script ships a 404 to every host.
repo_url() {
  local want f
  want=$(sed -n '1p' "$ROOT/packages/pacman/SERVER" | tr -d '[:space:]')
  [[ -n "$want" ]] || { echo "packages/pacman/SERVER is empty"; return 1; }
  [[ "$want" == https://github.com/*/releases/download/* ]] || {
    echo "SERVER is not a GitHub Release URL: $want"; return 1; }
  grep -qxF "Server = $want" \
    "$ROOT/packages/pkgbuilds/appsynergy-mirrorlist/appsynergy-mirrorlist" || {
    echo "mirrorlist Server != packages/pacman/SERVER"; return 1; }
  grep -qxF "Server = $want" "$ROOT/desktop/iso/pacman.conf" || {
    echo "iso/pacman.conf remote Server != packages/pacman/SERVER"; return 1; }
  for f in publish-repo.sh verify-repo.sh fetch-repo.sh pull-kernel.sh; do
    grep -q 'pacman/SERVER' "$ROOT/packages/scripts/$f" || {
      echo "$f does not read packages/pacman/SERVER"; return 1; }
    if grep -qE 'git\.appsynergy\.io|GITEA_' "$ROOT/packages/scripts/$f"; then
      echo "$f still names Gitea"; return 1
    fi
  done
  return 0
}
stage repo-url repo_url

# 11. One workflow file, linted. GitHub treats every file under workflows/ as
#     its own pipeline with its own check, notification stream and bill; jobs
#     for different events live in ci.yml and skip with `if:`. actionlint
#     catches schema and shell errors; zizmor the supply-chain ones (unpinned
#     actions, persisted credentials). Ignores live in .github/zizmor.yml with
#     their reason — never here.
workflows() {
  local -a wf
  mapfile -t wf < <(find "$ROOT/.github/workflows" -maxdepth 1 -type f \( -name '*.yml' -o -name '*.yaml' \) | sort)
  [[ ${#wf[@]} -eq 1 && "$(basename "${wf[0]}")" == ci.yml ]] || {
    echo "exactly one workflow, .github/workflows/ci.yml, is allowed; found:"; printf '  %s\n' "${wf[@]}"; return 1; }
  # Explicit paths: actionlint's project autodetection fails inside the CI
  # container ("no project was found in any parent directories").
  actionlint "${wf[@]}" && zizmor --no-progress --config "$ROOT/.github/zizmor.yml" "$ROOT/.github/workflows"
}
stage workflows workflows

exit $fail
