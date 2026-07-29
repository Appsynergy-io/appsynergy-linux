#!/usr/bin/bash
# Root entrypoint for a clean ISO build. Removes dead work dirs, then builds.
set -uo pipefail

ROOT=/home/imma/projects/appsynergy-desktop
LOG=/tmp/appsynergy-iso-build-clean.log
export PATH="/usr/local/sbin:/usr/local/bin:/usr/bin:/usr/sbin"
export CLEAN=0
export OUT="$ROOT/out"
export WORK="$ROOT/work-iso-$(date +%Y%m%d-%H%M%S)"

exec > >(tee -a "$LOG") 2>&1

echo "=== BUILD START $(date -Is) ==="

# Refuse to touch anything still mounted.
if findmnt -rno TARGET | grep -q "^$ROOT/work-"; then
  echo "FATAL: mounts still present under $ROOT/work-*"
  findmnt -rno TARGET | grep "^$ROOT/work-"
  exit 1
fi

echo "==> removing dead work dirs"
for d in "$ROOT"/work-iso-*; do
  [[ -d "$d" ]] || continue
  [[ "$d" == "$WORK" ]] && continue
  echo "    rm -rf $d"
  rm -rf "$d"
done

mkdir -p "$OUT" "$WORK"
echo "==> free space: $(df -h "$ROOT" | tail -1 | awk '{print $4}')"

cd "$ROOT"
bash "$ROOT/scripts/build-iso.sh"
rc=$?

echo "=== BUILD END $(date -Is) rc=$rc ==="
if (( rc == 0 )); then
  echo "=== SUCCESS ==="
  ls -lh "$OUT"/*.iso
else
  echo "=== FAILED rc=$rc ==="
fi
exit $rc
