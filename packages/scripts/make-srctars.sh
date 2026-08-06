#!/usr/bin/bash
# Generate deterministic payload tarballs for the local-source PKGBUILDs.
#
# Why: makepkg copies local source=() entries into $srcdir BY BASENAME —
# duplicate basenames (appsynergy-linux.png x9 sizes) silently collide and drop
# files, and the name::path rename syntax only works for URLs (both verified
# 2026-08-06). A tarball preserves structure and gives makepkg one input it can
# checksum. Determinism (sort, epoch mtime, owner 0) means the same files always
# produce the same sha256, so the sums recorded in each PKGBUILD stay valid and
# any drift between loose files and recorded sums is a hard build failure.
#
# Tarballs are gitignored: loose files are canonical, this regenerates.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

mktar() {
  local dir="$1" out="$2"; shift 2
  tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner \
      -C "$ROOT/pkgbuilds/$dir" -cf "$ROOT/pkgbuilds/$dir/$out" "$@"
  printf '  %s  %s\n' "$(sha256sum "$ROOT/pkgbuilds/$dir/$out" | cut -d' ' -f1)" "$dir/$out"
}

echo "==> payload tarballs (sha256  path)"
mktar appsynergy-branding branding-payload.tar \
  os-release motd SHELL-POLICY.txt ascii bin fish profile.d skel
mktar appsynergy-branding-desktop branding-desktop-payload.tar \
  icons plymouth applications
mktar appsynergy-wallpapers wallpapers-payload.tar \
  metadata-desktop.json metadata-lock.json images
