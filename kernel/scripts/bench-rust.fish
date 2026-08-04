#!/usr/bin/env fish
# Controlled cargo + perf baseline/after measurement.
# Usage:
#   scripts/bench-rust.fish <project-dir> [label]
# Example:
#   scripts/bench-rust.fish ~/projects/combly cachyos-baseline
#   scripts/bench-rust.fish ~/projects/combly rustopt-after
#
# For A/B: same EPP performance, CARGO_INCREMENTAL=0, no RUSTC_WRAPPER,
# cargo clean before run. Compare wall_sec and cache miss rates in SUMMARY.

set -l proj $argv[1]
set -l label $argv[2]
if test -z "$proj"
    echo "Usage: "(status filename)" <project-dir> [label]" >&2
    exit 1
end
if test -z "$label"
    set label (uname -r | string replace -a '/' '_')
end
if not test -d $proj
    echo "Missing project: $proj" >&2
    exit 1
end

# Absolute: run_one pushd's into the project, so relative paths would break.
set -l root (realpath (dirname (status filename))/..)
# -g: run_one reads $B and $EVENTS; script-local vars are invisible in functions
set -g B $root/bench/(date +%Y%m%d-%H%M)-$label
mkdir -p $B

# Match baseline conditions
for g in /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference
    echo performance | sudo tee $g >/dev/null
end

set -e RUSTC_WRAPPER
set -e CARGO_TARGET_DIR
set -e RUSTFLAGS
set -x CARGO_INCREMENTAL 0

begin
    echo "timestamp="(date -Iseconds)
    echo "kernel="(uname -r)
    uname -a
    echo "governor="(cat /sys/devices/system/cpu/cpu0/cpufreq/scaling_governor)
    echo "epp="(cat /sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference)
    echo "THP="(cat /sys/kernel/mm/transparent_hugepage/enabled)
    echo "CARGO_INCREMENTAL=0 RUSTFLAGS unset RUSTC_WRAPPER unset"
    echo "ananicy="(systemctl is-active ananicy-cpp 2>/dev/null)
    rustc --version
    cargo --version
    perf version
    nproc
    free -h
end | tee $B/env.txt

# Hybrid CPU: atom+core split is fine; sum both for rates when comparing
set -g EVENTS task-clock,cycles,instructions,branches,branch-misses,cache-references,cache-misses,page-faults

function run_one
    set -l name $argv[1]
    set -l dir $argv[2]
    set -l rest $argv[3..-1]
    echo "===== $name @ "(date -Iseconds)" =====" | tee -a $B/run.log
    pushd $dir
    cargo clean 2>| tee -a $B/run.log
    /usr/bin/time -f 'wall_sec=%e user_sec=%U sys_sec=%S maxrss_kb=%M' -o $B/time-$name.txt \
        perf stat --no-big-num -e $EVENTS -o $B/perf-$name.txt -- $rest
    popd
    cat $B/time-$name.txt | tee -a $B/run.log
    cat $B/perf-$name.txt | tee -a $B/run.log
end

set -l J (nproc)
run_one release $proj cargo build --release -j$J
run_one check $proj cargo check -j$J

perf bench sched messaging -g 20 -l 5000 > $B/perf-bench-sched.txt 2>&1
perf bench mem memcpy -s 512MB -l 40 > $B/perf-bench-memcpy.txt 2>&1

begin
    echo "SUMMARY $label "(uname -r)
    for f in $B/time-*.txt
        echo "### "(basename $f)
        cat $f
    end
    echo "### sched"; cat $B/perf-bench-sched.txt
    echo "### memcpy"; cat $B/perf-bench-memcpy.txt
end | tee $B/SUMMARY.txt

echo "Wrote $B"
