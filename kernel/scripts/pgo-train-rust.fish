#!/usr/bin/env fish
# Representative Rust training load for kernel AutoFDO / perf record.
# Run under: perf record ... -- scripts/pgo-train-rust.fish

set -l projects appsynergy-rs beetv-rs combly keel frontier
set -l jobs (nproc)

for p in $projects
    set -l dir $HOME/projects/$p
    if not test -d $dir
        echo "skip missing $dir"
        continue
    end
    echo "=== train $p ==="
    pushd $dir
    cargo fetch
    cargo check -j$jobs
    cargo build --release -j$jobs
    # tests may fail; still good for kernel profiling
    cargo test --tests -j$jobs -- --test-threads=$jobs; or true
    popd
end

echo "Training suite finished"
