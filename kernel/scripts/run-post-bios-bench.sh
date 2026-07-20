#!/usr/bin/env bash
set -euo pipefail
B=/home/imma/projects/kernel/bench/20260712-post-bios-5600
BASE=/home/imma/projects/kernel/bench/20260712-baseline-cachyos
mkdir -p "$B"

for g in /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference; do
  echo performance | sudo tee "$g" >/dev/null
done
unset RUSTC_WRAPPER CARGO_TARGET_DIR RUSTFLAGS || true
export CARGO_INCREMENTAL=0

EVENTS="task-clock,cycles,instructions,branches,branch-misses,cache-references,cache-misses,page-faults"

{
  echo "timestamp=$(date -Iseconds)"
  echo "kernel=$(uname -r)"
  uname -a
  echo "governor=$(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)"
  echo "epp=$(cat /sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference)"
  echo "THP=$(cat /sys/kernel/mm/transparent_hugepage/enabled)"
  echo "CARGO_INCREMENTAL=0"
  echo "ananicy=$(systemctl is-active ananicy-cpp 2>/dev/null || true)"
  rustc --version
  cargo --version
  nproc
  free -h
  echo "BIOS=$(sudo dmidecode -s bios-version 2>/dev/null || true)"
  sudo dmidecode -t memory 2>/dev/null | awk '/Configured Memory Speed:/{print; exit}'
} | tee "$B/env.txt"

run_one() {
  local name="$1" dir="$2"
  shift 2
  echo "===== $name @ $(date -Iseconds) =====" | tee -a "$B/run.log"
  pushd "$dir" >/dev/null
  cargo clean 2>&1 | tail -3 | tee -a "$B/run.log"
  /usr/bin/time -f "wall_sec=%e user_sec=%U sys_sec=%S maxrss_kb=%M" -o "$B/time-$name.txt" \
    perf stat --no-big-num -e "$EVENTS" -o "$B/perf-$name.txt" -- \
      "$@"
  popd >/dev/null
  cat "$B/time-$name.txt" | tee -a "$B/run.log"
}

J=$(nproc)
run_one "combly-release" /home/imma/projects/combly cargo build --release -j"$J"
run_one "combly-check" /home/imma/projects/combly cargo check -j"$J"
run_one "beetv-release" /home/imma/projects/beetv-rs cargo build --release -j"$J"

perf bench sched messaging -g 20 -l 5000 > "$B/perf-bench-sched.txt" 2>&1
perf bench mem memcpy -s 512MB -l 40 > "$B/perf-bench-memcpy.txt" 2>&1

python3 - <<PY
from pathlib import Path
B = Path("$B")
BASE = Path("$BASE")
rows = []
for name in ["combly-release", "combly-check", "beetv-release"]:
    def wall(p):
        t = (p / f"time-{name}.txt").read_text().strip()
        for part in t.split():
            if part.startswith("wall_sec="):
                return float(part.split("=",1)[1])
        return None
    b, a = wall(BASE), wall(B)
    if b and a:
        d = 100.0 * (a - b) / b
        rows.append((name, b, a, d))

lines = []
lines.append("========================================")
lines.append("POST-BIOS 5600 vs BASELINE (~4000 MT/s)")
lines.append("========================================")
lines.append(f"{'workload':<22} {'before_s':>10} {'after_s':>10} {'delta%':>10}")
for name, b, a, d in rows:
    lines.append(f"{name:<22} {b:10.2f} {a:10.2f} {d:+9.1f}%")
lines.append("")
lines.append("negative delta% = faster after")
lines.append("")
for name, b, a, d in rows:
    lines.append(f"### {name}")
    lines.append((B / f"time-{name}.txt").read_text().strip())
    # cache miss rough rate from perf if present
    pf = B / f"perf-{name}.txt"
    if pf.exists():
        text = pf.read_text()
        # sum atom+core cache-misses / cache-references if present
        import re
        refs = [float(x.replace(",","")) for x in re.findall(r"([\d,]+)\s+cpu_\w+/cache-references", text)]
        misses = [float(x.replace(",","")) for x in re.findall(r"([\d,]+)\s+cpu_\w+/cache-misses", text)]
        if refs and misses and sum(refs) > 0:
            lines.append(f"cache_miss_rate~={100*sum(misses)/sum(refs):.1f}% (atom+core sum)")
    lines.append("")
lines.append("### micro after")
lines.append((B / "perf-bench-sched.txt").read_text())
lines.append((B / "perf-bench-memcpy.txt").read_text())
lines.append("### micro before")
lines.append((BASE / "perf-bench-sched.txt").read_text())
lines.append((BASE / "perf-bench-memcpy.txt").read_text())
out = "\n".join(lines)
(B / "COMPARE.txt").write_text(out)
(B / "SUMMARY.txt").write_text(out)
print(out)
print(f"\nWrote {B}")
PY
