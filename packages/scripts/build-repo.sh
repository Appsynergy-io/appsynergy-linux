#!/usr/bin/bash
# Build any/ PKGBUILDs + assemble repo/x86_64 with repo-add.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$ROOT/repo/x86_64"
mkdir -p "$REPO"

build_pkg() {
  local dir="$1"
  # -d: these are file-shipping any/ packages; depends= are runtime-only and
  # need not be installed on the build host.
  (cd "$dir" && makepkg -f --noconfirm -c -d 2>/dev/null || makepkg -f --noconfirm -d)
  # Copy ONLY what this build produced (makepkg --packagelist), never a dir
  # glob: pkgbuild dirs accumulate stale rels, and a glob re-imports every one
  # of them into staging where repo-add then indexes an arbitrary version.
  # NB: --packagelist also names -debug- packages that are never created; an
  # `[[ -f ]] &&` guard as the loop's last command returns 1 there and set -e
  # kills the whole script silently. Use `if` (set -e exempt) and skip -debug-.
  while IFS= read -r f; do
    if [[ "$f" != *-debug-* && -f "$f" ]]; then
      cp -a "$f" "$REPO/"
    fi
  done < <(cd "$dir" && makepkg --packagelist 2>/dev/null)
}

echo "==> Regenerating payload tarballs (deterministic; sums pinned in PKGBUILDs)"
"$ROOT/scripts/make-srctars.sh"

echo "==> Building appsynergy any/ packages"
build_pkg "$ROOT/pkgbuilds/appsynergy-mirrorlist"
build_pkg "$ROOT/pkgbuilds/appsynergy-ca-certificates"
build_pkg "$ROOT/pkgbuilds/appsynergy-keyring"
build_pkg "$ROOT/pkgbuilds/appsynergy-branding"
build_pkg "$ROOT/pkgbuilds/appsynergy-wallpapers"
build_pkg "$ROOT/pkgbuilds/appsynergy-branding-desktop"

# Stage custom kernel if present on this machine
# (KDIR: host-local kernel build tree; kernel/upstream/PIN records its contract)
KDIR="${KDIR:-/home/imma/src/linux-cachyos/linux-cachyos}"
for f in \
  "$KDIR"/linux-appsynergy-[0-9]*.pkg.tar.zst \
  "$KDIR"/linux-appsynergy-headers-*.pkg.tar.zst \
  "$KDIR"/linux-cachyos-igpu-[0-9]*.pkg.tar.zst \
  "$KDIR"/linux-cachyos-igpu-headers-*.pkg.tar.zst
 do
  [[ -f "$f" ]] || continue
  [[ "$f" == *dbg* ]] && continue
  cp -a "$f" "$REPO/"
  echo "    staged $(basename "$f")"
done

# Prune stale rels before signing: staging only ever accumulates (build_pkg and
# the kernel loop both copy in, nothing removes), and `repo-add ./*.pkg.tar.zst`
# over two versions of one package indexes whichever the glob yielded last —
# incidental ordering, not a decision. A kernel rebuild is exactly this case:
# 7.1.5-2 and 7.1.5-3 sit side by side and the box gets whichever won the glob.
# Identity comes from .PKGINFO, never the filename: pkgnames contain dashes and
# version fields, so splitting the basename guesses wrong on the flavor kernels.
echo "==> Pruning superseded packages from staging"
declare -A keep_file keep_ver
for f in "$REPO"/*.pkg.tar.zst; do
  [[ -f "$f" ]] || continue
  info=$(bsdtar -xOqf "$f" .PKGINFO 2>/dev/null) || continue
  name=$(awk -F' = ' '$1=="pkgname"{print $2; exit}' <<<"$info")
  ver=$(awk -F' = ' '$1=="pkgver"{print $2; exit}' <<<"$info")
  [[ -n "$name" && -n "$ver" ]] || { echo "    SKIP unreadable .PKGINFO: $(basename "$f")"; continue; }
  if [[ -z "${keep_ver[$name]:-}" ]] || (( $(vercmp "$ver" "${keep_ver[$name]}") > 0 )); then
    if [[ -n "${keep_file[$name]:-}" ]]; then
      echo "    prune $(basename "${keep_file[$name]}") (superseded by $ver)"
      rm -f "${keep_file[$name]}" "${keep_file[$name]}.sig"
    fi
    keep_ver[$name]="$ver"; keep_file[$name]="$f"
  else
    echo "    prune $(basename "$f") (superseded by ${keep_ver[$name]})"
    rm -f "$f" "$f.sig"
  fi
done

# Signing key: fingerprint pinned in pkgbuilds/appsynergy-keyring. Every package
# gets a detached sig; repo-add --sign covers the database. SIGN=0 skips (dev).
GPGKEY="${GPGKEY:-3B90D92D1E28E9E060D5C53D15D4351CF0D36AD1}"
if [[ "${SIGN:-1}" == "1" ]]; then
  echo "==> Signing packages ($GPGKEY)"
  for f in "$REPO"/*.pkg.tar.zst; do
    # re-sign only when missing or stale (sig older than package)
    if [[ ! -f "$f.sig" || "$f" -nt "$f.sig" ]]; then
      gpg --batch --yes --detach-sign --no-armor -u "$GPGKEY" -o "$f.sig" "$f"
      echo "    signed $(basename "$f")"
    fi
  done
else
  echo "##############################################################"
  echo "# SIGN=0: staging is UNSIGNED (dev only)."
  echo "# publish-repo.sh will refuse it; ALLOW_UNSIGNED=1 overrides."
  echo "##############################################################"
fi

echo "==> repo-add"
cd "$REPO"
rm -f appsynergy.db* appsynergy.files*
if [[ "${SIGN:-1}" == "1" ]]; then
  repo-add -n --sign --key "$GPGKEY" appsynergy.db.tar.gz ./*.pkg.tar.zst
else
  repo-add -n appsynergy.db.tar.gz ./*.pkg.tar.zst
fi
# pacman also looks for appsynergy.db (symlink created by repo-add usually)
ls -lh
echo "Done. Staging dir: $REPO"
