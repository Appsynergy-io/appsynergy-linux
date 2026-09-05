#!/usr/bin/bash
# Build any/ PKGBUILDs + assemble repo/x86_64 with repo-add.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REPO="$ROOT/repo/x86_64"
mkdir -p "$REPO"

# A published version is immutable. makepkg output is never byte-identical
# (BUILDINFO carries the build date and the host's package list), so rebuilding
# a pkgver-pkgrel that is already on the Release would re-sign and re-upload new
# bytes under the same name on every push to main. Instead, a version the
# published db already names is pulled back into staging as published and
# indexed as-is; new bytes ship by bumping pkgrel. REBUILD_PUBLISHED=1 forces a
# local build (dev only — publish-repo.sh would then clobber the published bytes).
SERVER_FILE="$ROOT/pacman/SERVER"
BASE="$(sed -n '1p' "$SERVER_FILE" 2>/dev/null | tr -d '[:space:]')"
declare -A PUBLISHED
if [[ -n "$BASE" && "${REBUILD_PUBLISHED:-0}" != "1" ]]; then
  _pub_db=$(mktemp)
  if curl -fsSL -o "$_pub_db" "$BASE/appsynergy.db.tar.gz" 2>/dev/null; then
    while IFS= read -r n; do PUBLISHED["$n"]=1; done < <(
      tar xzOf "$_pub_db" --wildcards '*/desc' 2>/dev/null | awk '/^%FILENAME%/{getline; print}')
    echo "==> published db names ${#PUBLISHED[@]} package(s); those versions are pulled, not rebuilt"
  else
    echo "==> no published db reachable at $BASE — building everything"
  fi
  rm -f "$_pub_db"
fi

build_pkg() {
  local dir="$1" f name
  # Every file this PKGBUILD would produce that is already published: pull the
  # published bytes (+sig) instead of building. -debug- names are listed but
  # never created, so they are ignored here as below.
  local -a want=() have=()
  while IFS= read -r f; do
    [[ "$f" != *-debug-* ]] && want+=("$(basename "$f")")
  done < <(cd "$dir" && makepkg --packagelist 2>/dev/null)
  for name in "${want[@]}"; do
    [[ -n "${PUBLISHED[$name]:-}" ]] && have+=("$name")
  done
  if ((${#want[@]} && ${#have[@]} == ${#want[@]})); then
    for name in "${have[@]}"; do
      echo "    pull published $name"
      curl -fsSL "$BASE/$name" -o "$REPO/$name"
      curl -fsSL "$BASE/$name.sig" -o "$REPO/$name.sig"
      touch "$REPO/$name.sig"   # newer than the package: the signing loop below leaves it alone
    done
    return 0
  fi
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

# The kernel stages itself. build-appsynergy-linux.sh copies straight into
# $REPO after asserting the built artifact against kernel/upstream/PIN, so there
# is nothing to scrape from a build tree here — and no build tree to scrape from,
# since it builds in a scratch dir and removes it.
if ! compgen -G "$REPO/appsynergy-linux-[0-9]*.pkg.tar.zst" > /dev/null; then
  echo "    note: no appsynergy-linux in staging — run packages/scripts/build-appsynergy-linux.sh"
fi

# Retired kernels must not be re-indexed: a client with the old package still
# installed would keep resolving updates for a kernel nobody builds.
for f in "$REPO"/linux-appsynergy-*.pkg.tar.zst "$REPO"/linux-cachyos-igpu-*.pkg.tar.zst; do
  [[ -f "$f" ]] || continue
  echo "    drop retired $(basename "$f")"
  rm -f "$f" "$f.sig"
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
# GPG_PASSPHRASE_FILE: CI holds a protected key; repo-add calls `gpg` on PATH,
# so wrap it when a passphrase file is set. Never print the file.
GPGKEY="${GPGKEY:-3B90D92D1E28E9E060D5C53D15D4351CF0D36AD1}"
if [[ -n "${GPG_PASSPHRASE_FILE:-}" ]]; then
  [[ -f "$GPG_PASSPHRASE_FILE" ]] || { echo "GPG_PASSPHRASE_FILE not a file"; exit 1; }
  _gpgwrap=$(mktemp -d)
  cat > "$_gpgwrap/gpg" <<EOF
#!/bin/bash
exec /usr/bin/gpg --pinentry-mode loopback --passphrase-file $(printf '%q' "$GPG_PASSPHRASE_FILE") "\$@"
EOF
  chmod 700 "$_gpgwrap/gpg"
  export PATH="$_gpgwrap:$PATH"
fi
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
